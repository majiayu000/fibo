//! What is off about how the host or this window was started, and the way out of it.
use super::*;
use fibo_core::launch::LaunchIssue;

impl Workbench {
    /// A host this window fixes by starting it again itself gets a button, which hands
    /// every terminal over. A window started the wrong way can only be opened again: the
    /// host it starts would inherit the same.
    pub(super) fn launch_chip(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        if let Some((_, fixed)) = crate::lifecycle::misplaced_host() {
            let text = match (self.restarting, fixed.first()) {
                (true, _) => t!("正在重启后台…"),
                (false, Some(LaunchIssue::RemoteSession)) => {
                    t!("后台是从 SSH 启动的，之后启动的 Claude 读不到登录 · 重启后台")
                }
                (false, Some(LaunchIssue::LowPriority)) => t!("后台在以低优先级运行 · 重启后台"),
                (false, _) => t!("后台不是这个窗口启动的 · 重启后台"),
            };
            return Some(
                button("restart-misplaced-host", text)
                    .text_color(T::color(T::secondary()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.restarting {
                            this.request_close(Closing::Host, cx);
                        }
                    }))
                    .into_any_element(),
            );
        }
        let window = crate::lifecycle::window_launch();
        let text = if window.contains(&LaunchIssue::RemoteSession) {
            t!("窗口是从 SSH 启动的，之后启动的 Claude 读不到登录：请从程序坞重新打开 Fibo")
        } else if window.contains(&LaunchIssue::LowPriority) {
            t!("窗口在以低优先级运行：请从程序坞重新打开 Fibo")
        } else {
            return None;
        };
        Some(label(text).into_any_element())
    }
}
