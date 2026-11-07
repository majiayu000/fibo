use super::Memory;
use std::io;

fn sysctl<T: Copy>(name: &std::ffi::CStr) -> Option<T> {
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<T>();
    let read = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut T).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (read == 0 && len == std::mem::size_of::<T>()).then_some(value)
}

#[allow(deprecated)] // libc exposes these Mach calls; no additional Mach dependency needed.
pub fn cpu_ticks() -> io::Result<[u32; 4]> {
    unsafe extern "C" {
        fn mach_port_deallocate(task: u32, name: u32) -> i32;
    }
    let mut info: libc::host_cpu_load_info = unsafe { std::mem::zeroed() };
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    let result = unsafe {
        let host = libc::mach_host_self();
        let result = libc::host_statistics(
            host,
            libc::HOST_CPU_LOAD_INFO,
            (&mut info as *mut libc::host_cpu_load_info).cast(),
            &mut count,
        );
        mach_port_deallocate(libc::mach_task_self(), host);
        result
    };
    if result != 0 {
        return Err(io::Error::other(format!("整机 CPU 统计不可读（{result}）")));
    }
    Ok(info.cpu_ticks)
}
/// Physical memory the way Activity Monitor adds it up: app memory (anonymous pages that
/// cannot be purged), wired and compressed. None when the kernel does not answer.
#[allow(deprecated)]
pub fn memory() -> Option<Memory> {
    unsafe extern "C" {
        fn mach_port_deallocate(task: u32, name: u32) -> i32;
    }
    let total: u64 = sysctl(c"hw.memsize").filter(|&t| t > 0)?;
    let mut info: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let result = unsafe {
        let host = libc::mach_host_self();
        let result = libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            (&mut info as *mut libc::vm_statistics64).cast(),
            &mut count,
        );
        mach_port_deallocate(libc::mach_task_self(), host);
        result
    };
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if result != 0 || page <= 0 {
        return None;
    }
    let page = page as u64;
    let app = u64::from(
        info.internal_page_count
            .saturating_sub(info.purgeable_count),
    );
    let compressed = u64::from(info.compressor_page_count) * page;
    let wired = u64::from(info.wire_count) * page;
    Some(Memory {
        total,
        used: (app * page + wired + compressed).min(total),
        compressed,
        wired,
        cached: (u64::from(info.external_page_count) + u64::from(info.purgeable_count)) * page,
        swap: sysctl::<libc::xsw_usage>(c"vm.swapusage").map_or(0, |s| s.xsu_used),
        pressure: sysctl::<i32>(c"kern.memorystatus_vm_pressure_level"),
    })
}

pub fn host_name() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: the kernel writes at most `buf.len()` bytes, NUL-terminated when it fits.
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::memory;

    #[test]
    fn macos_answers_every_part_of_memory() {
        let m = memory().expect("macOS answers hw.memsize and HOST_VM_INFO64");
        assert!(m.compressed <= m.used);
        assert!(m.wired > 0 && m.wired + m.compressed <= m.used);
        assert!(
            m.pressure.is_some(),
            "kern.memorystatus_vm_pressure_level is readable"
        );
    }
}
