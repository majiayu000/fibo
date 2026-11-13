use super::*;
// Printable ASCII has no shaping context when terminal ligatures are disabled.
// Shape this bounded alphabet once per face/style, then reuse the exact native
// glyph IDs. Non-ASCII and fonts whose advances aren't one cell retain shaping.
pub(super) struct AsciiGlyphs {
    pub(super) glyphs: [(gpui::FontId, gpui::GlyphId); 95],
    pub(super) baseline: Pixels,
}
impl AsciiGlyphs {
    pub(super) fn new(font: Font, metrics: CellMetrics, window: &Window) -> Option<Self> {
        if !["liga", "calt"].iter().all(|tag| {
            font.features
                .tag_value_list()
                .iter()
                .any(|(name, value)| name == tag && *value == 0)
        }) {
            return None;
        }
        let text: String = (' '..='~').collect();
        let line = window.text_system().shape_line(
            text.into(),
            px(font_size()),
            &[TextRun {
                len: 95,
                font,
                color: hsla(crate::ui::theme::Theme::terminal_fg()),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let mut glyphs = [None; 95];
        for run in &line.runs {
            for glyph in &run.glyphs {
                if glyph.index >= 95
                    || glyphs[glyph.index].is_some()
                    || glyph.is_emoji
                    || f32::from(glyph.position.x - metrics.advance * glyph.index as f32).abs()
                        > 0.02
                    || glyph.position.y != px(0.)
                {
                    return None;
                }
                glyphs[glyph.index] = Some((run.font_id, glyph.id));
            }
        }
        if glyphs.iter().any(Option::is_none)
            || f32::from(line.width() - metrics.advance * 95.).abs() > 0.02
        {
            return None;
        }
        Some(Self {
            glyphs: glyphs.map(Option::unwrap),
            baseline: (metrics.height - line.ascent - line.descent) / 2. + line.ascent,
        })
    }
    pub(super) fn paint(
        &self,
        text: &str,
        origin: gpui::Point<Pixels>,
        metrics: CellMetrics,
        color: Hsla,
        window: &mut Window,
    ) {
        for (col, byte) in text.bytes().enumerate() {
            if byte == b' ' {
                continue;
            }
            let (font, glyph) = self.glyphs[(byte - b' ') as usize];
            let _ = window.paint_glyph(
                point(
                    origin.x + metrics.width * col as f32,
                    origin.y + self.baseline,
                ),
                font,
                glyph,
                px(font_size()),
                color,
            );
        }
    }
}
