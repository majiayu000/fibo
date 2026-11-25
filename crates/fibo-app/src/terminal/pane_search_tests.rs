//! Keys in the scrollback search field stay out of the terminal behind it.
use super::Pane;
use gpui::{Focusable, TestAppContext};

fn pane(cx: &mut TestAppContext) -> (gpui::Entity<Pane>, &mut gpui::VisualTestContext) {
    let socket = std::env::temp_dir().join(format!("fibo-search-{}.sock", std::process::id()));
    let (pane, cx) = cx.add_window_view(|_, cx| Pane::new(&socket, "t", false, cx));
    pane.update_in(cx, |p, window, cx| p.open_search(window, cx));
    cx.run_until_parked();
    (pane, cx)
}

#[gpui::test]
fn escape_in_the_search_field_closes_it_and_never_reaches_the_terminal(cx: &mut TestAppContext) {
    let (pane, cx) = pane(cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    pane.update_in(cx, |p, window, cx| {
        assert!(p.search.field.is_none(), "Esc closes the search");
        assert!(
            p.focus_handle(cx).is_focused(window),
            "and hands the keys back to the pane"
        );
        // A key the pane takes for the program fails here with no host behind it.
        assert_eq!(p.local_error, None, "Esc was sent on to the terminal");
    });
}

#[gpui::test]
fn keys_the_search_field_leaves_alone_never_reach_the_terminal(cx: &mut TestAppContext) {
    let (pane, cx) = pane(cx);
    for key in ["up", "down", "tab", "ctrl-c", "ctrl-d", "backspace"] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        pane.read_with(cx, |p, _| {
            assert!(p.search.field.is_some(), "{key} closed the search");
            // A key the pane takes for the program fails here with no host behind it.
            assert_eq!(p.local_error, None, "{key} was sent on to the terminal");
        });
    }
}

#[gpui::test]
fn enter_in_the_search_field_finds_rather_than_typing_a_new_line(cx: &mut TestAppContext) {
    let (pane, cx) = pane(cx);
    let field = pane.read_with(cx, |p, _| p.search.field.clone().unwrap());
    field.update(cx, |f, cx| {
        f.set_text("needle");
        cx.notify();
    });
    cx.simulate_keystrokes("enter");
    pane.read_with(cx, |p, cx| {
        assert_eq!(p.search.field.as_ref().unwrap().read(cx).text, "needle");
        assert_eq!(
            p.search.searched.as_deref(),
            Some("needle"),
            "Enter ran the search"
        );
    });
}
