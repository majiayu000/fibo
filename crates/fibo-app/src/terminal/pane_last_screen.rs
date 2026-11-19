//! The screen a pane last showed, kept when the pane goes (its tab was switched away) so
//! the next pane on the same terminal draws it while the host's snapshot is on its way.
//! Without it a pane opens empty with 「正在恢复终端…」 until the snapshot lands, which
//! on a busy machine took seconds (11-19) and read as if the session had to be restored.
use super::Pane;
use crate::{terminal::screen::Screen, ui::theme::Theme as T};
use fibo_core::terminal::TermColors;
use std::{cell::RefCell, collections::HashMap, path::PathBuf};

/// Terminals whose last screen is kept; the one kept longest ago goes first.
const KEPT: usize = 64;

struct Kept {
    screen: Screen,
    /// The theme it was painted under: after a switch it would show the old colors.
    scheme: TermColors,
    order: u64,
}

#[derive(Default)]
struct Shelf {
    next: u64,
    kept: HashMap<(PathBuf, String), Kept>,
}

thread_local! {
    // Panes are made and dropped on the UI thread only.
    static SHELF: RefCell<Shelf> = RefCell::default();
}

fn put(key: (PathBuf, String), screen: Screen, scheme: TermColors) {
    SHELF.with_borrow_mut(|shelf| {
        shelf.next += 1;
        let order = shelf.next;
        shelf.kept.insert(
            key,
            Kept {
                screen,
                scheme,
                order,
            },
        );
        while shelf.kept.len() > KEPT {
            let Some(oldest) = (shelf.kept.iter())
                .min_by_key(|(_, k)| k.order)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            shelf.kept.remove(&oldest);
        }
    });
}

fn take(key: &(PathBuf, String), scheme: TermColors) -> Option<Screen> {
    SHELF.with_borrow_mut(|shelf| {
        let kept = shelf.kept.remove(key)?;
        (kept.scheme == scheme).then_some(kept.screen)
    })
}

fn forget(key: &(PathBuf, String)) {
    SHELF.with_borrow_mut(|shelf| {
        shelf.kept.remove(key);
    });
}

impl Pane {
    /// Starts from the screen the last pane on this terminal left, if any.
    pub(super) fn restore_last_screen(&mut self) {
        if let Some(mut screen) = take(&(self.socket.clone(), self.terminal.clone()), T::term()) {
            // A selection belonged to that pane.
            screen.selection.iter_mut().for_each(|s| *s = None);
            self.screen = screen;
            self.showing_kept = true;
        }
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let key = (self.socket.clone(), self.terminal.clone());
        // Scrolled into history, the pane shows old rows the next one would not open on.
        if self.scrolled_back || self.screen.rows.is_empty() {
            forget(&key);
            return;
        }
        put(key, std::mem::take(&mut self.screen), T::term());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::screen::Run;

    fn screen(text: &str) -> Screen {
        Screen {
            rows: vec![vec![Run {
                col: 0,
                cols: text.len() as u16,
                text: text.into(),
                fg: 0,
                bg: None,
                attrs: Default::default(),
            }]],
            selection: vec![Some((0, 3))],
            ..Screen::default()
        }
    }
    fn key(id: &str) -> (PathBuf, String) {
        (PathBuf::from("/sock"), id.into())
    }

    #[test]
    fn kept_screen_comes_back_once_and_only_under_the_same_theme() {
        let scheme = T::term();
        put(key("a"), screen("hello"), scheme);
        let back = take(&key("a"), scheme).expect("kept");
        assert_eq!(back.rows[0][0].text, "hello");
        assert!(take(&key("a"), scheme).is_none(), "taken once");

        let mut other = scheme;
        other.bg = other.bg.wrapping_add(1);
        put(key("b"), screen("x"), other);
        assert!(take(&key("b"), scheme).is_none(), "another theme's colors");
    }

    #[test]
    fn oldest_goes_past_the_limit() {
        let scheme = T::term();
        for i in 0..=KEPT {
            put(key(&format!("t{i}")), screen("x"), scheme);
        }
        assert!(take(&key("t0"), scheme).is_none());
        assert!(take(&key(&format!("t{KEPT}")), scheme).is_some());
    }
}
