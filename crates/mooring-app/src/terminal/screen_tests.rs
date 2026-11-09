use super::*;

#[test]
fn zoom_steps_by_one_point_and_resets() {
    let base = crate::ui::theme::Theme::TERMINAL_FONT_SIZE;
    assert_eq!(zoomed(base, Some(1)), base + 1.);
    assert_eq!(zoomed(base + 1., Some(-1)), base);
    assert_eq!(zoomed(30., None), base);
    assert_eq!(zoomed(36., Some(1)), 36.);
    assert_eq!(zoomed(8., Some(-1)), 8.);
}
#[test]
fn retained_capture_keeps_the_selection_on_its_text_through_scrolling() {
    use mooring_core::terminal::libghostty_vt::{
        selection::SelectLineOptions,
        terminal::{Point, PointCoordinate, ScrollViewport},
    };
    let mut vt = Vt::new(20, 3).unwrap();
    vt.feed(b"a\r\nb\r\nc\r\nd\r\ne");
    let mut capture = Capture::new().unwrap();
    let mut screen = Screen::default();
    capture.update(&vt, &mut screen).unwrap();
    // Select "d", on viewport row 1.
    let t = vt.terminal();
    let at = t
        .grid_ref(Point::Viewport(PointCoordinate { x: 0, y: 1 }))
        .unwrap();
    let line = t.select_line(SelectLineOptions::new(at)).unwrap();
    t.set_selection(line.as_ref()).unwrap();
    capture.update(&vt, &mut screen).unwrap();
    assert_eq!(screen.selection, [None, Some((0, 0)), None]);
    // Scrolling the view up one row moves "d" and its highlight down one row.
    vt.scroll(ScrollViewport::Delta(-1));
    capture.update(&vt, &mut screen).unwrap();
    assert_eq!(screen.selection, [None, None, Some((0, 0))]);
    // New output pushes "d" up into history; the highlight goes with it.
    vt.scroll(ScrollViewport::Bottom);
    vt.feed(b"\r\nf");
    capture.update(&vt, &mut screen).unwrap();
    assert_eq!(screen.selection, [Some((0, 0)), None, None]);
    vt.terminal().set_selection(None).unwrap();
    capture.update(&vt, &mut screen).unwrap();
    assert_eq!(screen.selection, [None, None, None]);
}
#[test]
fn retained_capture_rebuilds_only_dirty_rows_and_clears_old_links() {
    let mut vt = Vt::new(40, 10).unwrap();
    vt.feed(b"\x1b[3;1H");
    let mut capture = Capture::new().unwrap();
    let mut screen = Screen::default();
    assert_eq!(capture.update(&vt, &mut screen).unwrap(), 10);
    assert_eq!(capture.update(&vt, &mut screen).unwrap(), 0);
    vt.feed(b"\x1b[3;1H\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\");
    assert_eq!(capture.update(&vt, &mut screen).unwrap(), 1);
    assert!(screen.link_at(2, 0).is_some());
    let unchanged = screen.rows[0].as_ptr();
    vt.feed(b"\x1b[3;1H\x1b[2Kplain");
    assert_eq!(capture.update(&vt, &mut screen).unwrap(), 1);
    assert!(screen.links.is_empty());
    assert_eq!(screen.rows[0].as_ptr(), unchanged);
    assert_eq!(capture.update(&vt, &mut screen).unwrap(), 0);
}

#[test]
fn retained_capture_matches_full_capture_after_resize_scroll_palette_and_alternate_screen() {
    let mut vt = Vt::new(40, 5).unwrap();
    let mut reader = Capture::new().unwrap();
    let mut screen = Screen::default();
    for bytes in [
        b"one\r\ntwo\r\nthree".as_slice(),
        b"\x1b[?1049hALT",
        b"\x1b[?1049l",
        b"\x1b]10;#123456\x07",
        b"\x1b[31mred\x1b]4;1;#abcdef\x07",
        b"\r\n4\r\n5\r\n6\r\n7",
    ] {
        vt.feed(bytes);
        reader.update(&vt, &mut screen).unwrap();
        let full = capture(&vt).unwrap();
        assert_eq!(screen.rows, full.rows);
        assert_eq!(screen.cursor, full.cursor);
        assert_eq!(screen.links, full.links);
    }
    vt.resize(20, 3).unwrap();
    reader.update(&vt, &mut screen).unwrap();
    assert_eq!(screen.rows, capture(&vt).unwrap().rows);
    vt.scroll(libghostty_vt::terminal::ScrollViewport::Top);
    reader.update(&vt, &mut screen).unwrap();
    assert_eq!(screen.rows, capture(&vt).unwrap().rows);
}

#[test]
fn osc_links_keep_cell_bounds_and_reject_executable_schemes() {
    let mut vt = Vt::new(40, 3).unwrap();
    vt.feed(
        "\x1b]8;;https://example.com\x1b\\中文\x1b]8;;\x1b\\ \x1b]8;;javascript:alert(1)\x1b\\bad"
            .as_bytes(),
    );
    let s = capture(&vt).unwrap();
    assert_eq!(s.link_at(0, 3).unwrap().uri, "https://example.com");
    assert!(s.link_at(0, 4).is_none());
    assert!(s.link_at(0, 5).is_none());
    for uri in [
        "javascript:alert(1)",
        "data:text/plain,a",
        "https://",
        "https://a\n--arg",
        "file:///",
    ] {
        assert!(!allowed_link(uri));
    }
    assert!(allowed_link("file:///tmp/test"));
}
#[test]
fn plain_text_urls_become_links_without_the_prose_around_them() {
    let mut vt = Vt::new(80, 6).unwrap();
    vt.feed(
        "端口：http://100.113.31.23:3002/radar?s=rust。\r\n\
         (see https://en.wikipedia.org/wiki/Rust_(language)).\r\n\
         xhttps://no.example 和 **https://b.example/x**\r\n\
         \x1b]8;;https://osc.example\x1b\\https://shown.example\x1b]8;;\x1b\\"
            .as_bytes(),
    );
    let s = capture(&vt).unwrap();
    let link = s.link_at(0, 6).unwrap();
    assert_eq!(link.uri, "http://100.113.31.23:3002/radar?s=rust");
    assert_eq!((link.col, link.cols), (6, 38));
    assert!(s.link_at(0, 44).is_none());
    assert_eq!(
        s.link_at(1, 5).unwrap().uri,
        "https://en.wikipedia.org/wiki/Rust_(language)"
    );
    assert!(s.link_at(2, 1).is_none());
    assert_eq!(s.link_at(2, 25).unwrap().uri, "https://b.example/x");
    // An OSC 8 link keeps its own target over the URL its text shows.
    let osc: Vec<_> = s.links.iter().filter(|l| l.row == 3).collect();
    assert!(osc.iter().all(|l| l.uri == "https://osc.example"));
}
#[test]
fn a_path_broken_over_rows_is_one_word() {
    fn piece(s: &Screen, row: u16) -> Vec<(u16, u16, &str, Option<&str>)> {
        s.paths
            .iter()
            .filter(|p| p.row == row)
            .map(|p| (p.col, p.cols, p.uri.as_str(), p.alone.as_deref()))
            .collect()
    }
    // The terminal wraps it: a soft wrap.
    let mut vt = Vt::new(20, 4).unwrap();
    vt.feed(b"see /tmp/aaaa/bbbbbbbbbbbb.rs:3 ok");
    let s = capture(&vt).unwrap();
    let whole = "/tmp/aaaa/bbbbbbbbbbbb.rs:3";
    assert_eq!(piece(&s, 0), [(4, 16, whole, Some("/tmp/aaaa/bbbbbb"))]);
    assert_eq!(piece(&s, 1), [(0, 11, whole, Some("bbbbbb.rs:3"))]);
    assert!(s.paths.iter().all(|p| p.start == (0, 4)));
    // An Agent breaks it at the edge itself and indents the rest.
    let mut vt = Vt::new(24, 4).unwrap();
    vt.feed("- 见 /Users/me/project/a\r\n  bc/def.rs 和其他\r\nshort a.rs\r\nrest".as_bytes());
    let s = capture(&vt).unwrap();
    let whole = "/Users/me/project/abc/def.rs";
    assert_eq!(piece(&s, 0), [(5, 19, whole, Some("/Users/me/project/a"))]);
    assert_eq!(piece(&s, 1), [(2, 9, whole, Some("bc/def.rs"))]);
    // A row that stops short of the edge does not run on.
    assert_eq!(piece(&s, 2), [(6, 4, "a.rs", None)]);
    assert!(piece(&s, 3).is_empty());
}
#[test]
fn path_words_are_found_without_prose_or_versions() {
    let mut vt = Vt::new(100, 4).unwrap();
    vt.feed(
        "- 把 3 个 Codex 的 skill 软链接到了 ~/.claude/skills。\r\n\
         见 `terminal/screen.rs:72`, (a.rs:42:7) 和 /tmp/x.\r\n\
         Opus 5.5、v0.1.41 和 full_v18.mp4 https://a.example/b 1/2"
            .as_bytes(),
    );
    let s = capture(&vt).unwrap();
    let words: Vec<_> = s.paths.iter().map(|p| (p.row, p.uri.as_str())).collect();
    assert_eq!(
        words,
        [
            (0, "~/.claude/skills"),
            (1, "terminal/screen.rs:72"),
            (1, "a.rs:42:7"),
            (1, "/tmp/x"),
            (2, "full_v18.mp4"),
            (2, "1/2"),
        ]
    );
    let skills = s.path_at(0, 44).unwrap();
    assert_eq!((skills.col, skills.cols), (36, 16));
    assert_eq!(split_line("a.rs:42:7"), ("a.rs", Some(42)));
    assert_eq!(split_line("a.rs:42"), ("a.rs", Some(42)));
    assert_eq!(split_line("C:x"), ("C:x", None));
    assert_eq!(split_position("a.rs:42:7"), ("a.rs", Some((42, Some(7)))));
    assert_eq!(split_position("a.rs:42"), ("a.rs", Some((42, None))));
    let cwd = std::path::Path::new("/work");
    assert_eq!(
        resolve_path("src/a.rs:3", Some(cwd)).unwrap(),
        cwd.join("src/a.rs")
    );
    assert_eq!(
        resolve_path("/etc/hosts", None).unwrap(),
        std::path::Path::new("/etc/hosts")
    );
    assert!(resolve_path("a.rs", None).is_none());
}
#[test]
fn emoji_cluster_capture_is_one_two_column_run() {
    let mut vt = Vt::new(40, 3).unwrap();
    vt.feed("👨‍👩‍👧‍👦🇨🇳X".as_bytes());
    let s = capture(&vt).unwrap();
    assert_eq!(s.rows[0][0].text, "👨‍👩‍👧‍👦");
    assert_eq!(s.rows[0][0].cols, 2);
    assert_eq!(s.rows[0][1].col, 2);
    assert_eq!(s.cursor, Some((5, 0)));
}
#[test]
fn the_cursor_blinks_until_a_program_asks_for_a_steady_one() {
    let mut vt = Vt::new(80, 24).unwrap();
    assert!(capture(&vt).unwrap().cursor_blinking);
    vt.feed(b"\x1b[2 q");
    assert!(!capture(&vt).unwrap().cursor_blinking);
    vt.feed(b"\x1b[0 q");
    assert!(capture(&vt).unwrap().cursor_blinking);
}
#[test]
fn hidden_cursor_keeps_ime_anchor_and_missing_position_keeps_last_known() {
    let mut vt = Vt::new(80, 24).unwrap();
    vt.feed(b"\x1b[6;12H\x1b[?25l");
    let screen = capture(&vt).unwrap();
    assert_eq!(screen.cursor, Some((11, 5)));
    assert!(!screen.cursor_visible);
    let mut anchor = CursorAnchor::default();
    assert_eq!(anchor.get(), None);
    anchor.observe(screen.cursor);
    anchor.observe(None);
    assert_eq!(anchor.get(), Some((11, 5)));
    anchor.observe(Some((7, 3)));
    assert_eq!(anchor.get(), Some((7, 3)));
}

#[test]
fn captured_cjk_occupies_two_columns_and_combining_text_stays_separate() {
    let mut vt = Vt::new(80, 24).unwrap();
    vt.feed("A实现反馈日本語한글B e\u{301}".as_bytes());
    let screen = capture(&vt).unwrap();
    let runs: Vec<_> = screen.rows[0]
        .iter()
        .filter(|r| !r.text.trim().is_empty())
        .collect();
    assert_eq!(runs[0].text, "A");
    for (i, expected) in "实现反馈日本語한글".chars().enumerate() {
        assert_eq!(runs[i + 1].text, expected.to_string());
        assert_eq!(runs[i + 1].col, 1 + i as u16 * 2);
        assert_eq!(runs[i + 1].cols, 2);
    }
    assert!(runs.iter().any(|r| r.text == "e\u{301}" && r.cols == 1));
}

/// Per-face metrics read straight from the font file (no GPUI window in unit tests).
struct FaceMetrics {
    units_per_em: f32,
    line_height: f32,
    advance_m: f32,
}
fn be16(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}
fn face_metrics(d: &[u8]) -> FaceMetrics {
    // First face of a collection (Menlo.ttc face 0 is Regular).
    let face = if &d[..4] == b"ttcf" {
        u32::from_be_bytes(d[12..16].try_into().unwrap()) as usize
    } else {
        0
    };
    let table = |tag: &[u8; 4]| {
        (0..be16(d, face + 4) as usize)
            .map(|i| face + 12 + 16 * i)
            .find(|&r| &d[r..r + 4] == tag)
            .map(|r| u32::from_be_bytes(d[r + 8..r + 12].try_into().unwrap()) as usize)
            .expect("font table")
    };
    let (head, hhea, os2, cmap, hmtx) = (
        table(b"head"),
        table(b"hhea"),
        table(b"OS/2"),
        table(b"cmap"),
        table(b"hmtx"),
    );
    let i16_at = |o| be16(d, o) as i16 as f32;
    // Ghostty face/coretext.zig: typo metrics if USE_TYPO_METRICS, else hhea, else typo, else win.
    let (hhea_asc, hhea_desc, hhea_gap) = (i16_at(hhea + 4), i16_at(hhea + 6), i16_at(hhea + 8));
    let (typo_asc, typo_desc, typo_gap) = (i16_at(os2 + 68), i16_at(os2 + 70), i16_at(os2 + 72));
    let line_height = if be16(d, os2 + 62) & 0x80 != 0 {
        typo_asc - typo_desc + typo_gap
    } else if hhea_asc != 0. || hhea_desc != 0. {
        hhea_asc - hhea_desc + hhea_gap
    } else if typo_asc != 0. || typo_desc != 0. {
        typo_asc - typo_desc + typo_gap
    } else {
        be16(d, os2 + 74) as f32 + be16(d, os2 + 76) as f32
    };
    // cmap format 4 lookup of 'm', then its hmtx advance (what CellMetrics::measure reads).
    let sub = (0..be16(d, cmap + 2) as usize)
        .map(|i| cmap + 4 + 8 * i)
        // Unicode platform (Menlo ships only 0/3) or Windows Unicode BMP.
        .filter(|&r| matches!((be16(d, r), be16(d, r + 2)), (0, _) | (3, 1)))
        .map(|r| cmap + u32::from_be_bytes(d[r + 4..r + 8].try_into().unwrap()) as usize)
        .find(|&s| be16(d, s) == 4)
        .expect("cmap format 4");
    let segs = be16(d, sub + 6) as usize;
    let ch = 'm' as u16;
    let seg = (0..segs / 2)
        .find(|&i| be16(d, sub + 14 + 2 * i) >= ch)
        .unwrap();
    let start = be16(d, sub + 16 + segs + 2 * seg);
    assert!(start <= ch, "'m' mapped");
    let delta = be16(d, sub + 16 + 2 * segs + 2 * seg);
    let range_at = sub + 16 + 3 * segs + 2 * seg;
    let range = be16(d, range_at) as usize;
    let glyph = if range == 0 {
        ch.wrapping_add(delta)
    } else {
        be16(d, range_at + range + 2 * (ch - start) as usize).wrapping_add(delta)
    } as usize;
    let metrics = be16(d, hhea + 34) as usize;
    FaceMetrics {
        units_per_em: be16(d, head + 18) as f32,
        line_height,
        advance_m: be16(d, hmtx + 4 * glyph.min(metrics - 1)) as f32,
    }
}

/// Ghostty sizes the cell from the face (font/Metrics.zig): height = round(ascent - descent
/// + line_gap), width = round(advance), in backing pixels. Mooring uses the same face
///   height; a fixed multiplier broke when the face changed (1.2 was -6% for JetBrains Mono,
///   1.6 stretched the Codex logo ~37%) while internal-consistency checks still passed.
#[test]
fn cell_aspect_ratio_stays_within_five_percent_of_ghostty_for_the_resolved_font() {
    // Mirror Theme::mono: the bundled JetBrains Mono, else Menlo. SF Mono is a hidden
    // system face that GPUI does not list, so stock macOS resolves Menlo (measured by
    // examples/terminal_reference).
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/JetBrainsMono-Regular.ttf"
        )
        .into(),
        format!("{home}/Library/Fonts/JetBrainsMono-Regular.ttf"),
        "/Library/Fonts/JetBrainsMono-Regular.ttf".into(),
        "/System/Library/Fonts/Menlo.ttc".into(),
    ];
    let Some(path) = candidates.iter().find(|p| std::path::Path::new(p).exists()) else {
        eprintln!("no candidate monospace font on this system; skipped");
        return;
    };
    let face = face_metrics(&std::fs::read(path).unwrap());
    let backing = font_size() * 2.; // Retina: Ghostty rasterizes at points * scale.
    let ghostty_w = (face.advance_m / face.units_per_em * backing).round();
    let ghostty_h = (face.line_height / face.units_per_em * backing).round();
    if path.ends_with("Menlo.ttc") {
        // Anchors the parser to the Ghostty 1.3.1 capture (eval/terminal-reference).
        assert_eq!(
            (ghostty_w, ghostty_h),
            (15., 29.),
            "Ghostty Menlo 12.5pt @2x cell"
        );
    }
    let ghostty = ghostty_h / ghostty_w;
    let mooring = |line_height: f32| {
        (font_size() * line_height) / (face.advance_m / face.units_per_em * font_size())
    };
    let face_height = face.line_height / face.units_per_em;
    let delta = mooring(face_height) / ghostty - 1.;
    assert!(
        delta.abs() <= 0.05,
        "{path}: Fibo cell h/w {:.3} vs Ghostty {ghostty:.3} ({:+.1}%)",
        mooring(face_height),
        delta * 100.
    );
    assert!(
        mooring(1.2) / ghostty - 1. < -0.05 || path.ends_with("Menlo.ttc"),
        "a fixed 1.2 multiplier must stay detectable for JetBrains Mono"
    );
    assert!(
        mooring(1.6) / ghostty - 1. > 0.05,
        "the check must catch the 1.6 regression"
    );
}

#[test]
fn sgr_decorations_and_faint_survive_capture() {
    let mut vt = Vt::new(80, 24).unwrap();
    vt.feed(b"\x1b[2ma\x1b[0m\x1b[4:3;58;5;1mb\x1b[0m\x1b[9;53mc\x1b[0m\x1b[8md\x1b[21me");
    let screen = capture(&vt).unwrap();
    let attrs: Vec<Attrs> = screen.rows[0].iter().take(5).map(|r| r.attrs).collect();
    assert!(attrs[0].faint);
    assert_eq!(attrs[1].underline, Underline::Curly);
    assert!(attrs[1].underline_color.is_some());
    assert!(attrs[2].strikethrough && attrs[2].overline);
    assert!(attrs[3].invisible);
    assert_eq!(attrs[4].underline, Underline::Double);
}

#[test]
fn decscusr_shape_reaches_the_painter() {
    let mut vt = Vt::new(80, 24).unwrap();
    for (seq, shape) in [
        (&b"\x1b[6 q"[..], CursorShape::Bar),
        (b"\x1b[4 q", CursorShape::Underline),
        (b"\x1b[2 q", CursorShape::Block),
    ] {
        vt.feed(seq);
        assert_eq!(capture(&vt).unwrap().cursor_shape, shape);
    }
}

#[test]
fn ascii_cache_never_treats_wide_or_combining_text_as_independent_cells() {
    for (text, cols, expected) in [
        ("hello 123", 9, true),
        ("你", 2, false),
        ("e\u{301}", 1, false),
        ("\t", 1, false),
        ("ab", 1, false),
    ] {
        let run = Run {
            col: 0,
            cols,
            text: text.into(),
            fg: DEFAULT_FG,
            bg: None,
            attrs: Attrs::default(),
        };
        assert_eq!(printable_cells(&run), expected);
    }
}

#[test]
fn ime_offsets_are_utf16_not_utf8_and_never_split_surrogates() {
    let text = "n你😀hao";
    assert_eq!(utf16_byte_offset(text, 1), 1);
    assert_eq!(utf16_byte_offset(text, 2), 4);
    assert_eq!(utf16_byte_offset(text, 3), 4);
    assert_eq!(utf16_byte_offset(text, 4), 8);
    assert_eq!(utf16_byte_offset(text, usize::MAX), text.len());
}

/// JetBrains Mono has no ideographs. Windows answered with its placeholder box, 0.6 em wide, and
/// Chinese came out at 0.6 of its size with gaps between the characters.
#[test]
fn a_placeholder_box_is_not_the_width_of_a_character() {
    let (box_width, ideograph) = (px(7.8), px(13.));
    assert_eq!(
        own_glyph_width(Some(ideograph), Some(box_width)),
        Some(ideograph)
    );
    assert_eq!(own_glyph_width(Some(box_width), Some(box_width)), None);
    // Where the system says it has no such character.
    assert_eq!(own_glyph_width(None, None), None);
    assert_eq!(own_glyph_width(Some(ideograph), None), Some(ideograph));
    assert_eq!(own_glyph_width(Some(px(0.)), None), None);
}
