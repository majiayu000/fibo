use super::*;

#[test]
fn forty_idle_terminals_have_no_periodic_background_io() {
    let h = Host::measured();
    h.setup();
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    // Same runtime and watched log directory, but outside every managed PTY.
    struct Outside(Child);
    impl Drop for Outside {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let external=Outside(Command::new(&cli).args(["-c", &format!(
        "exec 3>'{}'; printf '%s\\n' '{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"external\",\"cwd\":\"/tmp\"}}}}' >&3; read -r line",logs.join("external.jsonl").display())])
        .stdin(Stdio::piped()).spawn().unwrap());
    for i in 0..40 {
        let id = format!("idle-{i}");
        let script = format!(
            "exec 3>\"{}\"; printf \"%s\\n\" \"{{\\\"type\\\":\\\"session_meta\\\",\\\"payload\\\":{{\\\"id\\\":\\\"{id}\\\",\\\"cwd\\\":\\\"/tmp\\\"}}}}\" >&3; read -r line",
            logs.join(format!("{id}.jsonl")).display()
        );
        let command = format!("exec '{}' -c '{}'", cli.display(), script);
        h.call(h.launch(&id, &command)).unwrap();
    }
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        let workspace = h.workspace();
        if workspace
            .agent_sessions
            .iter()
            .filter(|s| s.current)
            .count()
            == 40
        {
            assert!(workspace
                .agent_sessions
                .iter()
                .filter(|s| s.current)
                .all(|s| s.terminal.is_some() && s.cli_session_id != "external"));
            let mut processes = mooring_host::recognition::processes(&workspace).unwrap();
            assert!(!processes.iter().any(|p| p.pid == external.0.id()));
            // Even if passed an unrelated Agent PID, the lsof boundary filters it out.
            processes.push(mooring_host::recognition::Process {
                pid: external.0.id(),
                parent: std::process::id(),
                runtime: Some(Agent::Codex),
                cli_session_id: None,
                no_daemon: false,
                launch: vec![],
            });
            assert!(
                !mooring_host::recognition::open_transcripts(&workspace, &processes)
                    .unwrap()
                    .contains_key(&external.0.id())
            );
            break;
        }
        assert!(
            Instant::now() < until,
            "automatic subscription associated only {} of 40 sessions",
            workspace.agent_sessions.len()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    // Let startup FSEvents batches and PTY setup finish before the measurement window.
    std::thread::sleep(Duration::from_secs(2));
    let metrics_before = recognition(&h).metrics;
    fn usage(pid: u32) -> libc::rusage_info_v2 {
        unsafe {
            let mut r: libc::rusage_info_v2 = std::mem::zeroed();
            assert_eq!(
                libc::proc_pid_rusage(
                    pid as i32,
                    libc::RUSAGE_INFO_V2,
                    (&mut r as *mut libc::rusage_info_v2).cast()
                ),
                0
            );
            r
        }
    }
    fn task(pid: u32) -> libc::proc_taskinfo {
        unsafe {
            let mut t: libc::proc_taskinfo = std::mem::zeroed();
            assert_eq!(
                libc::proc_pidinfo(
                    pid as i32,
                    libc::PROC_PIDTASKINFO,
                    0,
                    (&mut t as *mut libc::proc_taskinfo).cast(),
                    std::mem::size_of_val(&t) as i32
                ),
                std::mem::size_of_val(&t) as i32
            );
            t
        }
    }
    // pti_syscalls_unix includes native FSEvents/libdispatch workqueue housekeeping,
    // not just file I/O. LLDB captured __workq_kernreturn after a filtered native
    // notification; parallel runs reproduced +2 with no application work. Allow
    // at most two such pairs, but keep our log/scan/index/probe counters EXACTLY zero.
    // We cannot retroactively name the user's two calls from an aggregate counter.
    // A full workspace run writes files beside this root the whole time and its filtered
    // notifications reached +9; periodic work would show in every window, so one clean
    // window out of three is enough. Disk I/O stays exactly zero in each window.
    let mut deltas = Vec::new();
    let cpu_percent = loop {
        let before = usage(h.child.id());
        let calls = task(h.child.id());
        let start = Instant::now();
        std::thread::sleep(Duration::from_secs(5));
        let after = usage(h.child.id());
        let end_calls = task(h.child.id());
        let cpu_ticks =
            after.ri_user_time + after.ri_system_time - before.ri_user_time - before.ri_system_time;
        let cpu_percent = cpu_seconds(cpu_ticks) / start.elapsed().as_secs_f64() * 100.;
        let extra_calls = end_calls.pti_syscalls_unix - calls.pti_syscalls_unix;
        eprintln!(
            "40 idle PTYs: CPU {cpu_percent:.4}%, disk read {} B, write {} B, Unix syscalls {extra_calls}",
            after.ri_diskio_bytesread - before.ri_diskio_bytesread,
            after.ri_diskio_byteswritten - before.ri_diskio_byteswritten,
        );
        assert_eq!(after.ri_diskio_bytesread, before.ri_diskio_bytesread);
        assert_eq!(after.ri_diskio_byteswritten, before.ri_diskio_byteswritten);
        deltas.push(extra_calls);
        if (0..=4).contains(&extra_calls) || deltas.len() == 3 {
            break cpu_percent;
        }
    };
    assert!(
        deltas.iter().any(|d| (0..=4).contains(d)),
        "unexpected idle Unix syscall deltas {deltas:?}"
    );
    assert_eq!(
        recognition(&h).metrics,
        metrics_before,
        "idle must not read logs, walk directories, write the index or spawn probes"
    );
    assert!(cpu_percent < 0.5, "idle CPU {cpu_percent}% exceeds 0.5%");
    assert_eq!(h.workspace().terminals.len(), 40);
}

#[test]
fn four_agents_write_for_thirty_seconds_without_repeated_process_probes() {
    let h = Host::measured();
    h.setup();
    let control = Host::with_lease(FixtureLease::Companion);
    control.setup();
    // Average over a longer, bounded window under scheduler pressure. Writers
    // retain 10 Hz each and the budget remains 2% of one core, never a relaxed rate.
    let mut load = [0.; 1];
    assert_eq!(unsafe { libc::getloadavg(load.as_mut_ptr(), 1) }, 1);
    let cores = std::thread::available_parallelism().unwrap().get() as f64;
    let scale = (load[0] / cores).ceil().clamp(1., 3.) as u64;
    let window = 30 * scale;
    let logs = h.root.join("logs/codex");
    fs::create_dir_all(&logs).unwrap();
    let cli = h.root.join("codex");
    std::os::unix::fs::symlink("/bin/sh", &cli).unwrap();
    // Relative sleeps lose samples under load; keep a fixed workload on absolute deadlines.
    for i in 0..4 {
        let id = format!("load-{i}");
        let script = h.root.join(format!("{id}.sh"));
        fs::write(
            &script,
            format!(
                r#"exec 3>'{}'
printf '%s\n' '{{"type":"session_meta","payload":{{"id":"{id}","cwd":"/tmp"}}}}' >&3
read -r start
LOAD_LOG='{}' HOLD_OPEN='{}' /usr/bin/python3 - <<'PY'
import os,time
start=time.monotonic()
for sample in range({window}*10):
    data=b'{{"type":"event_msg","payload":{{"type":"agent_message","message":"working"}}}}\n'
    if os.environ['HOLD_OPEN']=='yes':
        os.write(3,data)
    else:
        with open(os.environ['LOAD_LOG'],'ab') as log:
            log.write(data)
    time.sleep(max(0,start+(sample+1)/10-time.monotonic()))
PY
printf done >'{}'
read -r finish
"#,
                logs.join(format!("{id}.jsonl")).display(),
                logs.join(format!("{id}.jsonl")).display(),
                if i < 2 { "yes" } else { "no" },
                h.root.join(format!("{id}.done")).display()
            ),
        )
        .unwrap();
        h.call(h.launch(
            &id,
            &format!("exec '{}' '{}'", cli.display(), script.display()),
        ))
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while h
        .workspace()
        .agent_sessions
        .iter()
        .filter(|s| s.current)
        .count()
        != 4
    {
        assert!(
            Instant::now() < deadline,
            "automatic association did not finish"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(1));
    // Connection setup and teardown are not part of incremental log processing.
    // Keep all streams alive until after the sampled interval.
    let mut streams: Vec<_> = (0..4)
        .map(|i| h.attach(&format!("load-{i}"), true).0)
        .collect();
    let before = recognition(&h);
    let cpu = usage(h.child.id());
    let control_cpu = usage(control.child.id());
    let started = Instant::now();
    for stream in &mut streams {
        send(
            stream,
            CONTROL,
            &Request::Input {
                bytes: b"go\n".to_vec(),
                lossy: false,
            },
        )
        .unwrap();
        assert!(matches!(ack(stream), Reply::Ok));
    }
    // Sleep only the driver; neither daemon has a periodic sampler/timer.
    std::thread::sleep(Duration::from_secs(window + 1));
    let done_deadline = started + Duration::from_secs(window + 30);
    while (0..4).any(|i| !h.root.join(format!("load-{i}.done")).exists()) {
        assert!(
            Instant::now() < done_deadline,
            "writers did not finish under load"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut appended = 0;
    for i in 0..4 {
        assert!(h.root.join(format!("load-{i}.done")).exists());
        let text = fs::read_to_string(logs.join(format!("load-{i}.jsonl"))).unwrap();
        appended += (text.len() - text.find('\n').unwrap() - 1) as u64;
        let lines = text.lines().count();
        assert_eq!(lines as u64, 1 + window * 10, "writer {i} workload");
    }
    // Writer completion precedes the asynchronous file event. Include its final
    // consumption in the CPU sample, within the existing workload deadline.
    let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
    for i in 0..4 {
        let path = logs.join(format!("load-{i}.jsonl")).canonicalize().unwrap();
        let end = fs::metadata(&path).unwrap().len();
        let offset = loop {
            let offset: i64 = db
                .query_row(
                    "SELECT offset FROM log_files WHERE path=?1",
                    [path.to_str().unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            if offset as u64 == end || Instant::now() >= done_deadline {
                break offset;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(offset as u64, end, "subscriber cursor for writer {i}");
    }
    let after = usage(h.child.id());
    let control_after = usage(control.child.id());
    let stats = recognition(&h);
    // Gated on retired instructions per appended line, not CPU time, which moves with
    // machine load (649-666 k a line at load 31-50 on 2026-11-04, about 1.3% of a core).
    // A VM (CI) counts none and its CPU time tracks load: there appended bytes are read once.
    let seconds = cpu_seconds(
        after.ri_user_time + after.ri_system_time - cpu.ri_user_time - cpu.ri_system_time,
    );
    let idle = control_after.ri_instructions - control_cpu.ri_instructions;
    let per_line = (after.ri_instructions - cpu.ri_instructions).saturating_sub(idle)
        / (4 * (1 + window * 10));
    let ps = stats.metrics.ps_spawns - before.metrics.ps_spawns;
    let lsof = stats.metrics.lsof_spawns - before.metrics.lsof_spawns;
    eprintln!(
        "4 agents / {window}s (load {load:?}, {cores} cores): daemon CPU {seconds:.4}s, {per_line} instructions a line, idle control {idle}, ps spawns {ps}, lsof spawns {lsof}, incremental bytes {} of {appended}",
        stats.metrics.log_bytes - before.metrics.log_bytes
    );
    assert_eq!((ps, lsof), (0, 0), "transcript appends spawned probes");
    assert!(idle <= 50_000_000, "idle control busy: {idle}");
    let within = match after.ri_instructions {
        0 => stats.metrics.log_bytes - before.metrics.log_bytes <= appended + appended / 10,
        _ => per_line <= 1_000_000,
    };
    assert!(within, "daemon over its budget, see the line above");
    assert!(
        stats.metrics.log_bytes - before.metrics.log_bytes > 80_000 * scale,
        "the subscriber must actually consume the workload"
    );
}
#[test]
fn recognition_status_stream_recovers_after_log_root_is_rebuilt() {
    let mut h = Host::start();
    let root = h.root.join("logs/codex");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("before.jsonl"),
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"before\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(1));
    // Reopen with existing roots so this exercises an actual WatchRoot stream, not only its ancestor.
    h.child.kill().unwrap();
    h.child.wait().unwrap();
    h.child = fixture_command(&h.root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if h.call(Request::Workspace).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut stream = UnixStream::connect(h.socket()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    receive::<Reply>(&mut stream).unwrap();
    send(&mut stream, CONTROL, &Request::WatchRecognition).unwrap();
    assert!(matches!(
        receive::<Reply>(&mut stream).unwrap(),
        Reply::Recognition(_)
    ));
    fs::rename(&root, h.root.join("moved-codex")).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("after.jsonl"),
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"after\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    let mut recovering = false;
    let mut interrupted = false;
    loop {
        let Reply::Recognition(status) = receive::<Reply>(&mut stream).unwrap() else {
            panic!("status frame required")
        };
        interrupted |= status.phase == RecognitionPhase::Interrupted;
        recovering |= status.phase == RecognitionPhase::Recovering;
        if interrupted && recovering && status.phase == RecognitionPhase::Healthy {
            break;
        }
    }
    // A fresh append after recovery is consumed without restarting the host.
    use std::io::Write;
    let path = root.join("after.jsonl");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{}\n")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
    loop {
        let offset: Option<i64> = db
            .query_row(
                "SELECT offset FROM log_files WHERE cli_id='after'",
                [],
                |r| r.get(0),
            )
            .ok();
        if offset == Some(fs::metadata(&path).unwrap().len() as i64) {
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM log_files WHERE cli_id='before'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0,
                "missing old paths must be removed from the repaired subtree index"
            );
            break;
        }
        assert!(Instant::now() < deadline, "listener did not resume");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn log_subscription_restarts_itself_after_an_index_write_fails() {
    let h = Host::start();
    let root = h.root.join("logs/codex");
    fs::create_dir_all(&root).unwrap();
    let db = rusqlite::Connection::open(h.root.join("workbench.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let wait = |what: &str, done: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    wait("subscription did not start", &|| {
        recognition(&h).phase == RecognitionPhase::Healthy
    });
    // Indexing fails, and so does recording that failure: the error leaves the subscription.
    db.execute_batch(
        "CREATE TRIGGER fail_insert BEFORE INSERT ON log_files BEGIN SELECT RAISE(ABORT,'injected index failure'); END;
         CREATE TRIGGER fail_update BEFORE UPDATE ON log_files BEGIN SELECT RAISE(ABORT,'injected index failure'); END;
         CREATE TRIGGER fail_state BEFORE UPDATE ON log_index BEGIN SELECT RAISE(ABORT,'injected index failure'); END;",
    )
    .unwrap();
    fs::write(
        root.join("broken.jsonl"),
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"broken\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    wait(
        "the injected failure never stopped the subscription",
        &|| recognition(&h).detail.starts_with("识别中断"),
    );
    db.execute_batch(
        "DROP TRIGGER fail_insert; DROP TRIGGER fail_update; DROP TRIGGER fail_state;",
    )
    .unwrap();
    // No host restart: the subscription must come back on its own and index new writes.
    let path = root.join("after.jsonl");
    fs::write(
        &path,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"after-failure\",\"cwd\":\"/tmp\"}}\n",
    )
    .unwrap();
    wait(
        "subscription stayed dead after the failure cleared",
        &|| {
            db.query_row(
                "SELECT offset FROM log_files WHERE cli_id='after-failure'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .is_ok_and(|offset| offset == fs::metadata(&path).unwrap().len() as i64)
        },
    );
    assert!(
        h.call(Request::RefreshAgents).is_ok(),
        "refresh must not keep reporting the old subscription failure"
    );
}

/// 在编辑器里看滚动内容 reads up to 10 000 rows under the terminal's lock; how long that
/// holds output back on a full, wide scrollback.
#[test]
#[ignore = "measurement"]
fn reading_ten_thousand_rows_of_scrollback_is_quick() {
    let h = Host::start();
    h.setup();
    h.call(h.launch(
        "long",
        "i=0; while [ $i -lt 12000 ]; do printf '%05d %s\\n' $i 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; i=$((i+1)); done; echo DONE; sleep 60",
    ))
    .unwrap();
    let read = |rows| match h
        .call(Request::ReadScreen {
            id: "long".into(),
            rows,
        })
        .unwrap()
    {
        Reply::Text(t) => t,
        x => panic!("{x:?}"),
    };
    let until = Instant::now() + Duration::from_secs(60);
    while !read(5).contains("DONE") {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut times = vec![];
    for _ in 0..5 {
        let t = Instant::now();
        let text = read(10_000);
        times.push(t.elapsed());
        assert!(
            text.contains("DONE") && text.lines().count() > 4_000,
            "{}",
            text.lines().count()
        );
    }
    eprintln!("read 10000 rows: {times:?}");
}
