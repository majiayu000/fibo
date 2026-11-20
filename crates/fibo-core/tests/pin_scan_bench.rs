//! What locating the pinned asks and reading the tail cost, in retired instructions:
//! `cargo test --release -p fibo-core --test pin_scan_bench -- --ignored --nocapture`.
//! A full scrollback of indented answer rows under eight asks, the first of which has
//! scrolled out of history, so every one of the `MAX_ROWS` rows is read (the usual case
//! in a long or resumed conversation). macOS only: instruction counts come from `proc_pid_rusage`.
#![cfg(target_os = "macos")]
use fibo_core::agent::PromptLine;
use fibo_core::prompt_pin::key;
use fibo_core::terminal::Vt;

fn instructions() -> u64 {
    fibo_platform::inspect::usage(std::process::id()).map_or(0, |u| u.instructions)
}

fn per_call(n: u64, mut f: impl FnMut()) -> f64 {
    f();
    let start = instructions();
    for _ in 0..n {
        f();
    }
    (instructions() - start) as f64 / n as f64
}

#[test]
#[ignore = "measurement, run by hand"]
fn pinned_ask_scan_and_tail_text_instructions() {
    let mut vt = Vt::new(120, 50).unwrap();
    let asks: Vec<String> = (0..8)
        .map(|i| format!("第{i}个问题 please look at item {i}"))
        .collect();
    let mut bytes = Vec::new();
    for i in 0..6000 {
        // asks[0] is pushed out of the 5000-row history; the others sit near the bottom.
        let ask = match i {
            0 => Some(0),
            _ if i >= 5000 && i % 120 == 0 => Some((i - 4880) / 120),
            _ => None,
        };
        match ask.filter(|&a| a < asks.len()) {
            Some(a) => bytes.extend(format!("\u{276f} {}\r\n", asks[a]).bytes()),
            None => bytes.extend(
                format!("  第 {i} 行的回答 the answer runs on, src/main.rs:{i} 和 更多 中文 文字 here\r\n")
                    .bytes(),
            ),
        }
    }
    vt.feed(&bytes);
    let keys: Vec<_> = asks.iter().map(|a| key(a)).collect();
    let prompt = PromptLine {
        markers: vec!["\u{276f}".into(), ">".into()],
        pins_own: false,
    };
    let found = vt.find_prompts(&keys, &prompt).unwrap();
    assert_eq!(found[0], None, "{found:?}");
    assert!(found[1..].iter().all(Option::is_some), "{found:?}");
    if instructions() == 0 {
        eprintln!("no instruction counter here");
        return;
    }
    let scan = per_call(10, || {
        std::hint::black_box(vt.find_prompts(&keys, &prompt).unwrap());
    });
    let tail = per_call(500, || {
        std::hint::black_box(vt.tail_text(45).unwrap());
    });
    eprintln!(
        "find_prompts {:.2} M per scan, tail_text(45) {:.3} M per call",
        scan / 1e6,
        tail / 1e6
    );
}
