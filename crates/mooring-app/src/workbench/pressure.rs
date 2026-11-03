//! The machine's memory pressure for the status bar. macOS announces each change through a
//! dispatch memory-pressure source, so nothing polls: the bar shows a cell only while
//! pressure is above normal, and a click opens 本机诊断 with the memory row first.
use super::*;
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
};
use mooring_core::diagnostics::Pressure;
use std::{
    ffi::c_void,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicPtr, AtomicU8, Ordering},
    },
};

/// The last level macOS announced, as `kern.memorystatus_vm_pressure_level` spells it.
static LEVEL: AtomicU8 = AtomicU8::new(0);
#[cfg(feature = "visual-test")]
static FORCED: AtomicU8 = AtomicU8::new(0);
static SOURCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static WAKE: Mutex<Vec<UnboundedSender<()>>> = Mutex::new(Vec::new());
static STARTED: OnceLock<()> = OnceLock::new();

const NORMAL: usize = 0x1;
const WARN: usize = 0x2;
const CRITICAL: usize = 0x4;

unsafe extern "C" {
    static _dispatch_source_type_memorypressure: c_void;
    fn dispatch_source_create(
        kind: *const c_void,
        handle: usize,
        mask: usize,
        queue: *mut c_void,
    ) -> *mut c_void;
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut c_void;
    fn dispatch_source_set_event_handler_f(
        source: *mut c_void,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_source_get_data(source: *mut c_void) -> usize;
    fn dispatch_resume(object: *mut c_void);
}

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

extern "C" fn changed(_: *mut c_void) {
    let source = SOURCE.load(Ordering::Acquire);
    if source.is_null() {
        return;
    }
    let flags = unsafe { dispatch_source_get_data(source) };
    let level = if flags & CRITICAL != 0 {
        4
    } else if flags & WARN != 0 {
        2
    } else {
        1
    };
    LEVEL.store(level, Ordering::Release);
    if let Ok(mut senders) = WAKE.lock() {
        senders.retain(|tx| tx.unbounded_send(()).is_ok());
    }
}

/// Starts the one dispatch source on first use and redraws `cx`'s view on each change.
pub(super) fn watch(cx: &mut Context<Workbench>) {
    STARTED.get_or_init(|| {
        let level = sysctl::<i32>(c"kern.memorystatus_vm_pressure_level").unwrap_or(1);
        LEVEL.store(level.clamp(0, 4) as u8, Ordering::Release);
        unsafe {
            let source = dispatch_source_create(
                &raw const _dispatch_source_type_memorypressure,
                0,
                NORMAL | WARN | CRITICAL,
                dispatch_get_global_queue(0, 0),
            );
            if !source.is_null() {
                SOURCE.store(source, Ordering::Release);
                dispatch_source_set_event_handler_f(source, changed);
                dispatch_resume(source);
            }
        }
    });
    let (tx, mut rx) = unbounded();
    if let Ok(mut senders) = WAKE.lock() {
        senders.push(tx);
    }
    cx.spawn(async move |this, cx| {
        while rx.next().await.is_some() {
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Pressure above normal, with swap in use, or None while memory is fine.
pub(super) fn tight() -> Option<(Pressure, u64)> {
    #[cfg(feature = "visual-test")]
    let forced = FORCED.load(Ordering::Acquire);
    #[cfg(not(feature = "visual-test"))]
    let forced = 0;
    let level = if forced != 0 {
        forced
    } else {
        LEVEL.load(Ordering::Acquire)
    };
    let pressure = Pressure::from_level(i32::from(level)).filter(|p| *p != Pressure::Normal)?;
    let swap = sysctl::<libc::xsw_usage>(c"vm.swapusage").map_or(0, |s| s.xsu_used);
    Some((pressure, swap))
}

#[cfg(feature = "visual-test")]
pub(super) fn force(level: u8) {
    FORCED.store(level, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dispatch_source_starts_and_reads_a_level() {
        unsafe {
            let source = dispatch_source_create(
                &raw const _dispatch_source_type_memorypressure,
                0,
                NORMAL | WARN | CRITICAL,
                dispatch_get_global_queue(0, 0),
            );
            assert!(!source.is_null(), "memory-pressure source refused");
        }
        let level = sysctl::<i32>(c"kern.memorystatus_vm_pressure_level");
        assert!(matches!(level, Some(1 | 2 | 4)), "{level:?}");
    }
}
