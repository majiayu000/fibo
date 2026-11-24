//! On-demand sampling, independent of session recognition and workspace persistence.
use fibo_core::{
    diagnostics::{
        AppUsage, Diagnostics, FiboMemory, Memory, Pressure, TerminalUsage, UnplacedTerminal,
    },
    model::{Agent, ProcessState, Workspace},
    protocol::*,
};
use fibo_platform::local;
use regex::Regex;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{self, Read},
    process::{Command, Stdio},
    sync::LazyLock,
    time::{Duration, Instant},
};

const INTERVAL: Duration = Duration::from_millis(7500);
/// How long one ps or ioreg run may take before the sample gives up.
const COMMAND_WAIT: Duration = Duration::from_secs(20);

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

/// This sleeps in the kernel and wakes immediately on client cancellation. There is
/// no sampling timer or worker once this one request has returned.
fn wait(stream: &local::Stream, duration: Duration) -> io::Result<()> {
    if stream.wait_readable(duration)? {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "诊断已取消",
        ));
    }
    Ok(())
}

/// Drain stdout while the child runs, so a large process list cannot fill a pipe.
/// Only our own transient ps/ioreg child is killed on cancellation or timeout.
fn capture(command: &mut Command, stream: &local::Stream) -> io::Result<String> {
    command_output(command, stream, Some(COMMAND_WAIT))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn command_output(
    command: &mut Command,
    stream: &local::Stream,
    timeout: Option<Duration>,
) -> io::Result<Vec<u8>> {
    wait(stream, Duration::ZERO)?;
    let mut child = command
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let errors = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(e) => break Err(e),
            Ok(None) => (),
        }
        if timeout.is_some_and(|limit| started.elapsed() >= limit) {
            break Err(io::Error::new(io::ErrorKind::TimedOut, "系统采样命令超时"));
        }
        if let Err(e) = wait(stream, Duration::from_millis(10)) {
            break Err(e);
        }
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let bytes = reader
        .join()
        .map_err(|_| io::Error::other("读取采样输出的线程中断"));
    let stderr = errors
        .join()
        .map_err(|_| io::Error::other("读取采样输出的线程中断"));
    if !result?.success() {
        return Err(io::Error::other(format!(
            "采样命令失败：{}",
            String::from_utf8_lossy(&stderr??).trim()
        )));
    }
    bytes?
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

/// Swaps each readable process's RSS for its footprint; returns how many kept RSS.
fn read_footprints(processes: &mut HashMap<u32, Process>) -> usize {
    let mut kept = 0;
    for (&pid, process) in processes.iter_mut() {
        match fibo_platform::inspect::memory(pid) {
            Some(bytes) => process.memory = bytes,
            None => kept += 1,
        }
    }
    kept
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

fn memory() -> Option<Memory> {
    let m = fibo_platform::system::memory()?;
    Some(Memory {
        total_bytes: m.total,
        used_bytes: m.used,
        compressed_bytes: m.compressed,
        wired_bytes: m.wired,
        cached_bytes: m.cached,
        swap_bytes: m.swap,
        pressure: m.pressure.and_then(Pressure::from_level),
    })
}
fn snapshot(stream: &local::Stream, notes: &mut Vec<String>) -> io::Result<Snapshot> {
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
    let ticks = fibo_platform::system::cpu_ticks()
        .map_err(|e| notes.push(e.to_string()))
        .ok();
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
/// Unix peer credentials keep the existing Diagnose request readable by an older host.
/// A CLI probe or an SSH forward is not a local window.
pub(crate) fn window_pid(stream: &local::Stream) -> Option<u32> {
    let pid = stream.peer_pid()?;
    let executable = fibo_platform::inspect::executable(pid).ok()?;
    let name = std::path::Path::new(&executable).file_name()?.to_str()?;
    matches!(name, "fibo-app" | "mooring-app").then_some(pid)
}

#[derive(Default)]
pub(crate) struct TerminalFacts {
    usage: HashMap<u32, TerminalUsage>,
    unplaced: Vec<(u32, UnplacedTerminal)>,
}

pub(crate) fn terminal_facts(
    workspace: &Workspace,
    recognized: &HashMap<String, Agent>,
) -> TerminalFacts {
    let mut facts = TerminalFacts::default();
    for t in &workspace.terminals {
        let Some(pid) = t.pid.filter(|_| t.state == ProcessState::Running) else {
            continue;
        };
        let agent = workspace
            .current_agent(&t.id)
            .map(|s| s.runtime)
            .or_else(|| recognized.get(&t.id).copied())
            .or_else(|| workspace.agent_starts.get(&t.id).map(|s| s.agent));
        facts.usage.insert(
            pid,
            TerminalUsage {
                id: t.id.clone(),
                agent: agent.map(|agent| agent.executable().to_owned()),
                ..TerminalUsage::default()
            },
        );
        if workspace.terminal_tab(&t.id).is_none() {
            facts.unplaced.push((
                pid,
                UnplacedTerminal {
                    id: t.id.clone(),
                    // Arguments can include private prompts or credentials.
                    program: agent
                        .map(|a| a.executable().to_owned())
                        .or_else(|| t.argv.first().cloned())
                        .unwrap_or_default(),
                    cwd: workspace.terminal_cwd(t),
                    started_at: None,
                },
            ));
        }
    }
    facts
}

#[derive(PartialEq, Eq)]
enum Owner {
    Terminal(u32),
    Window,
    Host,
    Other,
}

fn owner(
    mut pid: u32,
    current: &Snapshot,
    terminals: &HashMap<u32, TerminalUsage>,
    window: Option<u32>,
    host: u32,
) -> Owner {
    let mut seen = HashSet::new();
    let mut is_window = Some(pid) == window;
    while seen.insert(pid) {
        let Some(process) = current.processes.get(&pid) else {
            break;
        };
        if terminals.contains_key(&pid) {
            return Owner::Terminal(pid);
        }
        if is_window {
            return Owner::Window;
        }
        if pid == host {
            return Owner::Host;
        }
        pid = process.parent;
        is_window = false;
    }
    Owner::Other
}

pub fn serve(
    stream: &mut local::Stream,
    window: Option<u32>,
    terminals: impl Fn() -> io::Result<TerminalFacts>,
) -> io::Result<()> {
    let mut report = Diagnostics {
        started_at: fibo_core::content::now(),
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
    let mut terminal_cpu: HashMap<String, f64> = HashMap::new();
    for interval in 1..=2 {
        let before = previous.as_ref().unwrap_or(&first);
        wait(stream, INTERVAL.saturating_sub(before.at.elapsed()))?;
        let current = snapshot(stream, &mut report.notes)?;
        let dt = current.at.duration_since(before.at).as_secs_f64();
        let mut interval_cpu: HashMap<String, f64> = HashMap::new();
        let mut cpu_deltas = HashMap::new();
        for (&pid, process) in &current.processes {
            if let Some(old) = same(before, &current, pid) {
                if let Some(delta) = (process.cpu_secs - old.cpu_secs)
                    .is_sign_positive()
                    .then_some(process.cpu_secs - old.cpu_secs)
                {
                    cpu_deltas.insert(pid, delta);
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
        let facts = terminals()?;
        report.unplaced = facts
            .unplaced
            .into_iter()
            .map(|(pid, mut terminal)| {
                terminal.started_at = fibo_platform::inspect::started(pid);
                terminal
            })
            .collect();
        let facts = facts.usage;
        let host = std::process::id();
        let window = window.filter(|&pid| same(&first, &current, pid).is_some());
        let mut fibo = FiboMemory::default();
        let mut by_terminal: BTreeMap<String, TerminalUsage> = BTreeMap::new();
        for (&pid, process) in &current.processes {
            let app = apps.entry(process.app.clone()).or_insert_with(|| AppUsage {
                name: process.app.clone(),
                ..AppUsage::default()
            });
            app.processes += 1;
            app.memory_bytes += process.memory;
            let bucket = match owner(pid, &current, &facts, window, host) {
                Owner::Terminal(root) => {
                    if first.processes.contains_key(&root) && same(&first, &current, root).is_none()
                    {
                        continue;
                    }
                    let fact = &facts[&root];
                    let usage = by_terminal
                        .entry(fact.id.clone())
                        .or_insert_with(|| fact.clone());
                    usage.memory_bytes += process.memory;
                    usage.processes += 1;
                    if let Some(delta) = cpu_deltas.get(&pid) {
                        *terminal_cpu.entry(fact.id.clone()).or_default() += delta;
                    }
                    if !app.terminals.contains(&fact.id) {
                        app.terminals.push(fact.id.clone());
                    }
                    continue;
                }
                Owner::Window => &mut fibo.window,
                Owner::Host => &mut fibo.host,
                Owner::Other => continue,
            };
            bucket.memory_bytes += process.memory;
            bucket.processes += 1;
        }
        report.fibo_memory = (fibo.window.processes > 0 && fibo.host.processes > 0).then_some(fibo);
        report.terminals = by_terminal
            .into_values()
            .map(|mut t| {
                t.cpu_percent = terminal_cpu
                    .get(&t.id)
                    .map(|secs| secs / report.elapsed_secs * 100.);
                t
            })
            .collect();
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
    fn window_host_and_pty_trees_do_not_overlap_even_in_the_same_bundle() {
        let start = "Wed Oct 28 10:30:00 2026 00:00.01 100";
        let current = Snapshot {
            at: Instant::now(),
            processes: parse_processes(&format!(
                "20 1 {start} /Fibo.app/Contents/MacOS/fibo-app\n21 20 {start} /Fibo.app/Contents/MacOS/fibo\n22 21 {start} /bin/hook\n23 21 {start} /bin/zsh\n24 23 {start} /bin/claude"
            )).unwrap(),
            ticks: None,
            gpu: Gpu::default(),
        };
        let terminals = HashMap::from([(23, TerminalUsage::default())]);
        let classify = |pid| owner(pid, &current, &terminals, Some(20), 21);
        assert!(classify(20) == Owner::Window);
        assert!(classify(21) == Owner::Host);
        assert!(classify(22) == Owner::Host);
        assert!(classify(23) == Owner::Terminal(23));
        assert!(classify(24) == Owner::Terminal(23));
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
        let (a, b) = local::Stream::pair().unwrap();
        drop(b);
        let start = Instant::now();
        assert_eq!(
            wait(&a, INTERVAL).unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn reused_pid_does_not_inherit_an_old_cpu_counter() {
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
    }

    /// Unix only: the child is /bin/sh and it is looked for with kill(pid, 0).
    #[test]
    #[cfg(unix)]
    fn cancelling_during_a_command_reaps_our_child() {
        let (a, b) = local::Stream::pair().unwrap();
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
