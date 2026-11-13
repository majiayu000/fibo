use super::*;

/// The real CLIs, installed, run with a home of their own and no credentials: what the host
/// can tell of them before anyone signs in. By hand, `cargo test -p fibo-host --test
/// managed_session real_cli -- --ignored --nocapture`, with `claude` and `codex` on PATH.
#[test]
#[ignore = "needs the real CLIs installed"]
fn real_clis_are_recognized_from_their_processes() {
    let h = Host::start();
    h.setup();
    let home = h.root.join("real-home");
    fs::create_dir_all(&home).unwrap();
    for (pane, cli, agent) in [("c", "claude", Agent::Claude), ("x", "codex", Agent::Codex)] {
        let path = which(cli);
        h.call(h.launch(
            pane,
            // A login shell's PATH may not hold node, which npm's `codex` starts with.
            &format!(
                "HOME={} PATH={}:$PATH exec {path}",
                shell_quote(&home.to_string_lossy()),
                shell_quote(&std::env::var("PATH").unwrap()),
            ),
        ))
        .unwrap();
        let until = Instant::now() + Duration::from_secs(40);
        loop {
            let w = h.workspace();
            let found = fibo_host::recognition::processes(&w).unwrap();
            for p in &found {
                eprintln!(
                    "{cli}: pid {} parent {} name {:?} argv {:?} runtime {:?}",
                    p.pid,
                    p.parent,
                    fibo_platform::inspect::name(p.pid),
                    fibo_platform::inspect::argv(p.pid).unwrap_or_default(),
                    p.runtime
                );
            }
            if found.iter().any(|p| p.runtime == Some(agent)) {
                break;
            }
            assert!(Instant::now() < until, "{cli} was never recognized");
            std::thread::sleep(Duration::from_secs(2));
        }
        // What `fibo list` would show, and what `commands_of` looks for.
        let named = fibo_platform::inspect::all_pids()
            .into_iter()
            .filter(|p| fibo_platform::inspect::name(*p).as_deref() == Some(cli))
            .count();
        eprintln!("{cli}: {named} processes named exactly {cli}");
        h.call(Request::Stop { id: pane.into() }).unwrap();
    }
}

fn which(cli: &str) -> String {
    let out = Command::new("/bin/sh")
        .args(["-c", &format!("command -v {cli}")])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
