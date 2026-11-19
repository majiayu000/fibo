//! One window refresh, as host and window do it: the host diffs its snapshot against what
//! it sent, the window applies that to what it holds and hands the UI its own copy, and
//! the oldest kept workspace is dropped. Counts this process's retired instructions.
//! usage: cargo run --release -p fibo-core --example refresh_bench -- WORKSPACE.json [N]
#![allow(clippy::unwrap_used)]
use fibo_core::model::Workspace;
use fibo_core::workspace_delta::{Hashes, apply, diff_with};
use std::collections::VecDeque;

unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
}
/// `ri_instructions` of `rusage_info_v4`: after the 16-byte uuid, the 30th u64.
fn instructions() -> u64 {
    let mut info = [0u64; 48];
    // SAFETY: the kernel writes one rusage_info_v4 (under 48 u64s) into the buffer.
    unsafe { proc_pid_rusage(std::process::id() as i32, 4, info.as_mut_ptr()) };
    info[2 + 29]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("workspace json");
    let n: usize = args.next().map_or(200, |s| s.parse().unwrap());
    let mut w: Workspace = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let ids: Vec<String> = w.content.keys().cloned().collect();
    let mut hashes = Hashes::default();
    let (delta, mut index) = diff_with(w.clone(), None, true, &mut hashes).unwrap();
    let mut held = VecDeque::from([apply(delta, None).unwrap()]);
    let (mut host, mut window) = (0u64, 0u64);
    for i in 0..n {
        // A log line read: one session's content changes in the host.
        let id = &ids[i % ids.len()];
        std::sync::Arc::make_mut(w.content.get_mut(id).unwrap()).title = format!("第 {i} 步");
        let a = instructions();
        let (delta, next) = diff_with(w.clone(), Some(&index), true, &mut hashes).unwrap();
        let b = instructions();
        let kept = apply(delta, held.back()).unwrap();
        let ui = kept.workspace.clone();
        held.push_back(kept);
        if held.len() > 3 {
            held.pop_front();
        }
        drop(ui);
        let c = instructions();
        index = next;
        host += b - a;
        window += c - b;
    }
    println!(
        "{} sessions: host diff {:.1} M, window apply+copy {:.1} M per refresh",
        w.agent_sessions.len(),
        host as f64 / n as f64 / 1e6,
        window as f64 / n as f64 / 1e6
    );
}
