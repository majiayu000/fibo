//! A window's process ignores TERM (AppKit quits the app its own way), and a child keeps that
//! across exec: the ssh forwards a window left after crashing, and its host, ignored TERM.
//! Its worker threads also block every signal, and the mask goes across exec too.
//! Started through `takes_term` or `detach`, TERM stops them again. One test in its own
//! program, since it changes this process's TERM.
#![cfg(unix)]
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn a_child_of_a_process_that_ignores_term_still_stops_on_term() {
    // SAFETY: this test program holds this one test; nothing else here relies on TERM.
    unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
    // As a GCD worker thread, where the window starts its forwards from, has it.
    // SAFETY: changes this thread's mask only.
    unsafe {
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        libc::sigaddset(&mut mask, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
    }
    let start = |prepare: fn(&mut Command)| {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        prepare(&mut command);
        command.spawn().unwrap()
    };
    let term = |child: &std::process::Child| {
        // SAFETY: a signal to a child of this test.
        unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    };
    // As it was: the child ignores TERM as this process does.
    let mut plain = start(|_| {});
    term(&plain);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        plain.try_wait().unwrap().is_none(),
        "the control did not inherit the ignore"
    );
    plain.kill().unwrap();
    for (how, prepare) in [
        (
            "takes_term",
            fibo_platform::process::takes_term as fn(&mut Command),
        ),
        ("detach", fibo_platform::process::detach),
    ] {
        let mut child = start(prepare);
        term(&child);
        let at = Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(
                at.elapsed() < Duration::from_secs(3),
                "started with {how}, it ignored TERM"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
