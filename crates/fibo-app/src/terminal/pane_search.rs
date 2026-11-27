//! Searching the scrollback.
use super::Pane;
use fibo_core::terminal::libghostty_vt::terminal::ScrollViewport;
use gpui::{App, Context, Focusable, Window, prelude::*};

impl Pane {
    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // One line: Enter and Esc are the field's to report, so neither reaches the
        // program in the terminal behind it.
        let field = cx.new(|cx| crate::ui::field::Field::new(t!("搜索滚动内容"), "", cx).compact());
        window.focus(&field.focus_handle(cx), cx);
        // Enter finds, then steps to the next match (Shift-Enter the previous); the
        // panel's buttons do the same. Esc closes the search.
        let events = cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
            crate::ui::field::FieldEvent::Submit => {
                if this.search.matches.is_empty() || this.search.searched != this.search_query(cx) {
                    this.find_text(cx);
                } else {
                    this.step_match(if window.modifiers().shift { -1 } else { 1 }, cx);
                }
            }
            crate::ui::field::FieldEvent::Cancel => this.close_search(window, cx),
            crate::ui::field::FieldEvent::ClickOut => {}
        });
        self.search.field = Some(field);
        self.search._events = Some(events);
        self.search.matches.clear();
        self.search.current_match = None;
        self.search.searched = None;
        cx.notify();
    }
    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.field = None;
        self.search._events = None;
        self.search.matches.clear();
        self.search.current_match = None;
        self.search.searched = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }
    /// Whether typing goes to the search field rather than the terminal.
    pub(super) fn search_has_keys(&self, window: &Window, cx: &App) -> bool {
        self.search
            .field
            .as_ref()
            .is_some_and(|f| f.focus_handle(cx).is_focused(window))
    }
    fn search_query(&self, cx: &App) -> Option<String> {
        self.search
            .field
            .as_ref()
            .map(|f| f.read(cx).text.trim().to_owned())
    }
    /// Moves to the next (1) or previous (-1) match, wrapping around, and centers it.
    pub(super) fn step_match(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.search.matches.is_empty() {
            return;
        }
        let n = self.search.matches.len() as isize;
        let next = match self.search.current_match {
            Some(i) => (i as isize + step).rem_euclid(n),
            // The first step from nothing lands on the nearest match above the bottom.
            None if step < 0 => n - 1,
            None => 0,
        } as usize;
        self.show_match(next, cx);
    }
    pub(super) fn show_match(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(m) = self.search.matches.get(i) else {
            return;
        };
        let row = m.row.saturating_sub(self.viewport.2 / 2);
        if let Ok(mut s) = self.connection.state.lock()
            && let Some(vt) = &mut s.vt
        {
            vt.scroll(ScrollViewport::Row(row));
            if let Ok(v) = vt.viewport() {
                self.viewport = v;
            }
        }
        if let (Some(view), Some(metrics)) = (self.bounds, self.metrics) {
            let x = metrics.width * f32::from(m.col);
            let y = metrics.height * m.row.saturating_sub(self.viewport.0) as f32;
            self.pan.reveal(view.size, gpui::point(x, y));
        }
        self.search.current_match = Some(i);
        cx.notify();
    }
    pub(super) fn find_text(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.search_query(cx) else {
            return;
        };
        self.search.searched = Some(query.clone());
        let state = self.connection.state.clone();
        let job = cx.background_spawn(async move {
            state
                .lock()
                .map_err(|_| "terminal lock poisoned".to_owned())?
                .vt
                .as_ref()
                .ok_or_else(|| t!("终端尚未恢复").to_owned())?
                .search(&query)
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(found) => {
                        this.search.matches = found;
                        this.search.current_match = None;
                        // Newest first, as the bottom of the screen is closest.
                        this.step_match(-1, cx);
                    }
                    Err(e) => this.local_error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
