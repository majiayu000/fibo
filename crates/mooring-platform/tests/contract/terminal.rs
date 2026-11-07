use mooring_platform::terminal::{Master, Read, Reader, Spec, own_size, spawn};
use std::io::BufRead;
use std::path::Path;
use std::time::{Duration, Instant};

const ACTIVE: &str = "MOORING_CONTRACT_TERMINAL";

/// A program for the tests below: says what it was given, then does what it is told.
#[test]
#[ignore = "started by the tests in this file"]
fn helper_in_a_terminal() {
    if std::env::var_os(ACTIVE).is_none() {
        return;
    }
    let cwd = std::env::current_dir().unwrap();
    let name = cwd.file_name().unwrap().to_string_lossy().into_owned();
    println!("up here={name} var={}", std::env::var(ACTIVE).unwrap());
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        match line.trim() {
            "size" => {
                let (cols, rows) = own_size().unwrap();
                println!("is {cols}x{rows}.");
            }
            "end" => std::process::exit(7),
            other => println!("got:{other}."),
        }
    }
}

fn start(name: &str) -> (mooring_platform::terminal::Child, Master, Reader) {
    let dir = crate::scratch(name);
    let argv = vec![
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        "terminal::helper_in_a_terminal".to_owned(),
        "--exact".to_owned(),
        "--ignored".to_owned(),
        "--nocapture".to_owned(),
    ];
    let mut env: Vec<_> = std::env::vars_os().collect();
    env.push((ACTIVE.into(), "from-spec".into()));
    let (child, master) = spawn(&Spec {
        argv: &argv,
        cwd: Path::new(&dir),
        env,
        cols: 90,
        rows: 30,
    })
    .unwrap();
    let reader = master.reader().unwrap();
    (child, master, reader)
}

/// Reads until the output so far holds `marker`. Windows may put cursor moves anywhere, so
/// markers stay short and are looked for in order of arrival only.
fn wait_for(reader: &mut Reader, seen: &mut String, marker: &str) {
    let until = Instant::now() + Duration::from_secs(20);
    let mut buffer = [0; 4096];
    while !seen.contains(marker) {
        assert!(Instant::now() < until, "{marker} never came; saw {seen:?}");
        match reader
            .next(&mut buffer, Some(Duration::from_millis(200)))
            .unwrap()
        {
            Read::Data(n) => seen.push_str(&String::from_utf8_lossy(&buffer[..n])),
            Read::Quiet => (),
            Read::Closed => panic!("closed before {marker}; saw {seen:?}"),
        }
    }
}

#[test]
fn a_program_runs_where_and_how_it_was_told() {
    let name = "terminal-where";
    let (mut child, mut master, mut reader) = start(name);
    let mut seen = String::new();
    wait_for(&mut reader, &mut seen, "var=from-spec");
    assert!(
        seen.contains(&format!("here=kl-contract-{name}")),
        "{seen:?}"
    );
    master.write(b"end\r").unwrap();
    assert_eq!(child.wait().unwrap(), Some(7));
}

#[test]
fn what_is_typed_reaches_the_program_and_its_answer_comes_back() {
    let (mut child, mut master, mut reader) = start("terminal-typed");
    let mut seen = String::new();
    wait_for(&mut reader, &mut seen, "var=from-spec");
    master.write(b"hello\r").unwrap();
    wait_for(&mut reader, &mut seen, "got:hello.");
    // Wider than the piece a write sends at once.
    let long = "x".repeat(70);
    for _ in 0..8 {
        seen.clear();
        master.write(format!("{long}\r").as_bytes()).unwrap();
        wait_for(&mut reader, &mut seen, "x.");
    }
    master.write(b"end\r").unwrap();
    assert_eq!(child.wait().unwrap(), Some(7));
}

#[test]
fn a_program_sees_the_size_it_was_given_and_then_the_new_one() {
    let (mut child, mut master, mut reader) = start("terminal-size");
    let mut seen = String::new();
    wait_for(&mut reader, &mut seen, "var=from-spec");
    master.write(b"size\r").unwrap();
    wait_for(&mut reader, &mut seen, "is 90x30.");
    master.resize(100, 40, (8, 16)).unwrap();
    // The new size takes a moment to arrive on Windows.
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        seen.clear();
        master.write(b"size\r").unwrap();
        let mut buffer = [0; 4096];
        let answered = Instant::now() + Duration::from_secs(1);
        while Instant::now() < answered && !seen.contains("is 100x40.") {
            if let Read::Data(n) = reader
                .next(&mut buffer, Some(Duration::from_millis(100)))
                .unwrap()
            {
                seen.push_str(&String::from_utf8_lossy(&buffer[..n]));
            }
        }
        if seen.contains("is 100x40.") {
            break;
        }
        assert!(Instant::now() < until, "never resized; saw {seen:?}");
    }
    master.write(b"end\r").unwrap();
    assert_eq!(child.wait().unwrap(), Some(7));
}

#[test]
fn output_says_quiet_while_there_is_none_and_closed_at_the_end() {
    let (mut child, mut master, mut reader) = start("terminal-closed");
    let mut seen = String::new();
    wait_for(&mut reader, &mut seen, "var=from-spec");
    let mut buffer = [0; 4096];
    // Whatever is still on its way, then silence.
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let at = Instant::now();
        let got = reader
            .next(&mut buffer, Some(Duration::from_millis(300)))
            .unwrap();
        if got == Read::Quiet {
            assert!(at.elapsed() >= Duration::from_millis(250), "woke early");
            break;
        }
        assert!(Instant::now() < until, "never quiet");
    }
    master.write(b"end\r").unwrap();
    assert_eq!(child.wait().unwrap(), Some(7));
    // Every copy of the master has to go on Unix, where it is what holds the terminal open.
    drop(master);
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if reader
            .next(&mut buffer, Some(Duration::from_millis(200)))
            .unwrap()
            == Read::Closed
        {
            break;
        }
        assert!(Instant::now() < until, "output never ended");
    }
}

#[test]
fn a_program_that_cannot_start_says_so() {
    let dir = crate::scratch("terminal-missing");
    let argv = vec![dir.join("no-such-program").to_string_lossy().into_owned()];
    let started = spawn(&Spec {
        argv: &argv,
        cwd: &dir,
        env: std::env::vars_os().collect(),
        cols: 80,
        rows: 24,
    });
    assert!(started.is_err());
}

#[test]
fn killing_a_program_ends_it() {
    let (mut child, _master, mut reader) = start("terminal-kill");
    let mut seen = String::new();
    wait_for(&mut reader, &mut seen, "var=from-spec");
    let pid = child.id() as i32;
    child.kill().unwrap();
    child.wait().unwrap();
    // A process lasts on Windows while a handle to it does.
    drop(child);
    assert!(!mooring_platform::process::exists(pid));
}
