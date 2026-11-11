//! 设置: a floating panel over the workbench, opened from the sidebar, the Mooring menu
//! or ⌘,. Grouped cards in the manner of macOS System Settings: the left column holds the
//! search and the pages, the right one the page's cards. Every change applies and saves
//! at once; the shortcuts page records a new chord in place.
use super::*;
use crate::keymap::{self, Action, Chord, Keymap, Section};
use crate::ui::{
    app_icon::{AppIcon, Chosen},
    terminal_text::{self, Editor, OptionKey, TerminalText, UnfocusedCursor},
    theme::mix,
};
use mooring_core::i18n;
use std::path::PathBuf;

mod about;
mod general;
mod hooks;
mod keys;
mod look;
mod notifications;
mod overlay;
mod panel;
mod plugins;
mod remote;
mod terminal_look;
mod terminal_page;
mod theme_editor;
mod theme_material;
mod updates;
mod widgets;
use widgets::{card, heading, keycap, secondary, segment, segments, switch};

/// The chord button keeps one width, recording or not.
const KEY_WIDTH: f32 = T::HERE_LABEL + T::S20;
/// Room kept for 恢复 on every row.
const RESET_WIDTH: f32 = T::S20 * 2. + T::S12;
const NAV_WIDTH: f32 = 212.;
const PANEL_HEIGHT: f32 = T::HERE_HEIGHT + T::CLEANUP_ACTION_HEIGHT;
const SEGMENT_HEIGHT: f32 = 26.;
const SWITCH_WIDTH: f32 = 36.;
const SWITCH_HEIGHT: f32 = 20.;
const ICON_PREVIEW: f32 = 44.;
/// Monospace faces offered when installed; empty is the bundled JetBrains Mono.
const MONO_FACES: [&str; 15] = [
    "",
    "SF Mono",
    "Menlo",
    "Monaco",
    "Fira Code",
    "Cascadia Code",
    "Source Code Pro",
    "IBM Plex Mono",
    "Hack",
    "Iosevka",
    "Roboto Mono",
    "Ubuntu Mono",
    "PT Mono",
    "Andale Mono",
    "Courier New",
];

pub(super) use plugins::tilde;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    General,
    Look,
    Terminal,
    Keys,
    Notifications,
    Phones,
    Plugins,
    About,
}
impl Page {
    const ALL: [Self; 8] = [
        Self::General,
        Self::Look,
        Self::Terminal,
        Self::Keys,
        Self::Notifications,
        Self::Phones,
        Self::Plugins,
        Self::About,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::General => t!("通用"),
            Self::Look => t!("外观"),
            Self::Terminal => t!("终端"),
            Self::Keys => t!("快捷键"),
            Self::Notifications => t!("通知"),
            Self::Phones => t!("手机与远程"),
            Self::Plugins => t!("插件"),
            Self::About => t!("关于"),
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Self::General => "≡",
            Self::Look => "◐",
            Self::Terminal => "›_",
            Self::Keys if mooring_platform::desktop::HAS_COMMAND_KEY => "⌘",
            Self::Keys => "⌨",
            Self::Notifications => "◉",
            Self::Phones => "▯",
            Self::Plugins => "⧉",
            Self::About => "i",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::General => t!("新会话用哪个 Agent，以及侧栏怎样整理旧会话。"),
            Self::Look => t!("明暗、主题与应用图标，选中即生效；任何主题都能调整。"),
            Self::Terminal => t!("终端文字、光标与分屏，上方实时预览。"),
            Self::Keys => t!("点击组合键重新录制，Esc 取消。"),
            Self::Notifications => t!("Agent 需要你的时候，用系统通知或提示音叫你回来。"),
            Self::Phones => {
                t!("配对过的手机经中继连过来：看画面、答审批、打字。内容端到端加密，中继看不到。")
            }
            Self::Plugins => {
                t!("装上、更新、卸载和开关插件，马上生效；也能看到每个声明文件生效了没有、为什么。")
            }
            Self::About => t!("版本信息，以及 Fibo 把数据和偏好存在哪里。"),
        }
    }
}
/// Every searchable setting: its page, its title and the words that find it.
const ENTRIES: [(Page, &str, &str); 41] = [
    (
        Page::General,
        "Agent Hook",
        tk!("hook hooks claude settings.json 过期 安装 重新安装 卸载 对上对话 状态不准"),
    ),
    (
        Page::General,
        tk!("没装 Hook 时提醒"),
        tk!("hook 提醒 状态栏 没装 过期"),
    ),
    (
        Page::General,
        tk!("语言"),
        tk!("语言 中文 英文 english language 跟随系统"),
    ),
    (
        Page::Phones,
        tk!("其他电脑"),
        tk!("多机 远程 ssh mac 机器 主机 另一台"),
    ),
    (
        Page::Phones,
        tk!("让手机连过来"),
        tk!("手机 远程 中继 relay 配对 iphone 安卓"),
    ),
    (
        Page::Phones,
        tk!("中继地址"),
        tk!("relay 中继 服务器 地址 密钥 key"),
    ),
    (
        Page::Phones,
        tk!("操作记录"),
        tk!("日志 记录 log 手机 按键"),
    ),
    (
        Page::General,
        tk!("默认 Agent"),
        tk!("新建待办 会话 codex claude grok"),
    ),
    (
        Page::General,
        tk!("回来时显示总览"),
        tk!("离开 返回 工作进展 分钟 overview"),
    ),
    (
        Page::General,
        tk!("旧会话收起时间"),
        tk!("已关闭 侧栏 收起 历史 天"),
    ),
    (
        Page::General,
        tk!("偷看浮层"),
        tk!("peek 浮层 预览 侧栏 点开 会话 审批"),
    ),
    (
        Page::Look,
        tk!("主题"),
        tk!(
            "跟随系统 亮色 暗色 明暗 dark light 曜黑 纸白 蓝灰 石墨 暖灰 松绿 紫灰 配色 导入 itermcolors 编辑器"
        ),
    ),
    (
        Page::Look,
        tk!("界面样式"),
        tk!("样式 标准 终端原生 tmux 等宽 style"),
    ),
    (
        Page::Look,
        tk!("应用图标"),
        tk!("dock 图标 二次元 吉祥物 icon"),
    ),
    (
        Page::Look,
        tk!("标记完成动画"),
        tk!(
            "动效 动画 完成 印章 落印 扫光 化进侧栏 淡出 笑脸 松口气 睡醒 小太阳 比心 小猫 热茶 蹦回家 试播 effects"
        ),
    ),
    (
        Page::Look,
        tk!("钉住这一轮的提问"),
        tk!("提问 置顶 钉住 顶部条 浮出 你问 回答太长 pin prompt sticky"),
    ),
    (
        Page::Look,
        tk!("状态圆圈转动"),
        tk!("转圈 动画 耗电 性能 spinner"),
    ),
    (
        Page::Terminal,
        tk!("终端字体"),
        tk!("字体 family font mono 等宽"),
    ),
    (
        Page::Terminal,
        tk!("字号"),
        tk!("文字大小 font size 缩放 pt"),
    ),
    (
        Page::Terminal,
        tk!("行距"),
        tk!("行高 line height 紧凑 舒适 宽松"),
    ),
    (
        Page::Terminal,
        tk!("光标闪烁"),
        tk!("光标 闪烁 blink cursor"),
    ),
    (
        Page::Terminal,
        tk!("非当前分屏变暗"),
        tk!("分屏 变暗 焦点 dim split"),
    ),
    (
        Page::Terminal,
        tk!("后台窗口变暗"),
        tk!("窗口 变暗 后台 dim inactive"),
    ),
    (
        Page::Terminal,
        tk!("默认光标形状"),
        tk!("光标 形状 方块 竖线 下划线 cursor shape"),
    ),
    (
        Page::Terminal,
        tk!("光标所在行"),
        tk!("光标 辅助线 高亮 当前行 cursor guide"),
    ),
    (
        Page::Terminal,
        tk!("中文和符号字体"),
        tk!("字体 中文 汉字 非 ASCII font cjk"),
    ),
    (
        Page::Terminal,
        tk!("连字"),
        tk!("连字 ligature 合字 calt liga"),
    ),
    (
        Page::Terminal,
        tk!("最低对比度"),
        tk!("对比度 可读 看不清 contrast"),
    ),
    (
        Page::Terminal,
        tk!("弱化文字"),
        tk!("弱化 faint dim 淡 透明度"),
    ),
    (
        Page::Terminal,
        tk!("终端边距"),
        tk!("边距 留白 内边距 padding margin"),
    ),
    (
        Page::Terminal,
        tk!("用哪个编辑器打开行号"),
        tk!("编辑器 editor zed vscode cursor 行号 路径 点击 打开 ⌘"),
    ),
    (
        Page::Terminal,
        tk!("Option 键"),
        tk!("option alt meta 键盘 德语 法语 波兰 @ 重音 字符 keyboard"),
    ),
    (
        Page::Terminal,
        tk!("非当前分屏的光标"),
        tk!("分屏 光标 空心 焦点 cursor split"),
    ),
    (
        Page::Notifications,
        tk!("桌面通知"),
        tk!("提醒 等待 错误 macOS 权限 notification"),
    ),
    (
        Page::Notifications,
        tk!("提示音"),
        tk!("提醒 声音 sound 不开通知也会响"),
    ),
    (
        Page::Plugins,
        tk!("插件"),
        tk!("plugin 扩展 市场 安装 卸载 更新 声明 agent json 动作 验证规则 左栏 sidebar 动画"),
    ),
    (Page::About, tk!("版本"), tk!("关于 fibo version")),
    (
        Page::About,
        tk!("检查更新"),
        tk!("更新 升级 新版本 update release 发布页"),
    ),
    (
        Page::About,
        tk!("自动检查更新"),
        tk!("更新 联网 网络 隐私 自动 update"),
    ),
    (
        Page::About,
        tk!("运行数据"),
        tk!("数据目录 数据库 runtime 后台 访达"),
    ),
    (Page::About, tk!("偏好文件"), tk!("配置 config 访达")),
];
pub(super) enum SettingsAction {
    Close,
    /// 试播: play a completion animation (None: the one picked) on the focused pane
    /// without marking anything.
    PreviewComplete(Option<mooring_core::effect::Complete>),
    Change(Settings),
    /// Plugins were installed, removed or switched and read again here: the workbench
    /// redraws what they feed and tells the host.
    PluginsChanged,
    /// The other machines list was saved: connect to them afresh.
    MachinesChanged,
    /// A CLI's hooks were installed or removed here: the window reads them again.
    HooksChanged,
    /// Phones on a relay: on or off, the relay, and its key (None keeps the saved one).
    Remote {
        enabled: bool,
        relay: String,
        key: Option<String>,
    },
}
pub(super) struct SettingsView {
    pub(super) focus: gpui::FocusHandle,
    pub(super) page: Page,
    pub(super) values: Settings,
    pub(super) busy: bool,
    pub(super) save_error: Option<String>,
    text_error: Option<String>,
    pub(super) query: Entity<Field>,
    /// The workspace's runtime directory, shown on 关于.
    runtime: Option<PathBuf>,
    /// The action waiting for its new chord.
    pub(super) recording: Option<Action>,
    agent_menu: Option<Agent>,
    /// A recorded chord another action already has: it moves only when confirmed.
    pub(super) taken: Option<(Action, Chord, Action)>,
    /// Why the last recorded chord was refused, shown under its row.
    pub(super) refused: Option<(Action, &'static str)>,
    /// The Agent CLIs' hook entries, read once when the panel opens; None while reading.
    hooks: Option<Result<Vec<crate::agent_hooks::Hook>, String>>,
    /// The CLI whose hooks are being installed or removed after a click.
    installing: Option<String>,
    /// Phones on a relay as the host last said; kept current while the panel is open.
    pub(super) remote: mooring_core::protocol::RemoteStatus,
    relay: Entity<Field>,
    relay_key: Entity<Field>,
    /// Why the phones setting was not sent.
    remote_error: Option<&'static str>,
    /// 其他电脑: `<config>/machines.json`, the new one's field and the last save's error.
    machines: Vec<crate::machines::Spec>,
    machine_field: Entity<Field>,
    machines_error: Option<String>,
    /// 插件: the catalog and what the last install did.
    market: plugins::Market,
    /// 主题编辑器, open over 外观.
    editor: Option<theme_editor::Editor>,
}
impl gpui::EventEmitter<SettingsAction> for SettingsView {}
impl SettingsView {
    pub(super) fn new(
        values: Settings,
        busy: bool,
        runtime: Option<PathBuf>,
        remote: mooring_core::protocol::RemoteStatus,
        cx: &mut Context<Self>,
    ) -> Self {
        let relay = cx.new(|cx| {
            Field::new(t!("中继地址"), &remote.relay, cx)
                .compact()
                .placeholder("https://relay.example.com")
        });
        let relay_key = cx.new(|cx| {
            Field::new(t!("中继密钥"), "", cx)
                .compact()
                .placeholder(t!("可不填"))
        });
        cx.observe(&relay, |_, _, cx| cx.notify()).detach();
        let machine_field = cx.new(|cx| {
            Field::new(t!("电脑"), "", cx)
                .compact()
                .placeholder(t!("ssh 主机名，例如 mac2"))
        });
        cx.observe(&machine_field, |_, _, cx| cx.notify()).detach();
        cx.observe(&relay_key, |_, _, cx| cx.notify()).detach();
        let query = cx.new(|cx| {
            Field::new(t!("搜索设置"), "", cx)
                .compact()
                .placeholder(t!("搜索设置"))
        });
        cx.observe(&query, |this, _, cx| {
            this.agent_menu = None;
            cx.notify();
        })
        .detach();
        cx.observe_global::<crate::update::Updates>(|_, cx| cx.notify())
            .detach();
        // Closing 设置 mid-edit puts the saved theme back.
        cx.on_release(|this, cx| {
            if this.editor.is_some() {
                crate::ui::appearance::preview(None, cx);
            }
        })
        .detach();
        cx.subscribe(&query, |_, _, event, cx| {
            if matches!(event, crate::ui::field::FieldEvent::Cancel) {
                cx.emit(SettingsAction::Close);
            }
        })
        .detach();
        Self::change_hooks(None, cx);
        let (machines, machines_error) = match crate::machines::load() {
            Ok(machines) => (machines, None),
            Err(e) => (Vec::new(), Some(t!("没读到电脑配置：{e}", e))),
        };
        Self {
            focus: cx.focus_handle(),
            page: Page::General,
            values,
            busy,
            save_error: None,
            text_error: None,
            query,
            runtime,
            recording: None,
            agent_menu: None,
            taken: None,
            refused: None,
            hooks: None,
            installing: None,
            remote,
            relay,
            relay_key,
            remote_error: None,
            machines,
            machine_field,
            machines_error,
            market: plugins::Market::read(cx),
            editor: None,
        }
    }
    /// Sends a workspace setting to the host; nothing while the last one is saving.
    fn change(&self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Settings)) {
        if self.busy {
            return;
        }
        let mut values = self.values.clone();
        edit(&mut values);
        cx.emit(SettingsAction::Change(values));
    }
    fn nav_item(&self, page: Page, searching: bool, cx: &Context<Self>) -> impl IntoElement {
        let on = self.page == page && !searching;
        let dim = searching && !self.page_matches(page, cx);
        div()
            .id(page.label())
            .role(gpui::accesskit::Role::Tab)
            .aria_label(page.label())
            .h(px(T::ROW + T::S2))
            .px(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S10))
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .text_size(px(T::BODY_SIZE))
            .text_color(T::color(if dim { T::muted() } else { T::body() }))
            .map(|d| {
                if on {
                    d.bg(T::color(T::raised()))
                        .text_color(T::color(T::primary()))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                } else {
                    d.hover(|s| s.bg(T::color(T::button_hover())))
                }
            })
            .child(
                div()
                    .size(px(T::ICON_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(T::CONTROL_RADIUS))
                    .text_size(px(T::SMALL))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .map(|d| {
                        if on {
                            d.bg(T::color(T::accent()))
                                .text_color(T::color(T::on_accent()))
                        } else {
                            d.bg(T::color(T::chip()))
                                .text_color(T::color(T::secondary()))
                        }
                    })
                    .child(page.glyph()),
            )
            .child(page.label())
            .on_click(cx.listener(move |this, _, _, cx| this.show_page(page, cx)))
    }
    /// Turns to `page` with the search cleared.
    fn show_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.recording = None;
        self.taken = None;
        self.refused = None;
        self.query.update(cx, |field, cx| {
            field.set_text("");
            cx.notify();
        });
        cx.notify();
    }
    fn query(&self, cx: &gpui::App) -> String {
        self.query.read(cx).text.trim().to_lowercase()
    }
    fn matches(&self, page: Page, title: &str, note: &str, cx: &gpui::App) -> bool {
        let query = self.query(cx);
        query.is_empty()
            || format!("{} {title} {note}", page.label())
                .to_lowercase()
                .contains(&query)
    }
    /// Whether the setting called `title` on `page` answers the search.
    fn shows(&self, page: Page, title: &str, cx: &gpui::App) -> bool {
        let words = ENTRIES
            .iter()
            .find(|(p, t, _)| *p == page && i18n::say(t) == title)
            .map_or("", |(_, _, words)| i18n::say(words));
        self.matches(page, title, words, cx)
    }
    fn page_matches(&self, page: Page, cx: &gpui::App) -> bool {
        match page {
            Page::Keys => {
                Keymap::actions().any(|a| self.matches(page, &a.label(), a.section().label(), cx))
                    || self.matches(page, t!("切换到第几个标签"), "⌘1 ⌘9", cx)
            }
            _ => ENTRIES
                .iter()
                .any(|(p, title, _)| *p == page && self.shows(page, i18n::say(title), cx)),
        }
    }
    /// A titled row inside a card; every row after the first gets a hairline above.
    fn row(&self, first: bool, title: &str, note: &str, control: impl IntoElement) -> gpui::Div {
        div()
            .min_h(px(T::TASK_ROW + T::S6))
            .px(px(T::S16))
            .py(px(T::S12))
            .flex()
            .items_center()
            .gap(px(T::S16))
            .when(!first, |d| {
                d.border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(T::S2))
                    .child(
                        div()
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .child(title.to_owned()),
                    )
                    .when(!note.is_empty(), |d| d.child(label(note.to_owned()))),
            )
            .child(div().flex_shrink_0().child(control))
    }
    /// Cards and their headings, keeping only rows the search leaves.
    fn cards(&self, groups: Vec<(&'static str, Vec<gpui::AnyElement>)>) -> gpui::Div {
        let mut page = div().flex().flex_col();
        for (title, rows) in groups {
            if rows.is_empty() {
                continue;
            }
            page = page.child(heading(title)).child(card().children(rows));
        }
        page
    }
    fn content(&self, page: Page, cx: &Context<Self>) -> gpui::Div {
        match page {
            Page::General => self.general_page(cx),
            Page::Look => match self.editor {
                Some(_) => self.editor_page(cx),
                None => self.look_page(cx),
            },
            Page::Terminal => self.terminal_page(cx),
            Page::Keys => self.keys_page(cx),
            Page::Notifications => self.notifications_page(cx),
            Page::Phones => self.phones_page(cx),
            Page::Plugins => self.plugins_page(cx),
            Page::About => self.about_page(cx),
        }
    }
    fn body(&self, cx: &Context<Self>) -> gpui::Div {
        if self.query(cx).is_empty() {
            return self.content(self.page, cx);
        }
        let mut body = div().flex().flex_col().gap(px(T::S12));
        let mut found = false;
        for page in Page::ALL {
            if !self.page_matches(page, cx) {
                continue;
            }
            found = true;
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .pt(px(T::S8))
                            .text_size(px(T::FORM_TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .child(page.label()),
                    )
                    .child(self.content(page, cx)),
            );
        }
        if !found {
            body = body.child(
                div()
                    .pt(px(T::S20 * 4.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .text_size(px(T::FORM_TITLE))
                            .text_color(T::color(T::primary()))
                            .child(t!("没有找到相关设置")),
                    )
                    .child(label(t!("试试「字体」「光标」「图标」或「快捷键」"))),
            );
        }
        body
    }
    /// Saving, saved, or why it failed: one line under the pages.
    fn status(&self) -> gpui::Div {
        let error = self.save_error.as_ref().or(self.text_error.as_ref());
        let (mark, text, color) = match error {
            Some(error) => ("!", error.clone(), Tone::Danger.ink()),
            None if self.busy => ("…", t!("正在保存").into(), T::color(T::secondary())),
            None => ("✓", t!("更改自动保存").into(), T::color(T::secondary())),
        };
        div()
            .flex()
            .items_start()
            .gap(px(T::S6))
            .text_size(px(T::META))
            .text_color(color)
            .child(div().flex_shrink_0().child(mark))
            // A long path in the error is cut; the first lines say what failed.
            .child(div().min_w_0().line_clamp(3).text_ellipsis().child(text))
    }
}
