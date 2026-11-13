//! The pane's state, grouped by what uses it, so each part of the pane touches only
//! its own group.
use crate::terminal::screen;
use gpui::{Pixels, Point};
use std::time::Instant;

/// Selecting text by dragging, and the scrollbar drag that shares the mouse.
pub(super) struct Selection {
    /// The selection gesture; the selection itself lives on the terminal, so it stays
    /// on its text as output or the scrollbar moves it.
    pub(super) selecting: Option<crate::terminal::selection::Selecting>,
    pub(super) pressed_button: Option<gpui::MouseButton>,
    /// The scrollbar thumb is being dragged.
    pub(super) dragging_scrollbar: bool,
}
impl Selection {
    pub(super) fn new() -> Self {
        Self {
            selecting: crate::terminal::selection::Selecting::new().ok(),
            pressed_button: None,
            dragging_scrollbar: false,
        }
    }
}
/// The asks pinned along the top and floated over history.
#[derive(Default)]
pub(super) struct AskPin {
    /// What this terminal's conversation was asked, oldest first, as the workbench last
    /// saw it, and where each sits in history once found.
    pub(super) asks: Vec<String>,
    pub(super) ask_rows: Vec<Option<usize>>,
    /// When the asks were last looked for, and the history's length then.
    pub(super) asks_located: Option<(Instant, usize)>,
    pub(super) locating_asks: bool,
    pub(super) pinned: fibo_core::prompt_pin::Pinned,
    /// When the viewport last moved through history, for the floating ask.
    pub(super) scrolled_at: Option<Instant>,
}
/// The search box and its matches.
#[derive(Default)]
pub(super) struct Search {
    pub(super) field: Option<gpui::Entity<crate::ui::field::Field>>,
    pub(super) matches: Vec<fibo_core::terminal::SearchMatch>,
    /// The query `matches` came from; Enter on an edited query searches again.
    pub(super) searched: Option<String>,
    pub(super) _events: Option<gpui::Subscription>,
    /// The match the view moved to; ↑/↓ and Enter step through them.
    pub(super) current_match: Option<usize>,
}
/// Links and path words under the pointer.
#[derive(Default)]
pub(super) struct Links {
    pub(super) hover_link: Option<(u16, u16)>,
    /// The path word under the pointer, and where it points when that exists on disk.
    pub(super) hover_word: Option<screen::Link>,
    pub(super) hover_path: Option<super::Hovered>,
    /// The modifiers last seen, so links are underlined only while ⌘ is held, and where
    /// the pointer is, for the note saying what a click would do.
    pub(super) modifiers: gpui::Modifiers,
    pub(super) pointer: Option<Point<Pixels>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Shell,
    Agent,
    Placed,
    NotReady,
    /// The shell could not find the Agent command; nothing was placed.
    Missing,
}
pub(super) struct Handoff {
    /// The CLI being started; `None` for one started by hand, which any input box fits.
    pub(super) agent: Option<fibo_core::agent::Agent>,
    pub(super) command: Option<String>,
    pub(super) text: Option<String>,
    pub(super) stage: Stage,
    /// Send it once placed; otherwise it waits for the user's Enter.
    pub(super) submit: bool,
    pub(super) started: Instant,
    pub(super) released: bool,
    pub(super) tail: String,
    pub(super) still: u8,
}
