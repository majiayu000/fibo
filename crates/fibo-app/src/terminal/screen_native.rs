use super::*;

/// Run by the isolated native-render harness, using macOS fonts rather than GPUI's
/// mock text system. Both IME range geometry and the painted overlay use this layout.
#[cfg(feature = "visual-test")]
pub fn verify_native_geometry(window: &Window, cx: &App) {
    let font = crate::ui::theme::Theme::mono(cx);
    let metrics = CellMetrics::measure(&font, window).expect("native font measurement");
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut face = font.clone();
        if bold {
            face.weight = FontWeight::BOLD;
        }
        if italic {
            face.style = FontStyle::Italic;
        }
        let cached =
            AsciiGlyphs::new(face.clone(), metrics, window).expect("native mono ASCII cache");
        for text in [
            "0123456789 row 01",
            "!=>->==fiWWW_[]{}~",
            "ASCII with spaces",
        ] {
            let run = Run {
                col: 0,
                cols: text.len() as u16,
                text: text.into(),
                fg: DEFAULT_FG,
                bg: None,
                attrs: Attrs::default(),
            };
            let (line, _) = shape_cell(&run, face.clone(), metrics, window);
            assert_eq!(
                cached.baseline,
                (metrics.height - line.ascent - line.descent) / 2. + line.ascent
            );
            for shaped in &line.runs {
                for g in &shaped.glyphs {
                    assert_eq!(
                        cached.glyphs[(text.as_bytes()[g.index] - b' ') as usize],
                        (shaped.font_id, g.id)
                    );
                    assert!(
                        f32::from(g.position.x - metrics.advance * g.index as f32).abs() < 0.02
                    );
                }
            }
        }
    }
    for ch in [
        '\u{e0b0}', '\u{e0b2}', '\u{e0a0}', '\u{f120}', '\u{f07b}', '\u{f013}',
    ] {
        let run = Run {
            col: 0,
            cols: 1,
            text: ch.to_string(),
            fg: DEFAULT_FG,
            bg: None,
            attrs: Attrs::default(),
        };
        let (line, offset) = shape_cell(&run, font.clone(), metrics, window);
        assert!(
            line.runs.iter().any(|r| !r.glyphs.is_empty())
                && line
                    .runs
                    .iter()
                    .flat_map(|r| &r.glyphs)
                    .all(|g| g.id.0 != 0),
            "missing symbol {ch}"
        );
        assert!(
            (f32::from(line.width() + offset * 2. - metrics.width)).abs() < 0.1,
            "symbol not aligned: {ch}"
        );
    }
    eprintln!("native bundled symbol font: six Nerd/Powerline glyphs present and cell-aligned");
    let pane = Bounds::new(point(px(37.), px(83.)), size(px(700.), px(500.)));
    let anchor = metrics.cursor_bounds(pane, (11, 5));
    assert_eq!(
        anchor.origin,
        point(
            pane.left() + metrics.width * 11.,
            pane.top() + metrics.height * 5.
        )
    );
    for text in ["nihao", not_ui!("n你😀hao"), "e\u{301}", "👨‍👩‍👧‍👦🇨🇳👍🏽"]
    {
        let marked = MarkedText::new(text, font.clone(), anchor, window);
        assert_eq!(marked.bounds.origin, anchor.origin);
        let whole = marked.range_bounds(0..text.encode_utf16().count());
        assert_eq!(whole.origin, marked.bounds.origin);
        assert_eq!(whole.size.width, marked.line.width());
        assert_eq!(whole.size.height, metrics.height);
        for (byte, _) in text.char_indices() {
            let index = text[..byte].encode_utf16().count();
            let caret = marked.range_bounds(index..index);
            assert_eq!(
                caret.left(),
                marked.bounds.left() + marked.line.x_for_index(byte)
            );
            assert_eq!(caret.bottom(), marked.bounds.bottom());
            assert_eq!(caret.size.width, px(0.));
            let suffix = marked.range_bounds(index..text.encode_utf16().count());
            assert_eq!(suffix.left(), caret.left());
            // origin + width re-adds the caret offset; widths like 7.8 are not binary-exact.
            let end = marked.bounds.left() + marked.line.width();
            assert!(
                f32::from(suffix.right() - end).abs() < 1e-3,
                "{text}: {suffix:?}"
            );
        }
    }
    for text in [
        not_ui!("实"),
        not_ui!("现"),
        not_ui!("反"),
        not_ui!("馈"),
        not_ui!("日"),
        not_ui!("語"),
        "한",
        "글",
        "😀",
    ] {
        let run = Run {
            col: 0,
            cols: 2,
            text: text.into(),
            fg: DEFAULT_FG,
            bg: None,
            attrs: Attrs::default(),
        };
        let (line, offset) = shape_cell(&run, font.clone(), metrics, window);
        let target = metrics.width * run.cols as f32;
        assert!(
            (f32::from(line.width() + offset * 2. - target)).abs() < 0.1,
            "wide cell {text}: advance={:?} inset={offset:?} two-columns={target:?}",
            line.width()
        );
        assert!(
            line.width() <= target + px(0.1),
            "wide glyph overflow {text}"
        );
        eprintln!(
            "wide-cell {text}: advance={:?}, two-columns={target:?}, inset={offset:?}",
            line.width()
        );
    }
    eprintln!("native IME geometry: overlay/ranges/UTF16 agree; measured cell={metrics:?}");
}
