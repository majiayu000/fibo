//! A plugin opened: what it is like beside the others of its kind, what it will touch, and
//! its settings. Text-only plugins get what their own files say rather than a picture.
use super::catalog::{Item, playing};
use super::gallery::{Tab, theme_sample};
use super::*;
use crate::workbench::complete::{Play, stage};
use mooring_core::plugin_category::Category;
use std::time::Instant;

const LIST_WIDTH: f32 = 190.;
const PREVIEW_HEIGHT: f32 = 218.;
const FACT_LABEL: f32 = 60.;

fn fact(name: &str, text: String) -> gpui::Div {
    div()
        .flex()
        .gap(px(T::S10))
        .child(
            div()
                .w(px(FACT_LABEL))
                .flex_shrink_0()
                .child(label(name.to_owned())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(T::SMALL))
                .text_color(T::color(T::primary()))
                .child(text),
        )
}
fn mock_line(text: &str, color: u32) -> gpui::Div {
    div()
        .text_size(px(T::SMALL))
        .text_color(T::color(color))
        .child(text.to_owned())
}

impl SettingsView {
    pub(super) fn open_item(&mut self, id: String, cx: &mut Context<Self>) {
        if self.market.open.is_none() {
            self.market.back_to = self.market.scroll.offset();
        }
        self.market.open = Some(id);
        self.market.opened = Some(Instant::now());
        self.market.hover = None;
        // What the last install did belongs to that plugin, not the next one opened.
        self.market.done = None;
        self.market.scroll.set_offset(gpui::Point::default());
        cx.notify();
    }
    pub(super) fn close_item(&mut self, cx: &mut Context<Self>) {
        self.market.open = None;
        self.market.confirm = None;
        self.market.done = None;
        self.market.scroll.set_offset(self.market.back_to);
        cx.notify();
    }
    /// The picture, or the closest honest thing: its own files, or a labelled sketch.
    fn item_preview(&self, item: &Item, cx: &Context<Self>) -> gpui::AnyElement {
        let mono = T::mono(cx);
        let show = self
            .market
            .showcase
            .get(&item.id)
            .cloned()
            .unwrap_or_default();
        let (ink, soft) = (T::primary(), T::secondary());
        let frame = |body: gpui::Div, note: &'static str| {
            div()
                .relative()
                .size_full()
                .p(px(T::S16))
                .font(mono.clone())
                .flex()
                .flex_col()
                .gap(px(T::S4))
                .child(body)
                .child(
                    div()
                        .absolute()
                        .right(px(T::S10))
                        .bottom(px(T::S6))
                        .child(label(note.to_owned())),
                )
                .into_any_element()
        };
        if let Some(complete) = item.complete {
            let play = match &self.market.hover {
                Some((id, from)) if *id == item.id => Play::Loop(*from),
                _ => self.market.opened.map_or(Play::Still, Play::Once),
            };
            return stage(complete, play).into_any_element();
        }
        if !show.themes.is_empty() {
            return div()
                .size_full()
                .flex()
                .children(show.themes.iter().map(theme_sample))
                .into_any_element();
        }
        let has = |k: Kind| item.provides.contains(&k);
        let lines = |rows: Vec<gpui::Div>| div().flex().flex_col().gap(px(T::S4)).children(rows);
        if !show.actions.is_empty() {
            let rows = show
                .actions
                .iter()
                .map(|t| mock_line(&format!("› {t}"), ink))
                .collect();
            return frame(lines(rows), "它的内容，右键菜单和命令面板里出现");
        }
        if !show.checks.is_empty() {
            let mut rows = vec![mock_line("这些命令跑过，算验证过：", soft)];
            rows.extend(
                show.checks
                    .iter()
                    .map(|c| mock_line(&format!("$ {c}"), ink)),
            );
            return frame(lines(rows), "它的内容");
        }
        if let Some((db, resume)) = &show.agent {
            let rows = vec![
                mock_line("从这里读会话：", soft),
                mock_line(db, ink),
                mock_line("接着某个会话：", soft),
                mock_line(resume, ink),
            ];
            return frame(lines(rows), "它的内容");
        }
        let (rows, note) = if has(Kind::Quota) {
            (
                vec![mock_line("状态栏　5 小时 42% · 每周 18%", ink)],
                "示意，百分比是编的",
            )
        } else if has(Kind::Hooks) {
            (
                vec![
                    mock_line("mooring · 修登录 bug", ink),
                    mock_line("error · cargo test 失败", soft),
                ],
                "示意，通知的文字是编的",
            )
        } else if has(Kind::Decorations) {
            (
                vec![
                    mock_line("~/mooring　　✗ 退出 1　改动 3", ink),
                    mock_line("悬停标题：上一条命令用时 12 秒，退出码 1", soft),
                ],
                "示意，数字是编的",
            )
        } else if has(Kind::Sidebar) {
            (
                vec![
                    mock_line("修登录 bug", ink),
                    mock_line(
                        &format!("{} · 要不要改接口？", Status::Waiting.label()),
                        soft,
                    ),
                    mock_line("发布脚本", ink),
                    mock_line("其余 12 个 ▸", soft),
                ],
                "示意，会话是编的",
            )
        } else {
            (
                vec![mock_line(item.description.as_deref().unwrap_or(""), soft)],
                "",
            )
        };
        frame(lines(rows), note)
    }
    pub(super) fn item_detail(&self, item: &Item, items: &[Item], cx: &Context<Self>) -> gpui::Div {
        let tab = self.market.tab;
        let kin = items.iter().filter(|i| {
            i.category == item.category
                && (i.id == item.id
                    || match tab {
                        Tab::Installed => i.installed(),
                        Tab::Discover => !i.builtin(),
                    })
        });
        let category: Category = item.category;
        let list = div()
            .w(px(LIST_WIDTH))
            .flex_shrink_0()
            .p(px(T::S6))
            .flex()
            .flex_col()
            .gap(px(T::S2))
            .rounded(px(T::CONTROL_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .bg(T::color(T::popover()))
            .child(
                div()
                    .px(px(T::S8))
                    .py(px(T::S4))
                    .child(label(category.label().to_owned())),
            )
            .children(kin.map(|i| {
                let id = i.id.clone();
                let state = if i.update {
                    "可更新"
                } else if i.pick.as_ref().is_some_and(|p| *p == playing()) {
                    "正在用"
                } else if i.builtin() {
                    "内置"
                } else if !i.installed() {
                    "未安装"
                } else if i.enabled() {
                    "已安装"
                } else {
                    "已停用"
                };
                div()
                    .id(gpui::SharedString::from(format!("plugin-kin-{}", i.id)))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(gpui::SharedString::from(i.name.clone()))
                    .min_h(px(T::ROW))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(T::S6))
                    .rounded(px(T::CONTROL_RADIUS - T::S2))
                    .cursor_pointer()
                    .when(i.id == item.id, |d| d.bg(T::color(T::chip())))
                    .hover(|s| s.bg(T::color(T::chip())))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.market.confirm = None;
                        this.open_item(id.clone(), cx);
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(T::SMALL))
                            .text_color(T::color(T::primary()))
                            .child(i.name.clone()),
                    )
                    .child(div().flex_shrink_0().child(label(state.to_owned())))
            }));
        let asked = item.offer.as_ref().and_then(|o| self.confirming(o, cx));
        let mut manage = div().flex().flex_wrap().items_center().gap(px(T::S8));
        if let Some(complete) = item.complete {
            manage = manage.child(
                secondary(format!("complete-try-{}", item.id), "在窗格上试播").on_click(
                    cx.listener(move |_, _, _, cx| {
                        cx.emit(SettingsAction::PreviewComplete(Some(complete)));
                    }),
                ),
            );
        }
        if let (Some(pick), Some(complete)) = (&item.pick, item.complete) {
            manage = manage.children(self.face_edit_button(pick, &item.name, complete, cx));
        }
        if let Some(p) = &item.plugin {
            let on = p.enabled;
            let id = p.id.clone();
            manage = manage.child(
                secondary(
                    format!("plugin-switch-{}", p.id),
                    if on { "停用" } else { "启用" },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.market.working.is_none() {
                        this.switch_plugin(id.clone(), !on, false, cx);
                    }
                })),
            );
            let removable = p
                .dir
                .parent()
                .is_some_and(|d| plugin::config().is_some_and(|c| d == plugin::dir(c)));
            if removable {
                let (id, name) = (p.id.clone(), p.name.clone());
                manage = manage.child(
                    secondary(format!("plugin-remove-{}", p.id), "卸载").on_click(cx.listener(
                        move |this, _, _, cx| {
                            if this.market.working.is_none() {
                                this.uninstall(id.clone(), name.clone(), false, cx);
                            }
                        },
                    )),
                );
            }
        }
        let mut facts = div()
            .pt(px(T::S10))
            .border_t(px(T::BORDER_WIDTH))
            .border_color(T::color(T::border()))
            .flex()
            .flex_col()
            .gap(px(T::S4));
        facts = facts.child(fact(
            "版本",
            match (&item.plugin, &item.offer) {
                (Some(p), Some(o)) if item.update => format!(
                    "{}，{}有 {}",
                    p.version.as_deref().unwrap_or("未标"),
                    o.origin(),
                    o.entry.version
                ),
                _ => item.version.clone().unwrap_or_else(|| "内置".to_owned()),
            },
        ));
        facts = facts.child(fact("作用于", kinds(&item.provides)));
        facts = facts.child(fact("安全", item.safety()));
        let mut extra = Vec::new();
        if let Some(p) = item.plugin.as_ref().filter(|p| p.enabled) {
            if let Some(plugins) = plugin::config().map(plugin::dir) {
                for (o, v) in mooring_core::plugin_context::values(&plugins, p) {
                    extra.push(self.option_row(&p.id, o, v, cx).into_any_element());
                }
            }
            if p.provides.contains(&Kind::Actions) {
                extra.extend(self.action_rows(p, cx));
            }
        }
        let pane = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(T::S12))
            .child(
                div()
                    .id("plugin-preview")
                    .h(px(PREVIEW_HEIGHT))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .rounded(px(T::CONTROL_RADIUS))
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
                    .bg(T::color(T::popover()))
                    .on_hover({
                        let id = item.id.clone();
                        cx.listener(move |this, hovered: &bool, _, cx| {
                            this.market.hover = hovered.then(|| (id.clone(), Instant::now()));
                            cx.notify();
                        })
                    })
                    .child(self.item_preview(item, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(T::BODY_SIZE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .child(item.name.clone()),
                    )
                    .child(self.item_control(item, true, cx)),
            )
            .when_some(asked, |d, (question, buttons)| {
                d.child(
                    div()
                        .p(px(T::S10))
                        .flex()
                        .flex_col()
                        .gap(px(T::S8))
                        .rounded(px(T::CONTROL_RADIUS))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(Tone::Warn.dot())
                        .child(label(question))
                        .child(buttons),
                )
            })
            .when_some(self.market.done.clone(), |d, done| {
                d.child(label(match done {
                    Ok(t) | Err(t) => t,
                }))
            })
            .when_some(item.description.clone(), |d, text| d.child(label(text)))
            .child(manage)
            .child(facts)
            .when(!extra.is_empty(), |d| d.child(card().children(extra)));
        div()
            .pt(px(T::S8))
            .flex()
            .flex_col()
            .gap(px(T::S10))
            .child(
                div().flex().child(
                    secondary(
                        "plugin-back",
                        match tab {
                            Tab::Installed => "← 已安装",
                            Tab::Discover => "← 发现",
                        },
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.close_item(cx))),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(T::S16))
                    .child(list)
                    .child(pane),
            )
    }
}
