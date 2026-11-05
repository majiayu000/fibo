use super::*;

#[test]
fn diagnostic_stream_measures_a_busy_managed_terminal_without_holding_workspace_lock() {
    let host = Host::measured();
    host.setup();
    host.call(host.launch("diag-cpu", "while kill -0 $PPID 2>/dev/null; do :; done"))
        .unwrap();
    let before = host.workspace().revision;
    let mut stream = UnixStream::connect(host.socket()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR }).unwrap();
    assert!(matches!(
        receive::<Reply>(&mut stream).unwrap(),
        Reply::Hello { .. }
    ));
    send(&mut stream, CONTROL, &Request::Diagnose).unwrap();
    let start = Instant::now();
    let workspace = host.workspace();
    assert_eq!(workspace.revision, before);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "diagnosis blocked the workspace"
    );
    let Reply::Diagnostics(partial) = receive::<Reply>(&mut stream).unwrap() else {
        panic!("missing first sample");
    };
    assert!(!partial.complete);
    assert!(partial.elapsed_secs >= 7.);
    let Reply::Diagnostics(done) = receive::<Reply>(&mut stream).unwrap() else {
        panic!("missing final sample");
    };
    assert!(done.complete);
    assert!(done.elapsed_secs >= 14.5 && done.elapsed_secs < 25.);
    assert!(done.cpu_percent.is_some_and(|p| (0. ..=100.).contains(&p)));
    let shell = done
        .apps
        .iter()
        .find(|app| app.terminals.iter().any(|id| id == "diag-cpu"))
        .expect("managed shell not attributed");
    assert!(
        shell.cpu_percent > 5.,
        "busy shell was not measured: {shell:?}"
    );
    let shares: f64 = done.apps.iter().filter_map(|a| a.gpu_share_percent).sum();
    assert!(shares == 0. || (shares - 100.).abs() < 0.001);
    assert!(done
        .summary("", &Default::default())
        .contains("不是整颗 GPU 利用率"));
    assert_eq!(
        host.workspace().revision,
        before,
        "diagnosis mutated workspace"
    );
    host.call(Request::Stop {
        id: "diag-cpu".into(),
    })
    .unwrap();
}
#[test]
fn a_newer_host_takes_running_terminals_over_without_ending_them() {
    let mut h = Host::start();
    h.setup();
    h.call(h.launch("t", "stty -echo; cat")).unwrap();
    let (mut first, mut vt, writable) = h.attach("t", false);
    assert!(writable);
    let input = |stream: &mut UnixStream, text: &str| {
        send(
            stream,
            CONTROL,
            &Request::Input {
                bytes: text.as_bytes().to_vec(),
                lossy: false,
            },
        )
        .unwrap();
    };
    input(&mut first, "before-handoff\n");
    read_until(&mut first, &mut vt, "before-handoff");
    let pid = h
        .workspace()
        .terminals
        .iter()
        .find(|t| t.id == "t")
        .and_then(|t| t.pid)
        .unwrap();
    let mut newer = fixture_command(&h.root)
        .env("MOORING_FIXTURE_TAKEOVER", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn_tied()
        .unwrap();
    // The old host hands over and exits on its own.
    let until = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = h.child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < until, "the old host never exited");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success());
    std::mem::swap(&mut h.child, &mut newer);
    let until = Instant::now() + Duration::from_secs(15);
    while h.call(Request::Workspace).is_err() {
        assert!(Instant::now() < until, "the new host never answered");
        std::thread::sleep(Duration::from_millis(20));
    }
    let w = h.workspace();
    let t = w.terminals.iter().find(|t| t.id == "t").unwrap();
    assert_eq!(t.state, ProcessState::Running);
    assert_eq!(t.pid, Some(pid));
    assert_eq!(
        unsafe { libc::kill(pid as i32, 0) },
        0,
        "the shell survived"
    );
    // The screen came along, and the same program still answers.
    let (mut second, mut vt, writable) = h.attach("t", false);
    assert!(writable);
    assert!(vt.tail_text(24).unwrap().contains("before-handoff"));
    input(&mut second, "after-handoff\n");
    read_until(&mut second, &mut vt, "after-handoff");
    // Not this host's child any more: its end is still seen, by pid.
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    wait_for(&h, "the handed-over terminal's exit is recorded", |w| {
        w.terminals
            .iter()
            .any(|t| t.id == "t" && t.state == ProcessState::Exited)
    });
    drop(newer);
}

#[test]
fn a_viewer_that_falls_behind_a_burst_is_resynced_not_cut() {
    let h = Host::start();
    h.setup();
    h.call(h.launch(
        "burst",
        "sleep 1; yes 0000000000000000000000000000000000000000000000000000000000000000000000000000 | head -n 200000; echo END-MARK; sleep 60",
    ))
    .unwrap();
    let (mut stream, mut vt, writable) = h.attach("burst", false);
    assert!(writable);
    // Not reading while the burst runs: the host's queue for this viewer fills up. Longer
    // than a connection's 5-second write timeout, which used to cut the viewer here.
    std::thread::sleep(Duration::from_secs(8));
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut snapshot = Vec::new();
    let mut resynced = false;
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(Instant::now() < until, "END-MARK never arrived");
        // A cut connection fails here, as it did before resync existed.
        let (kind, bytes) = read_frame(&mut stream).expect("the host kept the connection");
        match kind {
            SNAPSHOT => snapshot.extend(bytes),
            READY => {
                vt = Vt::restore(&snapshot).unwrap();
                snapshot.clear();
                resynced = true;
            }
            OUTPUT => vt.feed(&bytes),
            _ => {}
        }
        if vt.tail_text(30).unwrap().contains("END-MARK") {
            break;
        }
    }
    assert!(resynced, "a lagging viewer gets a fresh snapshot");
}

#[test]
fn mouse_reports_stop_before_a_stalled_program_fills_its_input() {
    let h = Host::start();
    h.setup();
    // Raw mode and nothing reading: input piles up as it would behind a busy Codex.
    h.call(h.launch("stalled", "stty raw -echo; sleep 60"))
        .unwrap();
    let (mut stream, _, writable) = h.attach("stalled", false);
    assert!(writable);
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    // 300 reports are 3.6 KiB, more than the ~1 KiB input queue: written in full they
    // would block, and the last one would be cut.
    for _ in 0..300 {
        send(
            &mut stream,
            CONTROL,
            &Request::Input {
                bytes: b"\x1b[<65;20;10M".to_vec(),
                lossy: true,
            },
        )
        .unwrap();
        assert!(matches!(ack(&mut stream), Reply::Ok));
    }
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // Keystrokes are never dropped.
    send(
        &mut stream,
        CONTROL,
        &Request::Input {
            bytes: b"x".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut stream), Reply::Ok));
}

#[test]
fn reconnect_input_resize_and_takeover_are_real_pty_operations() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("one", "stty -echo; cat")).unwrap();
    let (mut first, _, writable) = h.attach("one", false);
    assert!(writable);
    send(
        &mut first,
        CONTROL,
        &Request::Input {
            bytes: b"before-detach\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut first), Reply::Ok));
    let (mut second, _, writable) = h.attach("one", false);
    assert!(!writable);
    send(
        &mut second,
        CONTROL,
        &Request::Input {
            bytes: b"must-not-write\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut second), Reply::Error(_)));
    let (mut third, _, writable) = h.attach("one", true);
    assert!(writable);
    send(
        &mut first,
        CONTROL,
        &Request::Resize {
            cols: 90,
            rows: 30,
            cell_width: 9,
            cell_height: 18,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut first), Reply::Error(_)));
    send(
        &mut third,
        CONTROL,
        &Request::Resize {
            cols: 90,
            rows: 30,
            cell_width: 9,
            cell_height: 18,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut third), Reply::Ok));
    drop(first);
    drop(second);
    drop(third);
    let (mut again, vt, _) = h.attach("one", true);
    assert_eq!(vt.terminal().cols().unwrap(), 90);
    assert_eq!(vt.terminal().rows().unwrap(), 30);
    send(
        &mut again,
        CONTROL,
        &Request::Input {
            bytes: b"after-detach\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut again), Reply::Ok));
    assert_eq!(h.workspace().terminals[0].state, ProcessState::Running);
}
/// A user's theme reaches the host: at launch, and again when it changes while running.
#[test]
fn a_programs_queries_answer_with_the_theme_it_runs_under() {
    const PROBE: &str = r#"exec python3 -c '
import os, select, tty
tty.setraw(0)
os.write(1, b"FGBG " + os.environ["COLORFGBG"].encode() + b" END\r\n")
for _ in range(2):
    while os.read(0, 1) != b"?":
        pass
    os.write(1, b"\x1b]11;?\x07\x1b]4;1;?\x07")
    got = b""
    while select.select([0], [], [], 0.5)[0]:
        got += os.read(0, 4096)
    os.write(1, b"GOT " + got.hex().encode() + b" END\r\n")
'"#;
    let mut ansi = [None; 16];
    ansi[1] = Some(0xff4040);
    let first = mooring_core::terminal::TermColors::new(0x202020, 0xf0f0e0, None, ansi);
    let second = mooring_core::terminal::TermColors::new(0xd5dfef, 0x0f1c2e, None, [None; 16]);
    let h = Host::start();
    h.setup();
    let mut launch = h.launch("probe", PROBE);
    if let Request::Launch { colors, .. } = &mut launch {
        *colors = Some(first);
    }
    h.call(launch).unwrap();
    let (mut owner, _, writable) = h.attach("probe", false);
    assert!(writable);
    let printed = |marks: usize| {
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            let (_, vt, _) = h.attach("probe", false);
            // The hex wraps across full rows; joined rows give the lines back.
            let text = vt.tail_text(30).unwrap().replace('\n', "");
            if text.matches(" END").count() >= marks {
                return text;
            }
            assert!(Instant::now() < until, "probe printed: {text}");
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let ask = |owner: &mut UnixStream| {
        let bytes = b"?".to_vec();
        send(
            owner,
            CONTROL,
            &Request::Input {
                bytes,
                lossy: false,
            },
        )
        .unwrap();
        assert!(matches!(ack(owner), Reply::Ok));
    };
    let hex = |s: &str| s.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    printed(1);
    ask(&mut owner);
    let got = printed(2);
    assert!(got.contains("FGBG 0;15"), "a light theme says so: {got}");
    assert!(got.contains(&hex("11;rgb:f0f0/f0f0/e0e0")), "{got}");
    assert!(got.contains(&hex("4;1;rgb:ffff/4040/4040")), "{got}");
    send(&mut owner, CONTROL, &Request::Colors(second)).unwrap();
    assert!(matches!(ack(&mut owner), Reply::Ok));
    ask(&mut owner);
    let got = printed(3);
    let last = got.rsplit("GOT ").next().unwrap();
    assert!(last.contains(&hex("11;rgb:0f0f/1c1c/2e2e")), "{got}");
    assert!(!last.contains(&hex("4;1;rgb:ffff/4040/4040")), "{got}");
}
#[test]
fn programs_get_answers_to_terminal_queries_and_pixel_size() {
    // A real program in the PTY: waits for "go", asks like Codex/TUIs do, prints what came back.
    const PROBE: &str = r#"exec python3 -c '
import os, select, tty, fcntl, struct, termios, time
input()
tty.setraw(0)
os.write(1, b"\x1b]11;?\x07\x1b]10;?\x07\x1b[c\x1b[>q\x1b[18t\x1b[14t\x1b[?996n")
got = b""
while select.select([0], [], [], 1.0)[0]:
    got += os.read(0, 4096)
win = struct.unpack("HHHH", fcntl.ioctl(1, termios.TIOCGWINSZ, b"\0" * 8))
os.write(1, b"\r\nGOT " + got.hex().encode() + b" WIN %d %d %d %d END\r\n" % win)
time.sleep(60)
'"#;
    let h = Host::start();
    h.setup();
    h.call(h.launch("probe", PROBE)).unwrap();
    let (mut owner, _, writable) = h.attach("probe", false);
    assert!(writable);
    send(
        &mut owner,
        CONTROL,
        &Request::Resize {
            cols: 100,
            rows: 30,
            cell_width: 16,
            cell_height: 34,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut owner), Reply::Ok));
    send(
        &mut owner,
        CONTROL,
        &Request::Input {
            bytes: b"go\n".to_vec(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut owner), Reply::Ok));
    let until = Instant::now() + Duration::from_secs(15);
    let line = loop {
        let (_, vt, _) = h.attach("probe", false);
        // The hex wraps across full rows; joined rows give the line back.
        let text = vt.tail_text(30).unwrap().replace('\n', "");
        if let Some(line) = text
            .split("GOT ")
            .nth(1)
            .and_then(|rest| rest.split(" END").next().filter(|_| rest.contains(" END")))
        {
            break format!("GOT {line}");
        }
        assert!(Instant::now() < until, "probe printed nothing");
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut parts = line.split_whitespace().skip(1);
    let hex = parts.next().unwrap();
    let got: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let got = String::from_utf8(got).unwrap();
    for reply in [
        "\x1b]11;rgb:0e0e/0e0e/1111\x07",
        "\x1b]10;rgb:d4d4/d4d4/d8d8\x07",
        "\x1b[?62;22c",
        "\x1bP>|mooring ",
        "\x1b[8;30;100t",
        "\x1b[4;1020;1600t",
        "\x1b[?997;1n",
    ] {
        assert!(got.contains(reply), "missing {reply:?} in {got:?}");
    }
    // The pty's own pixel size follows the cell size too (rows cols xpixel ypixel).
    assert_eq!(
        parts.collect::<Vec<_>>(),
        ["WIN", "30", "100", "1600", "1020"]
    );
}
/// Real Codex: it reads the terminal background (OSC 11) and paints its input box a
/// shade off it. Needs the `codex` CLI; start-up only, no prompt is sent.
/// `KL_CODEX=/path/to/codex KL_CODEX_CWD=<trusted dir> cargo test -- --ignored real_codex`
#[test]
#[ignore = "needs a local Codex CLI"]
fn real_codex_tints_its_input_box_from_the_terminal_background() {
    use mooring_core::terminal::libghostty_vt::{
        screen::CellContentTag,
        style::StyleColor,
        terminal::{Point, PointCoordinate},
    };
    let codex = std::env::var("KL_CODEX").unwrap_or_else(|_| "codex".into());
    let cwd = std::env::var("KL_CODEX_CWD").unwrap_or_else(|_| "/tmp".into());
    let h = Host::start();
    h.setup();
    let mut launch = h.launch("codex", &format!("cd '{cwd}' && exec '{codex}'"));
    if let Request::Launch { cols, rows, .. } = &mut launch {
        (*cols, *rows) = (100, 30);
    }
    h.call(launch).unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    let (vt, row) = loop {
        let (_, vt, _) = h.attach("codex", false);
        if let Some(m) = vt.search("Ask Codex").unwrap().into_iter().last() {
            break (vt, m.row);
        }
        assert!(Instant::now() < until, "{}", vt.tail_text(30).unwrap());
        std::thread::sleep(Duration::from_millis(200));
    };
    let t = vt.terminal();
    let mut backgrounds = std::collections::BTreeSet::new();
    for x in 0..t.cols().unwrap() {
        let cell = t
            .grid_ref(Point::Screen(PointCoordinate { x, y: row as u32 }))
            .unwrap();
        let bg = match cell.style().map(|s| s.bg_color) {
            Ok(StyleColor::Rgb(c)) => Some((c.r, c.g, c.b)),
            // A blank cell may carry only a background; text cells keep it in the style.
            _ => cell
                .cell()
                .ok()
                .filter(|c| c.content_tag().ok() == Some(CellContentTag::BgColorRgb))
                .and_then(|c| c.bg_color_rgb().ok())
                .map(|c| (c.r, c.g, c.b)),
        };
        backgrounds.insert(bg);
    }
    eprintln!("input row {row}: backgrounds {backgrounds:?}");
    // Codex lightens our dark default for its input box; unanswered, it paints none.
    let [_, base_r, base_g, base_b] = mooring_core::terminal::DEFAULT_BG.to_be_bytes();
    assert!(
        backgrounds
            .iter()
            .flatten()
            .any(|&(r, g, b)| (r, g, b) != (base_r, base_g, base_b) && r > base_r),
        "input box has no tint: {backgrounds:?}\n{}",
        vt.tail_text(30).unwrap()
    );
}
/// Real Codex: a file dropped on the pane arrives in Codex's input box as one bracketed
/// paste of its escaped path (`terminal_input::dropped_paths`), as Ghostty sends it.
/// (Codex 0.163 here kept image paths as text in every form tried: escaped, plain, quoted.)
#[test]
#[ignore = "needs a local Codex CLI"]
fn real_codex_receives_a_dropped_path_as_one_paste() {
    // 1x1 PNG; the name needs escaping.
    const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0d\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\xf0\x1f\x00\x05\x00\x01\xff\x89\x99\x3d\x1d\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
    let dir = std::env::temp_dir().join(format!("kl-drop-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&dir).unwrap();
    let image = dir.join("drop test (1).png");
    fs::write(&image, PNG).unwrap();
    let codex = std::env::var("KL_CODEX").unwrap_or_else(|_| "codex".into());
    let cwd = std::env::var("KL_CODEX_CWD").unwrap_or_else(|_| "/tmp".into());
    let h = Host::start();
    h.setup();
    h.call(h.launch("codex", &format!("cd '{cwd}' && exec '{codex}'")))
        .unwrap();
    let wait = |text: &str| {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let (_, vt, _) = h.attach("codex", false);
            if !vt.search(text).unwrap().is_empty() {
                return vt;
            }
            assert!(
                Instant::now() < until,
                "{text}: {}",
                vt.tail_text(30).unwrap()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    wait("Ask Codex");
    let (mut owner, _, writable) = h.attach("codex", true);
    assert!(writable);
    // What the pane sends for a drop: the escaped path inside a bracketed paste.
    let escaped = image
        .to_string_lossy()
        .replace(' ', "\\ ")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let pasted = format!("{escaped} ");
    send(
        &mut owner,
        CONTROL,
        &Request::Input {
            bytes: format!("\x1b[200~{pasted}\x1b[201~").into_bytes(),
            lossy: false,
        },
    )
    .unwrap();
    assert!(matches!(ack(&mut owner), Reply::Ok));
    // The composer wraps the long path; its file name lands whole on one row.
    let vt = wait("drop\\ test\\ \\(1\\).png");
    eprintln!("{}", vt.tail_text(8).unwrap());
    // Leave the prompt unsent and the session empty.
    send(
        &mut owner,
        CONTROL,
        &Request::Input {
            bytes: vec![3, 3],
            lossy: false,
        },
    )
    .unwrap();
    let _ = fs::remove_dir_all(&dir);
}
#[test]
fn clear_history_reaches_the_daemon_and_every_display() {
    let h = Host::start();
    h.setup();
    h.call(h.launch("c", "seq 1 100; printf 'PROMPT$ '; exec cat"))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while h.attach("c", false).1.search("PROMPT$").unwrap().is_empty() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(50));
    }
    let (mut owner, mut display, writable) = h.attach("c", true);
    assert!(writable);
    assert!(display.terminal().scrollback_rows().unwrap() > 0);
    send(&mut owner, CONTROL, &Request::ClearHistory).unwrap();
    // The owner's own stream carries the clear as output, before the ack.
    loop {
        let (kind, chunk) = read_frame(&mut owner).unwrap();
        if kind == OUTPUT {
            display.feed(&chunk);
        } else if kind == ACK {
            assert!(matches!(
                serde_json::from_slice::<Reply>(&chunk).unwrap(),
                Reply::Ok
            ));
            break;
        }
    }
    let (_, fresh, _) = h.attach("c", false);
    for vt in [&display, &fresh] {
        assert_eq!(vt.terminal().scrollback_rows().unwrap(), 0);
        assert!(vt.tail_text(24).unwrap().starts_with("PROMPT$"));
        assert!(vt.search("100").unwrap().is_empty());
    }
}
