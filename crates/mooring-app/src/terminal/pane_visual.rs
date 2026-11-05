//! Probes the visual-test harness reads a pane through.
use super::{Pane, SELECT_HINT};
use crate::terminal::screen;
use gpui::{Bounds, Context, EntityInputHandler, Pixels, Window};
use mooring_core::terminal::libghostty_vt::terminal::ScrollViewport;

impl Pane {
    pub fn visual_type(&mut self, text: &str, cx: &mut Context<Self>) {
        self.input(text.as_bytes().to_vec(), cx);
    }
    pub fn visual_cursor_shape(&self) -> screen::CursorShape {
        self.screen.cursor_shape
    }
    pub fn visual_run_fg(&self, text: &str) -> Option<u32> {
        let mut runs = self.screen.rows.iter().flatten();
        runs.find(|r| r.text.contains(text)).map(|r| r.fg)
    }
    /// The visible screen as text, for checking what reached the program.
    pub fn visual_text(&self) -> String {
        let state = self.connection.state.lock().unwrap();
        state
            .vt
            .as_ref()
            .and_then(|vt| {
                vt.tail_text(usize::from(self.requested_size.1.max(40)))
                    .ok()
            })
            .unwrap_or_default()
    }
    /// The rows the viewport shows now, scrolled back or not.
    pub fn visual_text_shown(&self) -> String {
        let rows: Vec<String> = self
            .screen
            .rows
            .iter()
            .map(|row| row.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        rows.join("\n")
    }
    pub fn visual_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds
    }
    pub fn visual_viewport(&self) -> (usize, usize, usize) {
        self.viewport
    }
    /// Where each ask was found, and what is pinned: the newest ask's text and whether
    /// it scrolls back, and the text of the ask whose answer is being read.
    pub fn visual_pinned_ask(
        &self,
    ) -> (Vec<Option<usize>>, Option<(String, bool)>, Option<String>) {
        (
            self.ask.ask_rows.clone(),
            self.pinned_latest(),
            self.ask
                .pinned
                .viewed
                .and_then(|(i, _)| self.ask.asks.get(i).cloned()),
        )
    }
    /// Scrolls history so `row` is the top row, as the scrollbar would.
    pub fn visual_scroll_row(&mut self, row: usize, cx: &mut Context<Self>) {
        self.scroll_to(ScrollViewport::Row(row), cx);
    }
    /// Opens search with `query` and runs it, as typing it and pressing Enter does.
    pub fn visual_search(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_search(window, cx);
        let field = self.search.field.clone().unwrap();
        field.update(cx, |f, cx| {
            f.text = query.into();
            cx.notify();
        });
        self.find_text(cx);
    }
    pub fn visual_matches(&self) -> (Option<usize>, Vec<usize>) {
        (
            self.search.current_match,
            self.search.matches.iter().map(|m| m.row).collect(),
        )
    }
    pub fn visual_step_match(&mut self, step: isize, cx: &mut Context<Self>) {
        self.step_match(step, cx);
    }
    /// The window point at the middle of cell (`row`, `col`).
    pub fn visual_cell(&self, row: u16, col: u16) -> Option<gpui::Point<Pixels>> {
        let (bounds, metrics) = (self.bounds?, self.metrics?);
        Some(gpui::point(
            bounds.left() + metrics.width * (col as f32 + 0.5),
            bounds.top() + metrics.height * (row as f32 + 0.5),
        ))
    }
    /// The selected columns (inclusive) on each viewport row, as drawn.
    pub fn visual_selection_cols(&self) -> Vec<Option<(u16, u16)>> {
        self.screen.selection.clone()
    }
    /// ⌘= / ⌘- / ⌘0 on the terminal text, as the shortcuts do.
    pub fn visual_zoom(&mut self, step: Option<i8>, cx: &mut Context<Self>) {
        screen::zoom(step).unwrap();
        cx.refresh_windows();
    }
    /// The viewport rows the selection is drawn on, and the text it would copy.
    pub fn visual_selection(&self) -> (Vec<u16>, Option<String>) {
        self.visual_selection_drawn().0
    }
    /// Also the text drawn on those rows.
    pub fn visual_selection_drawn(&self) -> ((Vec<u16>, Option<String>), Vec<String>) {
        let drawn = (self.screen.selection.iter().zip(&self.screen.rows))
            .filter(|(s, _)| s.is_some())
            .map(|(_, row)| {
                row.iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .collect();
        let rows = (self.screen.selection.iter().enumerate())
            .filter(|(_, s)| s.is_some())
            .map(|(y, _)| y as u16)
            .collect();
        let text = self.selected_text();
        ((rows, text), drawn)
    }
    pub fn visual_error(&self) -> Option<String> {
        self.local_error
            .clone()
            .or_else(|| self.connection.state.lock().ok()?.error.clone())
    }
    pub fn visual_select_hint(&mut self, cx: &mut Context<Self>) {
        self.show_toast(SELECT_HINT, cx);
    }
    /// Where the link whose target starts with `prefix` is drawn, and its cell count.
    pub fn visual_link(&self, prefix: &str) -> Option<(gpui::Point<Pixels>, u16)> {
        let (bounds, metrics) = (self.bounds?, self.metrics?);
        let link = self
            .screen
            .links
            .iter()
            .find(|l| l.uri.starts_with(prefix))?;
        Some((
            gpui::point(
                bounds.left() + metrics.width * (link.col as f32 + 0.5),
                bounds.top() + metrics.height * (link.row as f32 + 0.5),
            ),
            link.cols,
        ))
    }
    /// Whether the viewport shows the live screen rather than scrollback.
    pub fn visual_at_bottom(&self) -> bool {
        let state = self.connection.state.lock().unwrap();
        state
            .vt
            .as_ref()
            .is_some_and(|v| v.terminal().viewport_active().unwrap_or(false))
    }
    /// Where the path word `word` is drawn, and the file it points to once hovered.
    pub fn visual_path(
        &self,
        word: &str,
    ) -> Option<(gpui::Point<Pixels>, Option<std::path::PathBuf>)> {
        let (bounds, metrics) = (self.bounds?, self.metrics?);
        // The last piece, so a path broken over rows is hovered where it ends.
        let link = self.screen.paths.iter().rfind(|l| l.uri == word)?;
        let hovered = (self.links.hover_word.as_ref() == Some(link))
            .then(|| self.shown_path().cloned())
            .flatten();
        Some((
            gpui::point(
                bounds.left() + metrics.width * (link.col as f32 + 0.5),
                bounds.top() + metrics.height * (link.row as f32 + 0.5),
            ),
            hovered,
        ))
    }
    /// The note shown beside the pointer for a click on what it is over.
    pub fn visual_hint(&self) -> Option<String> {
        self.click_hint()
    }
    /// Whether the program in this pane asked for mouse reports, and where to click it.
    pub fn visual_mouse_app(&self) -> (bool, Option<gpui::Point<Pixels>>) {
        let state = self.connection.state.lock().unwrap();
        let tracking = state
            .vt
            .as_ref()
            .is_some_and(|v| v.terminal().is_mouse_tracking().unwrap_or(false));
        (tracking, self.bounds.map(|b| b.center()))
    }
    pub fn verify_marked_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Bounds<Pixels> {
        let rows = self.screen.rows.clone();
        self.replace_and_mark_text_in_range(None, "nihao", Some(2..2), window, cx);
        let bounds = self.bounds.expect("painted terminal");
        let expected = self
            .metrics
            .unwrap()
            .cursor_bounds(bounds, self.cursor_anchor.get().unwrap());
        let range = self.bounds_for_range(0..5, bounds, window, cx).unwrap();
        assert_eq!(
            range.origin, expected.origin,
            "IME must anchor at the terminal cursor"
        );
        assert_eq!(range.size.height, expected.size.height);
        let caret = self.bounds_for_range(2..2, bounds, window, cx).unwrap();
        assert!(caret.left() > range.left() && caret.left() < range.right());
        assert_eq!(
            caret.bottom(),
            range.bottom(),
            "candidate anchor under the composed text"
        );
        assert_eq!(
            self.screen.rows, rows,
            "marked text must not enter terminal cells"
        );
        eprintln!("Pane IME handler: overlay={range:?}, requested caret={caret:?}");
        range
    }
    fn shown_path(&self) -> Option<&std::path::PathBuf> {
        self.shown_target().map(|t| &t.path)
    }
}
