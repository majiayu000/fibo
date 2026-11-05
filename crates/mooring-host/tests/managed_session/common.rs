use super::*;

// Functional fixtures still run in parallel. Resource measurements exclusively
// own the fixture lane so 40-PTY setup, SQLite fsync and FSEvents recovery from
// sibling tests cannot contaminate their CPU/I/O window. This is test-only:
// the measured PID remains the isolated daemon, never the harness or host CPU.
pub(crate) static FIXTURES: RwLock<()> = RwLock::new(());

pub(crate) enum FixtureLease {
    // Created only while a measured Host already holds the exclusive fixture lane.
    Companion,
    Shared {
        _guard: RwLockReadGuard<'static, ()>,
    },
    Measured {
        _guard: RwLockWriteGuard<'static, ()>,
    },
}
pub(crate) struct Host {
    pub(crate) child: tied::Tied,
    pub(crate) root: PathBuf,
    pub(crate) _lease: FixtureLease,
}
impl Host {
    pub(crate) fn start() -> Self {
        Self::with_lease(FixtureLease::Shared {
            _guard: FIXTURES.read().unwrap_or_else(|e| e.into_inner()),
        })
    }
    pub(crate) fn measured() -> Self {
        Self::with_lease(FixtureLease::Measured {
            _guard: FIXTURES.write().unwrap_or_else(|e| e.into_inner()),
        })
    }
    pub(crate) fn with_lease(lease: FixtureLease) -> Self {
        let root = std::env::temp_dir().join(format!("kl-{}", uuid::Uuid::new_v4().simple()));
        let child = fixture_command(&root)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn_tied()
            .unwrap();
        let h = Self {
            child,
            root,
            _lease: lease,
        };
        let until = Instant::now() + Duration::from_secs(30);
        while !h.socket().exists() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
        h
    }
    pub(crate) fn socket(&self) -> PathBuf {
        self.root.join("workbench.sock")
    }
    pub(crate) fn call(&self, request: Request) -> io::Result<Reply> {
        rpc(&self.socket(), &request)
    }
    pub(crate) fn workspace(&self) -> Workspace {
        match self.call(Request::Workspace).unwrap() {
            Reply::Workspace(w) => w,
            x => panic!("{x:?}"),
        }
    }
    pub(crate) fn setup(&self) {
        self.call(Request::Edit {
            request: None,
            edit: Edit::AddSpace(Space {
                id: "s".into(),
                name: "Tests".into(),
                cwd: "/tmp".into(),
            }),
        })
        .unwrap();
    }
    pub(crate) fn launch(&self, id: &str, cmd: &str) -> Request {
        Request::Launch {
            id: id.into(),
            space: "s".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), cmd.into()],
            cwd: "/tmp".into(),
            cols: 80,
            rows: 24,
            scheme: Default::default(),
            colors: None,
        }
    }
    pub(crate) fn attach(&self, id: &str, takeover: bool) -> (UnixStream, Vt, bool) {
        let mut stream = UnixStream::connect(self.socket()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
        assert!(matches!(
            receive::<Reply>(&mut stream).unwrap(),
            Reply::Hello { major: MAJOR, .. }
        ));
        send(
            &mut stream,
            CONTROL,
            &Request::Attach {
                id: id.into(),
                takeover,
                watch: false,
            },
        )
        .unwrap();
        let writable = match receive::<Reply>(&mut stream).unwrap() {
            Reply::Attached { writable, .. } => writable,
            other => panic!("{other:?}"),
        };
        let mut bytes = Vec::new();
        loop {
            let (kind, chunk) = read_frame(&mut stream).unwrap();
            if kind == READY {
                break;
            }
            assert_eq!(kind, SNAPSHOT);
            bytes.extend(chunk);
        }
        (stream, Vt::restore(&bytes).unwrap(), writable)
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            if let Ok(Reply::Workspace(w)) = self.call(Request::Workspace) {
                for s in w.terminals {
                    let _ = self.call(Request::Stop { id: s.id });
                }
            }
            self.child.kill_tree();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub(crate) fn ack(stream: &mut UnixStream) -> Reply {
    loop {
        let (kind, bytes) = read_frame(stream).unwrap();
        if kind == ACK {
            return serde_json::from_slice(&bytes).unwrap();
        }
    }
}
// proc_pid_rusage uses Mach absolute time on Apple Silicon. Dividing raw ticks
// by 1e9 undercounts CPU by 125/3 on this host; convert using the OS timebase.
#[allow(deprecated)] // The existing libc binding is sufficient; no extra dependency for this test.
pub(crate) fn cpu_seconds(ticks: u64) -> f64 {
    let mut base = libc::mach_timebase_info { numer: 0, denom: 0 };
    assert_eq!(unsafe { libc::mach_timebase_info(&mut base) }, 0);
    ticks as f64 * base.numer as f64 / base.denom as f64 / 1e9
}

/// Reads a terminal stream until its screen shows `marker`.
pub(crate) fn read_until(stream: &mut UnixStream, vt: &mut Vt, marker: &str) {
    let until = Instant::now() + Duration::from_secs(10);
    while !vt.tail_text(24).unwrap().contains(marker) {
        assert!(Instant::now() < until, "{marker} never appeared");
        let (kind, bytes) = read_frame(stream).unwrap();
        if kind == OUTPUT {
            vt.feed(&bytes);
        }
    }
}
pub(crate) fn wait_file(path: &std::path::Path) -> String {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(body) = fs::read_to_string(path) {
            if !body.trim().is_empty() {
                return body;
            }
        }
        assert!(Instant::now() < until, "missing {}", path.display());
        std::thread::sleep(Duration::from_millis(25));
    }
}
pub(crate) fn fixture_command(root: &std::path::Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "daemon_fixture", "--nocapture"])
        .env("MOORING_FIXTURE_RUNTIME", root)
        // Tests that need Claude Code's official status write this script; others have none.
        .env("MOORING_CLAUDE_AGENTS", root.join("claude-agents"))
        .env("MOORING_CODEX_CONTROL", root.join("codex-control.sock"))
        // Grok's registry of live sessions is read from its home, not the fixture roots.
        .env("GROK_HOME", root.join("grok-home"))
        // The host starts `mooring remote --relay`; here it runs inside the test binary.
        .env("MOORING_REMOTE_EXE", env!("CARGO_BIN_EXE_mooring"));
    command
}
pub(crate) fn recognition(h: &Host) -> RecognitionStatus {
    match h.call(Request::RecognitionStatus).unwrap() {
        Reply::Recognition(s) => s,
        other => panic!("{other:?}"),
    }
}
pub(crate) fn usage(pid: u32) -> libc::rusage_info_v4 {
    unsafe {
        let mut r: libc::rusage_info_v4 = std::mem::zeroed();
        assert_eq!(
            libc::proc_pid_rusage(
                pid as i32,
                libc::RUSAGE_INFO_V4,
                (&mut r as *mut libc::rusage_info_v4).cast()
            ),
            0
        );
        r
    }
}
pub(crate) fn wait_for(h: &Host, what: &str, ok: impl Fn(&Workspace) -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !ok(&h.workspace()) {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}
pub(crate) fn current_is(w: &Workspace, pane: &str, thread: &str) -> bool {
    w.current_agent(pane)
        .is_some_and(|s| s.cli_session_id == thread)
}
pub(crate) fn ids<const N: usize>() -> [String; N] {
    std::array::from_fn(|_| uuid::Uuid::new_v4().to_string())
}
