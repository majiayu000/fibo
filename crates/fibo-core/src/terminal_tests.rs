#[test]
fn osc7_reports_decode_only_for_this_machine() {
    use super::local_path;
    let url = "file://Agent-OS.local/tmp/kl%20osc7/%E4%B8%AD%E6%96%87%20dir%23%25";
    assert_eq!(
        local_path(url, "Agent-OS.local").as_deref(),
        Some("/tmp/kl osc7/中文 dir#%")
    );
    assert_eq!(
        local_path(url, "agent-os").as_deref(),
        Some("/tmp/kl osc7/中文 dir#%")
    );
    assert_eq!(local_path("file:///a", "x").as_deref(), Some("/a"));
    assert_eq!(local_path("file://localhost/a", "x").as_deref(), Some("/a"));
    assert_eq!(local_path("file://devbox/home/u", "Agent-OS.local"), None);
    assert_eq!(local_path("file://x/a%2", "x"), None);
    assert_eq!(local_path("kitty-shell-cwd://x/a", "x"), None);
    assert_eq!(local_path("", "x"), None);
}
use super::*;
/// A fresh emulator fed `replay` shows what the original does: text, cursor, the
/// modes a client must honour for keys and paste, and the alternate screen.
#[test]
fn replay_rebuilds_the_screen_in_a_fresh_terminal() {
    use libghostty_vt::terminal::Mode;
    let mut vt = Vt::new(40, 6).unwrap();
    vt.feed(b"shell \x1b[1;31mred\x1b[0m line\r\n");
    vt.feed(b"\x1b[?2004h\x1b[?1h\x1b[?1049h\x1b[2J\x1b[H");
    vt.feed("❯ 1. Yes\r\n  2. 不\x1b[3;5H".as_bytes());
    let mut copy = Vt::new(40, 6).unwrap();
    copy.feed(&vt.replay().unwrap());
    assert_eq!(copy.tail_text(6).unwrap(), vt.tail_text(6).unwrap());
    assert!(copy.tail_text(6).unwrap().contains("2. 不"));
    let (a, b) = (vt.terminal(), copy.terminal());
    assert_eq!((b.cursor_x().unwrap(), b.cursor_y().unwrap()), (4, 2));
    for mode in [Mode::BRACKETED_PASTE, Mode::DECCKM] {
        assert_eq!(b.mode(mode).unwrap(), a.mode(mode).unwrap(), "{mode:?}");
        assert!(b.mode(mode).unwrap());
    }
}
#[test]
fn program_clipboard_writes_arrive_as_text() {
    use std::sync::{Arc, Mutex};
    let mut vt = Vt::new(80, 24).unwrap();
    let got = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&got);
    vt.on_clipboard_text(move |t| sink.lock().unwrap().push(t))
        .unwrap();
    vt.feed(b"\x1b]52;c;aGVsbG8g5LiW55WM\x07");
    vt.feed(b"\x1b]52;c;\x07");
    assert_eq!(*got.lock().unwrap(), ["hello 世界", ""]);
}
#[test]
fn selection_text_unwraps_soft_wraps_and_is_none_without_selection() {
    let mut vt = Vt::new(10, 4).unwrap();
    vt.feed(b"0123456789abcdef\r\nnext   ");
    assert_eq!(vt.selection_text().unwrap(), None);
    let t = vt.terminal();
    t.set_selection(t.select_all().unwrap().as_ref()).unwrap();
    assert_eq!(
        vt.selection_text().unwrap().as_deref(),
        Some("0123456789abcdef\nnext")
    );
}
#[test]
fn snapshot_preserves_mouse_reporting_modes() {
    use libghostty_vt::terminal::Mode;
    let modes = |vt: &Vt| {
        [Mode::NORMAL_MOUSE, Mode::BUTTON_MOUSE, Mode::SGR_MOUSE]
            .map(|m| vt.terminal().mode(m).unwrap())
    };
    let mut vt = Vt::new(80, 24).unwrap();
    vt.feed(b"\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1006h");
    assert_eq!(
        modes(&Vt::restore(&vt.snapshot().unwrap()).unwrap()),
        [true; 3]
    );
    vt.feed(b"\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?1049l");
    assert_eq!(
        modes(&Vt::restore(&vt.snapshot().unwrap()).unwrap()),
        [false; 3]
    );
}
#[test]
fn snapshot_preserves_shell_bracketed_paste_readiness() {
    let mut vt = Vt::new(80, 24).unwrap();
    vt.feed(b"\x1b[?2004hprompt> ");
    let restored = Vt::restore(&vt.snapshot().unwrap()).unwrap();
    assert!(
        restored
            .terminal()
            .mode(libghostty_vt::terminal::Mode::BRACKETED_PASTE)
            .unwrap()
    );
}
#[test]
fn emoji_clusters_use_two_cells_and_survive_snapshot_boundaries() {
    for text in ["👨‍👩‍👧‍👦", "🇨🇳", "👍🏽", "1️⃣", "🏳️‍🌈", "🧑‍💻"]
    {
        let mut vt = Vt::new(40, 3).unwrap();
        for ch in text.chars() {
            vt.feed(ch.to_string().as_bytes());
            vt = Vt::restore(&vt.snapshot().unwrap()).unwrap();
        }
        assert_eq!(vt.terminal().cursor_x().unwrap(), 2, "{text}");
        assert!(
            vt.terminal()
                .mode(libghostty_vt::terminal::Mode::GRAPHEME_CLUSTER)
                .unwrap()
        );
        vt.feed(b"X");
        assert!(vt.search(&format!("{text}X")).unwrap().len() == 1);
    }
}
#[test]
fn snapshot_preserves_incomplete_utf8_and_escape_sequence() {
    for (prefix, tail) in [
        (&b"before \xe4\xb8"[..], &b"\xad after"[..]),
        (&b"\x1b[?1049h\x1b[31"[..], &b"mRED\x1b[0m"[..]),
    ] {
        let mut source = Vt::new(80, 24).unwrap();
        source.feed(prefix);
        let mut restored = Vt::restore(&source.snapshot().unwrap()).unwrap();
        source.feed(tail);
        restored.feed(tail);
        assert_eq!(source.snapshot().unwrap(), restored.snapshot().unwrap());
    }
}
#[test]
fn search_finds_scrollback_and_wide_graphemes_without_spacer_gaps() {
    let mut vt = Vt::new(40, 3).unwrap();
    vt.feed("old 实现反馈 e\u{301}\r\nsecond\r\nthird\r\nfourth\r\nfifth".as_bytes());
    let found = vt.search("实现反馈").unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].line.contains("实现反馈 e\u{301}"));
    // Four wide characters after "old ": columns 4..12, so a highlight covers them all.
    assert_eq!((found[0].row, found[0].col, found[0].cols), (0, 4, 8));
    assert_eq!(
        vt.search("d")
            .unwrap()
            .iter()
            .map(|m| (m.row, m.col))
            .collect::<Vec<_>>(),
        [(0, 2), (1, 5), (2, 4)]
    );
    assert!(vt.search("absent").unwrap().is_empty());
    assert!(vt.search("").unwrap().is_empty());
}
#[test]
fn search_shares_each_rows_text_and_keeps_every_occurrence() {
    let mut vt = Vt::new(40, 3).unwrap();
    vt.feed(b"aaaaa  \r\naaa  ");
    let found = vt.search("a").unwrap();
    assert_eq!(
        found
            .iter()
            .map(|m| (m.row, m.col, m.cols))
            .collect::<Vec<_>>(),
        [
            (0, 0, 1),
            (0, 1, 1),
            (0, 2, 1),
            (0, 3, 1),
            (0, 4, 1),
            (1, 0, 1),
            (1, 1, 1),
            (1, 2, 1)
        ]
    );
    assert!(
        found[..5]
            .iter()
            .all(|m| Arc::ptr_eq(&m.line, &found[0].line))
    );
    assert!(
        found[5..]
            .iter()
            .all(|m| Arc::ptr_eq(&m.line, &found[5].line))
    );
    assert!(!Arc::ptr_eq(&found[0].line, &found[5].line));
    let copy = found.clone();
    assert!(Arc::ptr_eq(&copy[0].line, &found[0].line));
    drop(vt);
    assert_eq!(&*copy[0].line, "aaaaa");
    assert_eq!(&*copy[5].line, "aaa");
}
#[test]
fn viewport_counts_rows_from_the_top_of_history() {
    let mut vt = Vt::new(20, 3).unwrap();
    vt.feed(b"line-1\r\nline-2\r\nline-3\r\nline-4\r\nline-5");
    assert_eq!(
        vt.viewport().unwrap(),
        (2, 5, 3),
        "two rows scrolled into history"
    );
    vt.scroll(libghostty_vt::terminal::ScrollViewport::Top);
    assert_eq!(vt.viewport().unwrap().0, 0);
    assert_eq!(vt.search("line-1").unwrap()[0].row, 0);
}
fn replies_to(vt: &mut Vt, query: &[u8]) -> String {
    let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = got.clone();
    vt.on_reply(move |b| sink.lock().unwrap().extend_from_slice(b))
        .unwrap();
    vt.feed(query);
    String::from_utf8(got.lock().unwrap().clone()).unwrap()
}
#[test]
fn answers_the_queries_every_terminal_answers() {
    let cell = CellPixels::default();
    let mut vt = Vt::new(80, 24).unwrap();
    vt.answer_queries(cell.clone()).unwrap();
    cell.set(16, 34);
    for (query, reply) in [
        (&b"\x1b]10;?\x1b\\"[..], "\x1b]10;rgb:d4d4/d4d4/d8d8\x1b\\"),
        (b"\x1b]11;?\x07", "\x1b]11;rgb:0e0e/0e0e/1111\x07"),
        (b"\x1b]12;?\x07", "\x1b]12;rgb:2f2f/8181/f7f7\x07"),
        (b"\x1b]4;1;?\x07", "\x1b]4;1;rgb:"),
        (b"\x1b[c", "\x1b[?62;22c"),
        (b"\x1b[>c", "\x1b[>1;10;0c"),
        (b"\x1b[>q", "\x1bP>|fibo "),
        (b"\x1b[18t", "\x1b[8;24;80t"),
        (b"\x1b[16t", "\x1b[6;34;16t"),
        (b"\x1b[14t", "\x1b[4;816;1280t"),
        (b"\x1b[?996n", "\x1b[?997;1n"),
        (
            b"\x1bP+q544e\x1b\\",
            "\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\",
        ),
    ] {
        let got = replies_to(&mut vt, query);
        assert!(got.starts_with(reply), "{query:?}: got {got:?}");
    }
    // A program's own OSC 11 wins over the default, as in any terminal.
    vt.feed(b"\x1b]11;#ffffff\x07");
    assert_eq!(
        replies_to(&mut vt, b"\x1b]11;?\x07"),
        "\x1b]11;rgb:ffff/ffff/ffff\x07"
    );
}
#[test]
fn clear_history_keeps_the_prompt_row_at_the_top() {
    let mut vt = Vt::new(20, 4).unwrap();
    for i in 0..10 {
        vt.feed(format!("line{i}\r\n").as_bytes());
    }
    vt.feed(b"\x1b[31mhalf");
    vt.feed(b"\x1b[3"); // the program's output stops mid-sequence
    vt.feed(b"$ ");
    let clear = clear_history(vt.terminal()).unwrap();
    vt.feed(&clear);
    assert_eq!(vt.terminal().scrollback_rows().unwrap(), 0);
    assert_eq!(vt.terminal().cursor_y().unwrap(), 0);
    let text = vt.tail_text(4).unwrap();
    assert!(text.starts_with("half"), "{text:?}");
    assert!(!text.contains("line"), "{text:?}");
    vt.feed(b"echo");
    assert!(vt.tail_text(4).unwrap().starts_with("halfecho"));
    // Full-screen programs keep their screen.
    vt.feed(b"\x1b[?1049hTUI");
    assert!(clear_history(vt.terminal()).unwrap().is_empty());
}
#[test]
fn bells_count_real_bel_only_and_titles_survive_snapshots() {
    let rung = Arc::new(AtomicU64::new(0));
    let count = rung.clone();
    let mut vt = Vt::new(80, 24).unwrap();
    vt.on_bell(move || {
        count.fetch_add(1, Ordering::Relaxed);
    })
    .unwrap();
    assert_eq!(vt.title(), None);
    vt.feed(b"\x1b]0;~/code \xe2\x80\x94 zsh\x07"); // BEL ends the OSC: not a bell
    vt.feed(b"done\x07\x07");
    assert_eq!(rung.load(Ordering::Relaxed), 2);
    assert_eq!(vt.title().as_deref(), Some("~/code — zsh"));
    let restored = Vt::restore(&vt.snapshot().unwrap()).unwrap();
    assert_eq!(restored.title().as_deref(), Some("~/code — zsh"));
    vt.feed(b"\x1b]2;\x1b\\");
    assert_eq!(vt.title(), None);
}
#[test]
fn light_scheme_repaints_defaults_and_answers_light() {
    let mut vt = Vt::new(80, 24).unwrap();
    let replies = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = replies.clone();
    vt.on_reply(move |bytes| captured.lock().unwrap().extend_from_slice(bytes))
        .unwrap();
    vt.answer_queries(CellPixels::default()).unwrap();
    vt.set_scheme(Scheme::Light).unwrap();
    let bg = vt.terminal().bg_color().unwrap().unwrap();
    assert_eq!(rgb(LIGHT_BG), bg);
    let white = vt.terminal().color_palette().unwrap().0[7];
    assert_eq!(rgb(Scheme::Light.colors().ansi()[7].unwrap()), white);
    vt.feed(b"\x1b[?996n");
    assert_eq!(replies.lock().unwrap().as_slice(), b"\x1b[?997;2n");
    vt.set_scheme(Scheme::Dark).unwrap();
    let restored = Vt::restore(&vt.snapshot().unwrap()).unwrap();
    let bg = restored.terminal().bg_color().unwrap().unwrap();
    assert_eq!(rgb(DEFAULT_BG), bg);
}
#[test]
fn a_themes_colors_are_what_programs_read_back() {
    let mut ansi = [None; 16];
    ansi[1] = Some(0xff4040);
    let colors = TermColors::new(0xd5dfef, 0x0f1c2e, Some(0xff00aa), ansi);
    let mut vt = Vt::new(80, 24).unwrap();
    vt.set_colors(colors).unwrap();
    let replies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = replies.clone();
    vt.on_reply(move |bytes| captured.lock().unwrap().extend_from_slice(bytes))
        .unwrap();
    vt.feed(b"\x1b]11;?\x07\x1b]10;?\x07\x1b]4;1;?\x07\x1b]4;2;?\x07");
    let text = String::from_utf8(replies.lock().unwrap().clone()).unwrap();
    assert!(text.contains("11;rgb:0f0f/1c1c/2e2e"), "{text:?}");
    assert!(text.contains("10;rgb:d5d5/dfdf/efef"), "{text:?}");
    assert!(text.contains("4;1;rgb:ffff/4040/4040"), "{text:?}");
    // What it leaves out stays the terminal's own.
    let [r, g, b] = [16, 8, 0].map(|s| (builtin_ansi()[2] >> s) & 255);
    let own = format!("4;2;rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}");
    assert!(text.contains(&own), "{text:?} {own}");
    // Switching back leaves nothing of the theme behind.
    vt.set_scheme(Scheme::Dark).unwrap();
    replies.lock().unwrap().clear();
    vt.feed(b"\x1b]4;1;?\x07");
    let [r, g, b] = [16, 8, 0].map(|s| (builtin_ansi()[1] >> s) & 255);
    let text = String::from_utf8(replies.lock().unwrap().clone()).unwrap();
    assert!(
        text.contains(&format!(
            "4;1;rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}"
        )),
        "{text:?}"
    );
}
#[test]
fn display_copies_keep_default_colors() {
    let mut vt = Vt::new(80, 24).unwrap();
    let restored = Vt::restore(&vt.snapshot().unwrap()).unwrap();
    let bg = restored.terminal().bg_color().unwrap().unwrap();
    let [_, r, g, b] = DEFAULT_BG.to_be_bytes();
    assert_eq!((bg.r, bg.g, bg.b), (r, g, b));
}
#[test]
fn untrusted_osc_and_malformed_bytes_remain_terminal_data() {
    let mut vt = Vt::new(80, 24).unwrap();
    let replies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = replies.clone();
    vt.on_reply(move |bytes| captured.lock().unwrap().extend_from_slice(bytes))
        .unwrap();
    vt.feed(b"\x1b]52;c;c2VjcmV0\x07\x1b]52;c;?\x07");
    vt.feed(b"\x1b]8;;javascript:alert(1)\x07LINK\x1b]8;;\x07\r\n");
    vt.feed(b"\xff\xfe\x1b[999999999999m\x1b[0mRECOVERED");
    assert!(
        replies.lock().unwrap().is_empty(),
        "no clipboard response callback is installed"
    );
    assert_eq!(vt.search("LINK").unwrap().len(), 1);
    assert_eq!(vt.search("RECOVERED").unwrap().len(), 1);
    assert!(vt.search("javascript").unwrap().is_empty());
    let restored = Vt::restore(&vt.snapshot().unwrap()).unwrap();
    assert_eq!(restored.search("RECOVERED").unwrap().len(), 1);
}
#[test]
fn drops_leading_activity_glyphs_only() {
    assert_eq!(shown_title("✳ 你好啊").as_deref(), Some("你好啊"));
    assert_eq!(shown_title("⠋ fix login").as_deref(), Some("fix login"));
    assert_eq!(shown_title("·x").as_deref(), Some("·x"));
    assert_eq!(shown_title("~/code — zsh").as_deref(), Some("~/code — zsh"));
    assert_eq!(shown_title(" ⠙ "), None);
    assert_eq!(shown_title(""), None);
}
#[test]
fn truncated_snapshot_is_not_a_ready_terminal() {
    let mut source = Vt::new(80, 24).unwrap();
    source.feed(b"hello");
    let mut bytes = source.snapshot().unwrap();
    bytes.truncate(bytes.len() - 10);
    assert!(Vt::restore(&bytes).is_err());
}
