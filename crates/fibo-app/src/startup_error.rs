//! The window shown when Fibo cannot open its workbench: why, and what to do next.
//! A wall of text left the user only one move, quitting; each way out is a button now.
use crate::{lifecycle, ui::theme::Theme as T, ui::widgets::button};
use gpui::{App, ClipboardItem, Context, FocusHandle, Window, WindowOptions, prelude::*, px};
use std::path::{Path, PathBuf};

pub struct StartupError {
    message: String,
    root: PathBuf,
    focus: FocusHandle,
    /// The first click on 重启后台 only asks: a host that cannot hand over ends every terminal.
    confirming: bool,
    /// What the last button could not do.
    failed: Option<String>,
}

pub fn show(message: String, root: &Path, cx: &mut App) {
    eprintln!("Fibo: {message}");
    let root = root.to_path_buf();
    let bounds = gpui::Bounds::centered(None, gpui::size(px(640.), px(360.)), cx);
    let options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
        ..Default::default()
    };
    if let Err(e) = cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            StartupError {
                message,
                root,
                focus,
                confirming: false,
                failed: None,
            }
        })
    }) {
        eprintln!("error window: {e}");
    }
    cx.activate(true);
}

/// Opens this app again once this process is gone, then quits: the new process starts
/// from scratch, the host check included.
fn relaunch(cx: &mut App) -> std::io::Result<()> {
    fibo_platform::process::start_after_exit(
        std::process::id(),
        &std::env::current_exe()?,
        std::env::args_os().skip(1),
    )?;
    cx.quit();
    Ok(())
}

impl Render for StartupError {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let log = self.root.join("host.log");
        let replaceable = lifecycle::host_replaceable();
        let restart_label = if self.confirming {
            t!("确认重启后台：正在运行的终端会结束")
        } else {
            t!("重启后台")
        };
        let mut actions = gpui::div().flex().flex_wrap().gap(px(T::S8)).child(
            button("startup-retry", t!("重试")).on_click(cx.listener(|this, _, _, cx| {
                if let Err(e) = relaunch(cx) {
                    this.failed = Some(t!("没能重新打开：{e}", e));
                    cx.notify();
                }
            })),
        );
        if replaceable {
            actions = actions.child(button("startup-restart-host", restart_label).on_click(
                cx.listener(|this, _, _, cx| {
                    if !this.confirming {
                        this.confirming = true;
                        cx.notify();
                        return;
                    }
                    match lifecycle::restart_host(&this.root) {
                        Ok(()) => cx.quit(),
                        Err(e) => {
                            this.confirming = false;
                            this.failed = Some(t!("没能重启后台：{e}", e));
                            cx.notify();
                        }
                    }
                }),
            ));
        }
        let shown_log = log.clone();
        actions = actions
            .child(
                button(
                    "startup-log",
                    crate::ui::words::local(t!("在访达中显示日志")),
                )
                .on_click(move |_, _, cx| cx.reveal_path(&shown_log)),
            )
            .child(button("startup-copy", t!("复制错误")).on_click(cx.listener(
                |this, _, _, cx| {
                    let log = this.root.join("host.log");
                    cx.write_to_clipboard(ClipboardItem::new_string(format!(
                        "{}\n\n{}",
                        this.message,
                        log.display()
                    )));
                },
            )))
            .child(button("startup-quit", t!("退出")).on_click(|_, _, cx| cx.quit()));
        gpui::div()
            .id("startup-error")
            .size_full()
            .p(px(T::S16))
            .flex()
            .flex_col()
            .gap(px(T::S12))
            .overflow_y_scroll()
            .bg(T::surface(T::app()))
            .text_color(T::color(T::primary()))
            .text_size(px(T::BODY_SIZE))
            .track_focus(&self.focus)
            .child(gpui::div().child(t!("Fibo 没能打开工作台")))
            .child(gpui::div().child(self.message.clone()))
            .child(
                gpui::div()
                    .text_size(px(T::META))
                    .text_color(T::color(T::secondary()))
                    .child(t!("后台日志：{path}", path = log.display())),
            )
            .children(self.failed.clone().map(|f| {
                gpui::div()
                    .text_color(crate::ui::tone::Tone::Danger.ink())
                    .child(f)
            }))
            .child(actions)
    }
}
