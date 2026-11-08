//! 设置 › 关于 › 更新: the daily check's switch, and checking or installing by hand.
use super::*;
use crate::update::{self, Phase};

impl SettingsView {
    pub(super) fn update_rows(&self, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let Some(u) = update::updates(cx) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        if self.shows(Page::About, "检查更新", cx) {
            rows.push(self.update_now_row(u, cx));
        }
        if u.bundle.is_some() && self.shows(Page::About, "自动检查更新", cx) {
            let on = u.auto;
            let toggle = switch("update-auto", on).on_click(cx.listener(move |this, _, _, cx| {
                this.text_error = update::set_auto(!on, cx).err();
                cx.notify();
            }));
            rows.push(
                self.row(
                    rows.is_empty(),
                    "自动检查更新",
                    "每天最多一次，只向 GitHub 问最新的版本号；这是 Mooring 唯一会自己发出的网络请求",
                    toggle,
                )
                .into_any_element(),
            );
        }
        rows
    }

    fn update_now_row(&self, u: &update::Updates, cx: &Context<Self>) -> gpui::AnyElement {
        let current = &u.current;
        // A development build never installs anything, so nothing here would work.
        if u.bundle.is_none() {
            return self
                .row(
                    true,
                    "检查更新",
                    &format!("{current} · 从源码运行的版本不自动更新，用 git pull 后重新构建"),
                    div(),
                )
                .into_any_element();
        }
        let (note, action): (String, Option<(&str, bool)>) = match &u.phase {
            Phase::Idle => (format!("当前 {current}"), Some(("检查更新", false))),
            Phase::Checking => ("正在检查…".into(), None),
            Phase::Latest(_) => (format!("{current} 已是最新版本"), Some(("检查更新", false))),
            Phase::CheckFailed(e) => (format!("检查失败：{e}"), Some(("检查更新", false))),
            Phase::Available(r) => (
                format!("有新版本 {}（当前 {current}），终端会继续运行", r.version),
                Some(("立即更新", true)),
            ),
            Phase::Installing(r) => (format!("正在下载并校验 {}…", r.version), None),
            Phase::Failed { message, page, .. } => (
                message.clone(),
                Some(if *page {
                    ("打开发布页", true)
                } else {
                    ("重试", true)
                }),
            ),
            Phase::Homebrew(r) => (
                format!(
                    "有新版本 {}；这份由 Homebrew 安装，请运行 {}",
                    r.version,
                    mooring_core::update::BREW_COMMAND
                ),
                Some(("拷贝命令", false)),
            ),
        };
        let control = action.map(|(text, install)| {
            let homebrew = matches!(u.phase, Phase::Homebrew(_));
            secondary("update-now", text).on_click(cx.listener(move |_, _, _, cx| {
                if homebrew {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        mooring_core::update::BREW_COMMAND.into(),
                    ));
                } else if install {
                    update::install(cx);
                } else {
                    update::check(true, cx);
                }
            }))
        });
        self.row(true, "检查更新", &note, div().children(control))
            .into_any_element()
    }
}
