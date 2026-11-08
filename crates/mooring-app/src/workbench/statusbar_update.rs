//! The status bar's update item: there only while a release is offered. A click installs it;
//! when this copy cannot install it (Homebrew, no permission, a failed check) the click opens
//! 设置 › 关于, which says why and what to do.
use super::statusbar::item;
use super::*;
use crate::update::{self, Phase};

impl Workbench {
    pub(super) fn update_item(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let u = update::updates(cx)?;
        let release = u.offered()?;
        let version = &release.version;
        let (text, tone, tip) = match &u.phase {
            Phase::Available(_) => (
                format!("新版本 {version} · 更新"),
                Tone::Ok,
                format!("当前 {}；更新后 App 自动重启，终端继续运行", u.current),
            ),
            Phase::Installing(_) => (
                format!("正在更新到 {version}…"),
                Tone::Ok,
                "下载、校验签名后替换 App，然后自动重启".into(),
            ),
            Phase::Homebrew(_) => (
                format!("新版本 {version}"),
                Tone::Ok,
                format!(
                    "这份由 Homebrew 安装：运行 {}",
                    mooring_core::update::BREW_COMMAND
                ),
            ),
            Phase::Failed { message, .. } => {
                (format!("{version} 没装上"), Tone::Warn, message.clone())
            }
            _ => return None,
        };
        let installs = matches!(u.phase, Phase::Available(_));
        let busy = matches!(u.phase, Phase::Installing(_));
        Some(
            item("status-update", false)
                .aria_label(text.clone())
                .gap(px(T::S4))
                .child(div().size(px(T::DOT)).rounded_full().bg(tone.dot()))
                .child(div().text_color(tone.ink()).child(text))
                .tooltip(super::diagnostics::tip(vec![tip]))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if installs {
                        update::install(cx);
                    } else if !busy {
                        this.show_settings_page(settings::Page::About, window, cx);
                    }
                }))
                .into_any_element(),
        )
    }
}

pub(super) fn divider() -> gpui::Div {
    div()
        .w(px(T::BORDER_WIDTH))
        .h(px(T::S14))
        .mx(px(T::S4))
        .bg(T::color(T::border()))
}
