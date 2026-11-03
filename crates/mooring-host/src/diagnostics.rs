//! On-demand sampling, independent of session recognition and workspace persistence.
use mooring_core::{
    diagnostics::{AppUsage, Diagnostics, Memory, Pressure, TerminalUsage},
    model::Terminal,
    protocol::*,
};
use regex::Regex;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{self, Read},
    os::{fd::AsRawFd, unix::net::UnixStream},
    process::{Command, Stdio},
    sync::LazyLock,
    time::{Duration, Instant},
};

const INTERVAL: Duration = Duration::from_millis(7500);

#[derive(Clone, Debug)]
struct Process {
    parent: u32,
    started: String,
    cpu_secs: f64,
    /// Physical footprint when this user may read it, else RSS.
    memory: u64,
    app: String,
}
#[derive(Default)]
struct Gpu {
    percent: Option<f64>,
    /// Registry entry identity prevents a newly created queue inheriting old time.
    clients: HashMap<(String, u32), u64>,
}
struct Snapshot {
    at: Instant,
    processes: HashMap<u32, Process>,
    ticks: Option<[u32; 4]>,
    gpu: Gpu,
}
#[derive(Default)]
struct Total {
    cpu_secs: f64,
    peak: f64,
    gpu: u64,
}

/// poll sleeps in the kernel and wakes immediately on client cancellation. There is
/// no sampling timer or worker once this one request has returned.
fn wait(stream: &UnixStream, duration: Duration) -> io::Result<()> {
    let mut fd = libc::pollfd {
        fd: stream.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let deadline = Instant::now() + duration;
    loop {
        let ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(i32::MAX as u128) as i32;
        let result = unsafe { libc::poll(&mut fd, 1, ms) };
        if result == 0 {
            return Ok(());
        }
        if result > 0 {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "诊断已取消",
            ));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// Drain stdout while the child runs, so a large process list cannot fill a pipe.
/// Only our own transient ps/ioreg child is killed on cancellation or timeout.
fn capture(command: &mut Command, stream: &UnixStream) -> io::Result<String> {
    wait(stream, Duration::ZERO)?;
    let mut child = command
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    let result = loop {
        match child.try_wait() {
            Ok(Some(_)) => break Ok(()),
            Err(e) => break Err(e),
            Ok(None) => (),
        }
        if Instant::now() >= deadline {
            break Err(io::Error::new(io::ErrorKind::TimedOut, "系统采样命令超时"));
        }
        if let Err(e) = wait(stream, Duration::from_millis(10)) {
            break Err(e);
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let output = child.wait_with_output();
    let bytes = reader
        .join()
        .map_err(|_| io::Error::other("读取采样输出的线程中断"))?;
    result?;
    let output = output?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "采样命令失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&bytes?).into_owned())
}

fn cpu_time(text: &str) -> Option<f64> {
    let (days, clock) = text
        .split_once('-')
        .map_or(Some((0., text)), |(d, t)| Some((d.parse::<f64>().ok()?, t)))?;
    let mut value = 0.;
    for part in clock.split(':') {
        value = value * 60. + part.parse::<f64>().ok()?;
    }
    Some(days * 86400. + value)
}
fn app_name(path: &str) -> String {
    if let Some((bundle, _)) = path.split_once(".app/") {
        return bundle.rsplit('/').next().unwrap_or(bundle).to_owned();
    }
    path.rsplit('/').next().unwrap_or(path).to_owned()
}
fn parse_processes(text: &str) -> io::Result<HashMap<u32, Process>> {
    let mut out = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let fields: Vec<_> = line.split_whitespace().collect();
        let parse = || -> Option<(u32, Process)> {
            if fields.len() < 10 {
                return None;
            }
            Some((
                fields[0].parse().ok()?,
                Process {
                    parent: fields[1].parse().ok()?,
                    started: fields[2..7].join(" "),
                    cpu_secs: cpu_time(fields[7])?,
                    memory: fields[8].parse::<u64>().ok()? * 1024,
                    app: app_name(&fields[9..].join(" ")),
                },
            ))
        };
        let (pid, process) = parse().ok_or_else(|| io::Error::other("无法解析系统进程快照"))?;
        out.insert(pid, process);
    }
    if out.is_empty() {
        return Err(io::Error::other("系统进程快照为空"));
    }
    Ok(out)
}

/// Activity Monitor's Memory column for one process. Only our own user's processes answer;
/// root and system daemons return EPERM.
fn footprint(pid: u32) -> Option<u64> {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V4,
            (&mut info as *mut libc::rusage_info_v4).cast(),
        )
    };
    (result == 0).then_some(info.ri_phys_footprint)
}
/// Swaps each readable process's RSS for its footprint; returns how many kept RSS.
fn read_footprints(processes: &mut HashMap<u32, Process>) -> usize {
    let mut kept = 0;
    for (&pid, process) in processes.iter_mut() {
        match footprint(pid) {
            Some(bytes) => process.memory = bytes,
            None => kept += 1,
        }
    }
    kept
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

fn parse_gpu(text: &str) -> Gpu {
    static DEVICE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""Device Utilization %"\s*=\s*(\d+)"#).unwrap());
    static ENTRY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"id (0x[0-9a-fA-F]+)").unwrap());
    static PID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""IOUserClientCreator"\s*=\s*"pid (\d+),"#).unwrap());
    static TIME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""accumulatedGPUTime"\s*=\s*(\d+)"#).unwrap());
    let mut gpu = Gpu {
        percent: DEVICE.captures(text).and_then(|c| c[1].parse().ok()),
        ..Gpu::default()
    };
    for block in text.split("+-o AGXDeviceUserClient").skip(1) {
        if let (Some(entry), Some(pid)) = (ENTRY.captures(block), PID.captures(block)) {
            if let Ok(pid) = pid[1].parse() {
                let time = TIME
                    .captures_iter(block)
                    .filter_map(|c| c[1].parse::<u64>().ok())
                    .sum();
                gpu.clients.insert((entry[1].to_owned(), pid), time);
            }
        }
    }
    gpu
}

#[allow(deprecated)] // libc exposes these Mach calls; no additional Mach dependency needed.
fn cpu_ticks() -> io::Result<[u32; 4]> {
    extern "C" {
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
fn memory() -> Option<Memory> {
    extern "C" {
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
        total_bytes: total,
        used_bytes: (app * page + wired + compressed).min(total),
        compressed_bytes: compressed,
        wired_bytes: wired,
        cached_bytes: (u64::from(info.external_page_count) + u64::from(info.purgeable_count))
            * page,
        swap_bytes: sysctl::<libc::xsw_usage>(c"vm.swapusage").map_or(0, |s| s.xsu_used),
        pressure: sysctl::<i32>(c"kern.memorystatus_vm_pressure_level")
            .and_then(Pressure::from_level),
    })
}
fn snapshot(stream: &UnixStream, notes: &mut Vec<String>) -> io::Result<Snapshot> {
    let mut processes = parse_processes(&capture(
        Command::new("/bin/ps").args(["-axo", "pid=,ppid=,lstart=,time=,rss=,comm="]),
        stream,
    )?)?;
    let kept = read_footprints(&mut processes);
    if kept > 0 {
        notes.push(format!(
            "内存按活动监视器的口径（含被压缩部分的原始大小，所以各应用合计会超过已用）；约 {} 个系统或其他用户的进程读不到，按 RSS 计。",
            kept / 10 * 10
        ));
    }
    let at = Instant::now();
    let gpu = match capture(
        Command::new("/usr/sbin/ioreg").args(["-r", "-l", "-c", "AGXAccelerator"]),
        stream,
    ) {
        Ok(text) => parse_gpu(&text),
        Err(error) if error.kind() == io::ErrorKind::ConnectionAborted => return Err(error),
        Err(error) => {
            notes.push(format!("GPU 数据不可读：{error}"));
            Gpu::default()
        }
    };
    let ticks = cpu_ticks().map_err(|e| notes.push(e.to_string())).ok();
    Ok(Snapshot {
        at,
        processes,
        ticks,
        gpu,
    })
}

/// Both samples must identify the same process, not merely the same numeric PID.
fn same<'a>(before: &'a Snapshot, after: &Snapshot, pid: u32) -> Option<&'a Process> {
    let old = before.processes.get(&pid)?;
    (old.started == after.processes.get(&pid)?.started).then_some(old)
}
fn terminal_for(
    pid: u32,
    first: &Snapshot,
    current: &Snapshot,
    terminals: &[Terminal],
) -> Option<String> {
    let mut pid = pid;
    let mut seen = HashSet::new();
    while seen.insert(pid) {
        let process = current.processes.get(&pid)?;
        if let Some(terminal) = terminals.iter().find(|t| t.pid == Some(pid)) {
            same(first, current, pid)?;
            return Some(terminal.id.clone());
        }
        pid = process.parent;
    }
    None
}

pub fn serve(stream: &mut UnixStream, terminals: &[Terminal]) -> io::Result<()> {
    let mut report = Diagnostics {
        started_at: mooring_core::content::now(),
        cpu_cores: std::thread::available_parallelism().ok().map(|n| n.get()),
        notes: vec![
            "CPU 只统计相邻采样均存在且身份不变的进程；短命进程可能遗漏。".into(),
            "GPU 仅覆盖驱动公开的计数；无数据不表示占用为零，活动份额不是设备利用率。".into(),
            "资源排行不能单独证明占用原因；共享 Agent 后台不按会话猜测分摊。".into(),
        ],
        ..Diagnostics::default()
    };
    let first = snapshot(stream, &mut report.notes)?;
    let mut previous = None;
    let mut totals: BTreeMap<String, Total> = BTreeMap::new();
    for interval in 1..=2 {
        let before = previous.as_ref().unwrap_or(&first);
        wait(stream, INTERVAL.saturating_sub(before.at.elapsed()))?;
        let current = snapshot(stream, &mut report.notes)?;
        let dt = current.at.duration_since(before.at).as_secs_f64();
        let mut interval_cpu: HashMap<String, f64> = HashMap::new();
        for (&pid, process) in &current.processes {
            if let Some(old) = same(before, &current, pid) {
                if let Some(delta) = (process.cpu_secs - old.cpu_secs)
                    .is_sign_positive()
                    .then_some(process.cpu_secs - old.cpu_secs)
                {
                    totals.entry(process.app.clone()).or_default().cpu_secs += delta;
                    *interval_cpu.entry(process.app.clone()).or_default() += delta;
                }
            }
        }
        for (app, secs) in interval_cpu {
            let total = totals.entry(app).or_default();
            total.peak = total.peak.max(secs / dt * 100.);
        }
        for (key, &time) in &current.gpu.clients {
            if let (Some(&old), Some(process), Some(_)) = (
                before.gpu.clients.get(key),
                current.processes.get(&key.1),
                same(before, &current, key.1),
            ) {
                totals.entry(process.app.clone()).or_default().gpu += time.saturating_sub(old);
            }
        }
        report.elapsed_secs = current.at.duration_since(first.at).as_secs_f64();
        report.complete = interval == 2;
        report.process_count = current.processes.len();
        report.gpu_percent = current.gpu.percent;
        report.memory = memory();
        report.cpu_percent = first.ticks.zip(current.ticks).and_then(|(a, b)| {
            let ticks: Vec<u64> = a
                .iter()
                .zip(b)
                .map(|(&a, b)| b.wrapping_sub(a) as u64)
                .collect();
            let all: u64 = ticks.iter().sum();
            (all > 0).then(|| (all - ticks[2]) as f64 / all as f64 * 100.)
        });
        let gpu_total: u64 = totals.values().map(|t| t.gpu).sum();
        let mut apps: BTreeMap<String, AppUsage> = totals
            .iter()
            .map(|(name, total)| {
                (
                    name.clone(),
                    AppUsage {
                        name: name.clone(),
                        cpu_percent: total.cpu_secs / report.elapsed_secs * 100.,
                        cpu_peak_percent: total.peak,
                        gpu_share_percent: (gpu_total > 0 && total.gpu > 0)
                            .then(|| total.gpu as f64 / gpu_total as f64 * 100.),
                        ..AppUsage::default()
                    },
                )
            })
            .collect();
        let mut by_terminal: BTreeMap<String, TerminalUsage> = BTreeMap::new();
        for (&pid, process) in &current.processes {
            let app = apps.entry(process.app.clone()).or_insert_with(|| AppUsage {
                name: process.app.clone(),
                ..AppUsage::default()
            });
            app.processes += 1;
            app.memory_bytes += process.memory;
            if let Some(id) = terminal_for(pid, &first, &current, terminals) {
                let usage = by_terminal
                    .entry(id.clone())
                    .or_insert_with(|| TerminalUsage {
                        id: id.clone(),
                        ..TerminalUsage::default()
                    });
                usage.memory_bytes += process.memory;
                usage.processes += 1;
                if !app.terminals.contains(&id) {
                    app.terminals.push(id);
                }
            }
        }
        report.terminals = by_terminal.into_values().collect();
        report.apps = apps.into_values().collect();
        report.apps.sort_by(|a, b| {
            b.cpu_percent
                .total_cmp(&a.cpu_percent)
                .then_with(|| a.name.cmp(&b.name))
        });
        report.notes.sort();
        report.notes.dedup();
        send(stream, CONTROL, &Reply::Diagnostics(report.clone()))?;
        previous = Some(current);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_reading_is_within_physical_memory() {
        let m = memory().expect("macOS answers hw.memsize and HOST_VM_INFO64");
        assert!(m.total_bytes >= 1 << 30);
        assert!(m.used_bytes > 0 && m.used_bytes <= m.total_bytes);
        assert!(m.compressed_bytes <= m.used_bytes);
        assert!(m.wired_bytes > 0 && m.wired_bytes + m.compressed_bytes <= m.used_bytes);
        assert!(
            m.pressure.is_some(),
            "kern.memorystatus_vm_pressure_level is readable"
        );
    }

    #[test]
    fn own_process_reads_its_footprint() {
        let me = std::process::id();
        let bytes = footprint(me).expect("our own process answers proc_pid_rusage");
        assert!(bytes > 1 << 20);
        // launchd belongs to root: unreadable, so the RSS from ps stays.
        assert_eq!(footprint(1), None);
    }

    #[test]
    fn process_snapshot_keeps_bundle_spaces_start_identity_and_long_cpu_time() {
        let p = parse_processes("42 1 Wed Oct 28 10:30:00 2026 123:04.50 2048 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n").unwrap();
        assert_eq!(p[&42].app, "Google Chrome");
        assert_eq!(p[&42].memory, 2_097_152);
        assert_eq!(p[&42].cpu_secs, 7384.5);
        assert_eq!(cpu_time("1-02:03:04.50"), Some(93784.5));
        assert!(parse_processes("unexpected format").is_err());
    }
    #[test]
    fn gpu_queues_are_identified_separately_and_missing_is_not_zero() {
        let gpu = parse_gpu(
            r#""PerformanceStatistics" = {"Device Utilization %"=24}
+-o AGXDeviceUserClient <class AGXDeviceUserClient, id 0x123, active>
 "AppUsage" = ({"accumulatedGPUTime"=12},{"accumulatedGPUTime"=8})
 "IOUserClientCreator" = "pid 42, Chrome Helper"
+-o AGXDeviceUserClient <class AGXDeviceUserClient, id 0x456, active>
 "AppUsage" = ()
 "IOUserClientCreator" = "pid 42, Chrome Helper"
"#,
        );
        assert_eq!(gpu.percent, Some(24.));
        assert_eq!(gpu.clients[&("0x123".into(), 42)], 20);
        assert_eq!(gpu.clients.len(), 2);
        assert_eq!(parse_gpu("").percent, None);
    }
    #[test]
    fn cancellation_interrupts_the_wait_without_a_refresh_poll() {
        let (a, b) = UnixStream::pair().unwrap();
        drop(b);
        let start = Instant::now();
        assert_eq!(
            wait(&a, INTERVAL).unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn reused_pid_is_not_counted_or_attributed_to_the_old_terminal() {
        let before = Snapshot {
            at: Instant::now(),
            processes: parse_processes("42 1 Wed Oct 28 10:30:00 2026 12:00.00 100 /bin/sh")
                .unwrap(),
            ticks: None,
            gpu: Gpu::default(),
        };
        let after = Snapshot {
            at: Instant::now(),
            processes: parse_processes("42 1 Wed Oct 28 11:30:00 2026 00:01.00 100 /bin/sh")
                .unwrap(),
            ticks: None,
            gpu: Gpu::default(),
        };
        assert!(same(&before, &after, 42).is_none());
        let terminal = Terminal {
            id: "old-shell".into(),
            space: "s".into(),
            argv: vec![],
            cwd: "/tmp".into(),
            pid: Some(42),
            state: mooring_core::model::ProcessState::Running,
            exit_code: None,
            error: None,
        };
        assert!(terminal_for(42, &before, &after, &[terminal]).is_none());
    }

    #[test]
    fn cancelling_during_a_command_reaps_our_child() {
        let (a, b) = UnixStream::pair().unwrap();
        let root =
            std::env::temp_dir().join(format!("kl-diagnostic-cancel-{}", uuid::Uuid::new_v4()));
        // Written aside and renamed: `>` creates the file empty before the pid lands in it.
        let script = format!(
            "echo $$ > '{0}.tmp'; mv '{0}.tmp' '{0}'; exec /bin/sleep 20",
            root.display()
        );
        let path = root.clone();
        let task =
            std::thread::spawn(move || capture(Command::new("/bin/sh").args(["-c", &script]), &a));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !path.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid: i32 = std::fs::read_to_string(&path)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        drop(b);
        assert_eq!(
            task.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "sampling child was left alive"
        );
        std::fs::remove_file(path).unwrap();
    }
}
