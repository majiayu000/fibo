use super::*;
use fibo_core::content::{Background, Content, ShellTask};
use gpui::{Modifiers, Render, TestAppContext};

struct Row {
    local: bool,
    opened: usize,
}

impl Render for Row {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut workspace = crate::workbench::tests::fixture();
        let session = &mut workspace.agent_sessions[0];
        session.current = true;
        session.terminal = Some("t0".into());
        let session = &workspace.agent_sessions[0];
        workspace.content.insert(
            session.id.clone(),
            std::sync::Arc::new(Content {
                background: Some(Background {
                    shells: 2,
                    ..Default::default()
                }),
                shells: [18746, 8080]
                    .map(|port| ShellTask {
                        port: Some(port),
                        ..Default::default()
                    })
                    .to_vec(),
                ..Default::default()
            }),
        );
        let detail = fibo_core::status::left_behind_detail(&workspace, session).unwrap();
        div()
            .id("session-row")
            .debug_selector(|| "session-row".into())
            .w_full()
            .child(RowDetail::Background(detail).render(div(), self.local))
            .on_click(cx.listener(|this, _, _, _| this.opened += 1))
    }
}

#[gpui::test]
fn local_port_links_require_alt_and_keep_plain_row_clicks(cx: &mut TestAppContext) {
    let (row, cx) = cx.add_window_view(|_, _| Row {
        local: true,
        opened: 0,
    });
    cx.refresh().unwrap();
    let point = cx.debug_bounds("local-port-18746").unwrap().center();
    cx.simulate_click(point, Modifiers::none());
    assert!(cx.opened_url().is_none());
    assert_eq!(row.read_with(cx, |row, _| row.opened), 1);

    let alt = Modifiers {
        alt: true,
        ..Modifiers::none()
    };
    for (selector, url) in [
        ("local-port-8080", "http://127.0.0.1:8080/"),
        ("local-port-18746", "http://127.0.0.1:18746/"),
    ] {
        let point = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(point, alt);
        assert_eq!(cx.opened_url().as_deref(), Some(url));
        assert_eq!(row.read_with(cx, |row, _| row.opened), 1);
    }

    let row_bounds = cx.debug_bounds("session-row").unwrap();
    let port_bounds = cx.debug_bounds("local-port-8080").unwrap();
    let point = gpui::point(
        (row_bounds.left() + port_bounds.left()) / 2.,
        port_bounds.center().y,
    );
    cx.simulate_click(point, alt);
    assert_eq!(row.read_with(cx, |row, _| row.opened), 2);
}

#[gpui::test]
fn another_machines_ports_do_not_open_a_local_page(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Row {
        local: false,
        opened: 0,
    });
    cx.refresh().unwrap();
    assert!(cx.debug_bounds("local-port-8080").is_none());
    assert!(cx.debug_bounds("local-port-18746").is_none());
}
