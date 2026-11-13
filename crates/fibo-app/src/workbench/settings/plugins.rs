//! 设置 › 插件: what is installed (switch, update, uninstall, and per-item choices such as
//! each right-click action), and what can be installed: the catalog shipped with the app
//! and, when asked, a remote index. Changes apply at once (`plugin::reload` here, then
//! `SettingsAction::PluginsChanged` for the workbench and the host).
use super::*;
use fibo_core::effect::Complete;
use fibo_core::market::{self, Offer};
use fibo_core::plugin::{self, Kind};

mod actions;
mod catalog;
mod detail;
mod face_editor;
mod gallery;
mod player;
mod showcase;
#[cfg(feature = "visual-test")]
mod visual;
use catalog::Item;
use gallery::Tab;

/// The catalog as the page last read it.
#[derive(Default)]
pub(in crate::workbench) struct Market {
    bundled: Vec<Offer>,
    /// Why the bundled catalog could not be read.
    bundled_error: Option<String>,
    /// The remote index once fetched: its offers or why it failed.
    remote: Option<Result<Vec<Offer>, String>>,
    fetching: bool,
    /// A package fetched and checked, waiting for「确认安装」: what it would add to what
    /// runs on this computer comes from the package itself.
    confirm: Option<market::Prepared>,
    /// The plugin being installed, updated or removed.
    working: Option<String>,
    /// What the last install, update or removal did.
    done: Option<Result<String, String>>,
    /// A plugin that would not go because its teardown failed: (id, name, whether removing
    /// or only turning off). You may overrule that; it may have left something behind.
    stuck: Option<(String, String, bool)>,
    /// 自己画一个, while open.
    face: Option<face_editor::FaceEditor>,
    /// Which of the page's two tabs is showing, and which kind 发现 is narrowed to.
    tab: Tab,
    category: Option<fibo_core::plugin_category::Category>,
    /// The plugin opened in the detail view, when it opened, and where the list was.
    open: Option<String>,
    opened: Option<std::time::Instant>,
    back_to: gpui::Point<gpui::Pixels>,
    /// The card or stage being pointed at, and since when: it is the one that plays.
    hover: Option<(String, std::time::Instant)>,
    /// What is playing over the page, once settings has made it.
    player: Option<Entity<crate::workbench::complete::Player>>,
    spot: crate::workbench::complete::StageSpot,
    /// 声明文件明细, unfolded.
    files_open: bool,
    /// 发现's search, and the page's scroll so opening a plugin starts at its top.
    search: Option<Entity<crate::ui::field::Field>>,
    pub(super) scroll: gpui::ScrollHandle,
    /// What each plugin's own files say, read once with the catalog.
    showcase: std::collections::HashMap<String, showcase::Showcase>,
    /// A plugin setting being typed: plugin id, key, the field and its Enter/Esc events.
    editing: Option<(
        String,
        String,
        Entity<crate::ui::field::Field>,
        gpui::Subscription,
    )>,
}
impl Market {
    pub(super) fn loaded(&self) -> bool {
        self.player.is_some()
    }
    pub(in crate::workbench) fn read(cx: &mut Context<SettingsView>) -> Self {
        let search = cx.new(|cx| {
            crate::ui::field::Field::new(t!("搜索插件"), "", cx)
                .compact()
                .placeholder(t!("搜索插件"))
        });
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let spot = crate::workbench::complete::StageSpot::default();
        let player = cx.new(|_| crate::workbench::complete::Player {
            now: None,
            spot: spot.clone(),
        });
        let mut market = Self {
            search: Some(search),
            player: Some(player),
            spot,
            ..Self::default()
        };
        market.read_catalog();
        market
    }
    /// The catalog from disk again; what you were looking at stays.
    fn read_catalog(&mut self) {
        (self.bundled, self.bundled_error) = match market::bundled() {
            Ok(b) => (b, None),
            Err(e) => (vec![], Some(e)),
        };
        self.showcase = showcase::read_all(&self.bundled);
    }
    /// The plugin whose stage plays.
    fn play_id(&self) -> Option<&str> {
        self.hover
            .as_ref()
            .map(|(id, _)| id.as_str())
            .or(self.open.as_deref())
    }
    fn search_field(&self, _: &Context<SettingsView>) -> Option<Entity<crate::ui::field::Field>> {
        self.search.clone()
    }
    fn search_text(&self, cx: &Context<SettingsView>) -> String {
        self.search
            .as_ref()
            .map_or_else(String::new, |f| f.read(cx).text.trim().to_owned())
    }
    fn query(&self, cx: &Context<SettingsView>) -> String {
        self.search_text(cx).to_lowercase()
    }
    /// Every offer: bundled ones, replaced by newer remote ones, then remote-only ones.
    fn offers(&self) -> Vec<Offer> {
        let remote = self
            .remote
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .cloned()
            .unwrap_or_default();
        market::merge(self.bundled.clone(), remote)
    }
}

/// The drawing's card, as tall as the animation cards beside it.
const FACE_TILE: f32 = 190.;
/// An animation's picture in 已安装.
const THUMB_W: f32 = 116.;
const THUMB_H: f32 = 64.;

/// A path under the home folder written from `~`.
pub(in crate::workbench) fn tilde(path: &str) -> String {
    let home = fibo_platform::dirs::home_str().unwrap_or_default();
    match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => path.to_owned(),
    }
}

fn kinds(provides: &[Kind]) -> String {
    provides
        .iter()
        .map(|k| k.label())
        .collect::<Vec<_>>()
        .join(t!("、"))
}

/// An animation plugin's name inside 完成动画, without the 「完成动画：」 its name starts with.
fn short_name(name: &str) -> String {
    name.strip_prefix(not_ui!("完成动画："))
        .unwrap_or(name)
        .to_owned()
}

impl SettingsView {
    /// Reads the plugins again and tells the workbench and the host.
    pub(super) fn plugins_changed(&mut self, cx: &mut Context<Self>) {
        plugin::reload();
        self.market.showcase = showcase::read_all(&self.market.offers());
        crate::ui::themes::reload(cx);
        cx.emit(SettingsAction::PluginsChanged);
        cx.notify();
    }
    fn fetch_remote(&mut self, cx: &mut Context<Self>) {
        self.market.fetching = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let url = market::remote_url();
            let result = cx
                .background_spawn(async move { market::fetch_remote(&url) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.market.fetching = false;
                this.market.remote = Some(result);
                cx.notify();
            });
        })
        .detach();
    }
    /// One plugin setting under its plugin.
    fn option_row(
        &self,
        id: &str,
        o: fibo_core::plugin_context::Opt,
        v: serde_json::Value,
        cx: &Context<Self>,
    ) -> gpui::Div {
        use fibo_core::plugin_context::{Opt, OptKind};
        let note = if o.secret {
            let said = o.description.as_deref().unwrap_or_default();
            t!("{said}（存在钥匙串里，不写进文件）", said)
        } else {
            o.description.clone().unwrap_or_default()
        };
        let title = t!("　└ {title}", title = o.title);
        let (pid, key) = (id.to_owned(), o.key.clone());
        let save = move |this: &mut Self, value: serde_json::Value, cx: &mut Context<Self>| {
            this.text_error = plugin::config()
                .map(plugin::dir)
                .ok_or_else(|| t!("找不到配置目录").to_owned())
                .and_then(|d| fibo_core::plugin_context::set(&d, &pid, &key, value))
                .err();
            cx.notify();
        };
        let key_id = format!("plugin-option-{id}-{}", o.key);
        let editing = self
            .market
            .editing
            .as_ref()
            .filter(|(i, k, ..)| i == id && *k == o.key)
            .map(|(_, _, f, _)| f.clone());
        let control = match o.kind {
            OptKind::Boolean => {
                let on = v.as_bool().unwrap_or(false);
                div().child(switch(key_id, on).on_click(
                    cx.listener(move |this, _, _, cx| save(this, serde_json::Value::Bool(!on), cx)),
                ))
            }
            OptKind::Choice => {
                let now = v.as_str().unwrap_or_default().to_owned();
                let next = o
                    .choices
                    .iter()
                    .position(|c| *c == now)
                    .map_or(0, |i| (i + 1) % o.choices.len());
                let next = serde_json::Value::String(o.choices[next].clone());
                div().child(
                    secondary(key_id, format!("{now} ▸"))
                        .on_click(cx.listener(move |this, _, _, cx| save(this, next.clone(), cx))),
                )
            }
            OptKind::String | OptKind::Number => match editing {
                Some(field) => div().w(px(T::OPTION_FIELD)).child(field),
                None => {
                    let shown = Opt::text(&v);
                    let (pid, opt) = (id.to_owned(), o.clone());
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .child(
                            div()
                                .text_size(px(T::SMALL))
                                .text_color(T::color(T::secondary()))
                                .child(if shown.is_empty() {
                                    t!("（空）").to_owned()
                                } else if o.secret {
                                    fibo_core::i18n::say(&shown).to_owned()
                                } else {
                                    shown.clone()
                                }),
                        )
                        .child(secondary(key_id, t!("改")).on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.edit_option(&pid, &opt, &shown, window, cx)
                            },
                        )))
                }
            },
        };
        self.row(false, &title, &note, control)
    }
    /// Opens a field for a text or number setting; Enter saves it, Esc leaves it.
    fn edit_option(
        &mut self,
        id: &str,
        opt: &fibo_core::plugin_context::Opt,
        current: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A secret is never shown back: you can only type a new one.
        let (start, masked) = if opt.secret {
            ("", true)
        } else {
            (current, false)
        };
        let field = cx.new(|cx| {
            let field = crate::ui::field::Field::new(&opt.title, start, cx).compact();
            if masked { field.masked() } else { field }
        });
        window.focus(&field.focus_handle(cx), cx);
        let key = opt.key.clone();
        let opt = opt.clone();
        let pid = id.to_owned();
        let events = cx.subscribe(&field, move |this, field, event, cx| {
            if let crate::ui::field::FieldEvent::Submit = event {
                let text = field.read(cx).text.clone();
                this.text_error = opt
                    .parse(&text)
                    .and_then(|v| {
                        plugin::config()
                            .map(plugin::dir)
                            .ok_or_else(|| t!("找不到配置目录").to_owned())
                            .and_then(|d| fibo_core::plugin_context::set(&d, &pid, &opt.key, v))
                    })
                    .err();
            }
            this.market.editing = None;
            cx.notify();
        });
        self.market.editing = Some((id.to_owned(), key, field, events));
        cx.notify();
    }
    /// One plugin's right-click actions, each with its own switch.
    fn action_rows(
        &self,
        p: &fibo_core::plugin::Plugin,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        fibo_core::action::all()
            .iter()
            .filter(|a| a.file.parent() == Some(p.dir.as_path()))
            .map(|a| {
                let on = a.enabled();
                let aid = a.id.clone();
                self.row(
                    false,
                    &t!("　└ {title}", title = a.title),
                    &a.command,
                    switch(format!("action-switch-{}", a.id), on).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.text_error = fibo_core::action::set_enabled(&aid, !on).err();
                            cx.notify();
                        },
                    )),
                )
                .into_any_element()
            })
            .collect()
    }
    /// 远程目录, only ever fetched when asked.
    fn remote_row(&self, cx: &Context<Self>) -> gpui::Div {
        let url = market::remote_url();
        let note = match &self.market.remote {
            _ if self.market.fetching => t!("正在获取…").to_owned(),
            None => t!("{url}；只在点了之后才联网", url),
            Some(Ok(r)) => t!(
                "{url}；有 {len} 个插件，更新的和新的已列在上面",
                url,
                len = r.len()
            ),
            Some(Err(e)) => t!("获取失败：{e}", e),
        };
        let note = match &self.market.bundled_error {
            Some(e) => format!("{e} · {note}"),
            None => note,
        };
        div().pt(px(T::S20)).child(
            card().child(
                self.row(
                    true,
                    t!("远程目录"),
                    &note,
                    secondary("plugins-fetch", t!("检查远程目录"))
                        .when(self.market.fetching, |b| b.opacity(0.5))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.market.fetching {
                                this.fetch_remote(cx);
                            }
                        })),
                ),
            ),
        )
    }
    /// 自己画一个, or the drawing waiting from a 试播, as a card among the animations.
    fn face_tile(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("complete-draw")
            .role(gpui::accesskit::Role::Button)
            .aria_label(if face_editor::has_draft() {
                t!("接着画")
            } else {
                t!("自己画一个")
            })
            .min_h(px(FACE_TILE))
            .p(px(T::S10))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(T::S4))
            .cursor_pointer()
            .rounded(px(T::CONTROL_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::control()))
            .hover(|s| s.bg(T::color(T::chip())))
            .on_click(cx.listener(|this, _, _, cx| this.face_open(None, cx)))
            .child(
                div()
                    .text_size(px(T::BODY_SIZE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(T::color(T::primary()))
                    .child(if face_editor::has_draft() {
                        t!("接着画")
                    } else {
                        t!("- _ -　自己画一个")
                    }),
            )
            .child(label(if face_editor::has_draft() {
                t!("上次试播的那张还留着").to_owned()
            } else {
                t!("一帧一帧改眼睛和嘴，存成插件").to_owned()
            }))
    }
    /// One installed plugin in 已安装: its picture or mark, what it is, its next step.
    fn installed_row(&self, first: bool, item: &Item, cx: &Context<Self>) -> gpui::AnyElement {
        let mut note = Vec::new();
        if let Some(v) = &item.version {
            note.push(format!("v{v}"));
        }
        if item.builtin() {
            note.clear();
        }
        if let Some(d) = &item.description {
            note.push(d.clone());
        }
        if let Some(p) = &item.plugin {
            note.extend(p.warnings.iter().cloned());
            let runs = plugin::config()
                .map(|c| fibo_core::plugin_context::runs(&plugin::dir(c), &p.id))
                .unwrap_or_default();
            let now = fibo_core::content::now();
            for r in fibo_core::plugin_context::failing(&runs)
                .into_iter()
                .filter(|r| now.saturating_sub(r.at) < 7 * 86_400)
            {
                note.push(t!(
                    "上次运行失败（{what}）：{detail}；终端里 fibo plugins log {id} 看全部",
                    what = r.what,
                    detail = r.detail,
                    id = p.id
                ));
            }
        }
        let id = item.id.clone();
        let opened = id.clone();
        let mut controls = div().flex().items_center().gap(px(T::S8));
        if item.complete.is_some() || item.update {
            controls = controls.child(self.item_control(item, false, cx));
        }
        if let Some(p) = item.plugin.as_ref().filter(|_| item.complete.is_none()) {
            let on = p.enabled;
            controls = controls.child(switch(format!("plugin-switch-{}", p.id), on).on_click(
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if this.market.working.is_none() {
                        this.switch_plugin(id.clone(), !on, false, cx);
                    }
                }),
            ));
        }
        let hover_id = item.id.clone();
        div()
            .id(gpui::SharedString::from(format!("plugin-row-{}", item.id)))
            .role(gpui::accesskit::Role::Button)
            .aria_label(gpui::SharedString::from(item.name.clone()))
            .px(px(T::S12))
            .py(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S12))
            .cursor_pointer()
            .hover(|s| s.bg(T::color(T::chip())))
            .when(!first, |d| {
                d.border_t(px(T::BORDER_WIDTH))
                    .border_color(T::color(T::border()))
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let was = this
                    .market
                    .hover
                    .as_ref()
                    .is_some_and(|(i, _)| *i == hover_id);
                if *hovered && !was {
                    this.market.hover = Some((hover_id.clone(), std::time::Instant::now()));
                    this.sync_player(cx);
                } else if !*hovered && was {
                    this.market.hover = None;
                    this.sync_player(cx);
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.open_item(opened.clone(), cx)))
            .map(|d| match item.complete {
                Some(complete) => d.child(
                    div()
                        .w(px(THUMB_W))
                        .h(px(THUMB_H))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .rounded(px(T::CONTROL_RADIUS - T::S2))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::border()))
                        .child(self.item_stage(item, complete)),
                ),
                None => d.child(
                    div()
                        .w(px(T::S20))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .child(label(gallery::mark(item.category).to_owned())),
                ),
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
                            .child(item.name.clone()),
                    )
                    .child(label(note.join(" · "))),
            )
            .child(controls)
            .into_any_element()
    }
    fn installed_tab(&self, items: &[Item], cx: &Context<Self>) -> gpui::Div {
        use fibo_core::agent;
        use fibo_core::plugin_category::Category;
        let status = |ok: bool, text: String| {
            div()
                .flex_shrink_0()
                .text_size(px(T::SMALL))
                .text_color(if ok {
                    T::color(T::secondary())
                } else {
                    Tone::Danger.ink()
                })
                .child(text)
        };
        let busy = self.market.working.is_some();
        let mut page = div().flex().flex_col();
        // What just happened and what is waiting on you, above everything.
        let mut notices = Vec::new();
        if let Some(done) = &self.market.done {
            let (ok, text) = match done {
                Ok(t) => (true, t.clone()),
                Err(e) => (false, e.clone()),
            };
            notices.push(
                div()
                    .px(px(T::S16))
                    .py(px(T::S10))
                    .text_size(px(T::SMALL))
                    .text_color(if ok {
                        T::color(T::primary())
                    } else {
                        Tone::Danger.ink()
                    })
                    .child(text)
                    .into_any_element(),
            );
        }
        // The teardown failed and it is still running: leave the decision to whoever reads why.
        if let Some((id, name, removing)) = self.market.stuck.clone() {
            let verb = if removing {
                t!("仍然卸载")
            } else {
                t!("仍然停用")
            };
            notices.push(
                self.row(
                    notices.is_empty(),
                    verb,
                    t!("它撤回失败了，继续的话，它改过的东西可能会留下来"),
                    secondary(format!("plugin-force-{id}"), verb).on_click(cx.listener(
                        move |this, _, _, cx| {
                            if this.market.working.is_some() {
                                return;
                            }
                            if removing {
                                this.uninstall(id.clone(), name.clone(), true, cx);
                            } else {
                                this.switch_plugin(id.clone(), false, true, cx);
                            }
                        },
                    )),
                )
                .into_any_element(),
            );
        }
        let updates: Vec<&str> = items
            .iter()
            .filter(|i| i.update)
            .map(|i| i.name.as_str())
            .collect();
        if !updates.is_empty() {
            notices.push(
                self.row(
                    notices.is_empty(),
                    &t!("{len} 个插件有新版本", len = updates.len()),
                    &updates.join(t!("、")),
                    div(),
                )
                .into_any_element(),
            );
        }
        if !notices.is_empty() {
            page = page.child(div().pt(px(T::S12)).child(card().children(notices)));
        }
        // By what each is for, the animation playing first in its own group.
        for c in Category::ALL {
            let mut rows: Vec<&Item> = items
                .iter()
                .filter(|i| i.category == c && i.installed())
                .collect();
            let now = catalog::playing();
            rows.sort_by_key(|i| i.pick.as_ref() != Some(&now));
            if rows.is_empty() {
                continue;
            }
            let mut group = card();
            for (n, item) in rows.iter().enumerate() {
                group = group.child(self.installed_row(n == 0, item, cx));
            }
            page = page.child(heading(c.label())).child(group);
        }
        // What did not load cannot be turned off or updated, but can always be taken away.
        let broken: Vec<gpui::AnyElement> = plugin::report()
            .iter()
            .filter(|f| f.result.is_err())
            .enumerate()
            .map(|(n, f)| {
                let name = f
                    .dir
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let (dir_name, shown) = (name.clone(), name.clone());
                let remove = secondary(format!("plugin-remove-broken-{name}"), t!("移除"))
                    .when(busy, |b| b.opacity(0.5))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.market.working.is_none() {
                            this.uninstall(dir_name.clone(), shown.clone(), false, cx);
                        }
                    }));
                self.row(
                    n == 0,
                    &name,
                    f.result.as_ref().err().map_or("", String::as_str),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .child(status(false, t!("没有加载").into()))
                        .child(remove),
                )
                .into_any_element()
            })
            .collect();
        if !broken.is_empty() {
            page = page
                .child(heading(t!("没有加载")))
                .child(card().children(broken));
        }
        page = page.child(div().pt(px(T::S20)).child(card().child(self.row(
            true,
            t!("重新读取"),
            t!("手动改了插件目录，或在终端里用 fibo plugins install 装了插件后，点这里马上生效"),
            secondary("plugins-reload", t!("重新读取")).on_click(cx.listener(|this, _, _, cx| {
                this.market.read_catalog();
                this.market.confirm = None;
                this.market.working = None;
                this.market.done = None;
                this.market.stuck = None;
                this.market.face = None;
                this.market.editing = None;
                this.plugins_changed(cx);
            })),
        ))));
        // ── 声明文件明细 ──
        let opened = self.market.files_open;
        page = page.child(
            div().pt(px(T::S12)).flex().child(
                secondary(
                    "plugin-files",
                    if opened {
                        t!("▾ 声明文件明细")
                    } else {
                        t!("▸ 声明文件明细")
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.market.files_open = !opened;
                    cx.notify();
                })),
            ),
        );
        if opened {
            let files = |rows: Vec<(String, Result<String, String>)>| {
                rows.into_iter()
                    .enumerate()
                    .map(|(i, (file, result))| {
                        match result {
                            Ok(did) => self.row(
                                i == 0,
                                &tilde(&file),
                                &did,
                                status(true, t!("生效").into()),
                            ),
                            Err(e) => self.row(
                                i == 0,
                                &tilde(&file),
                                &e,
                                status(false, t!("没有生效").into()),
                            ),
                        }
                        .into_any_element()
                    })
                    .collect::<Vec<_>>()
            };
            let report = |r: &[agent::Loaded], did: fn(String) -> String| {
                r.iter()
                    .map(|l| (l.file.display().to_string(), l.result.clone().map(did)))
                    .collect::<Vec<_>>()
            };
            let mut detail = Vec::new();
            detail.extend(report(fibo_core::action::report(), |t| {
                crate::ui::words::local(&t!("加进右键菜单和 ⌘P：{t}", t))
            }));
            detail.extend(report(fibo_core::rule::report(), |t| {
                t!("这些命令算跑过验证：{t}", t)
            }));
            detail.extend(report(fibo_core::hook::report(), |t| {
                t!(
                    "会话状态变化时由后台调用：{t}；终端里 fibo plugins log 看运行记录",
                    t
                )
            }));
            detail.extend(report(fibo_core::sidebar::report(), |t| {
                t!("左栏顶部多了：{t}", t)
            }));
            detail.extend(report(fibo_core::decoration::report(), |t| {
                t!("窗格标题栏或悬停卡片里显示：{t}", t)
            }));
            detail.extend(report(fibo_core::effect::report(), |t| t));
            detail.extend(report(fibo_core::quota::report(), |t| {
                t!("状态栏额度多了：{t}", t)
            }));
            detail.extend(report(agent::report(), |t| t!("注册为 {t}", t)));
            let rows = files(detail);
            if !rows.is_empty() {
                page = page.child(div().pt(px(T::S8)).child(card().children(rows)));
            }
        }
        let where_ = agent::config_dir()
            .map_or_else(|| "~/.config/fibo".to_owned(), |d| d.display().to_string());
        page.child(div().pt(px(T::S12)).px(px(T::S4)).child(label(t!(
            "插件放在 {dir}/plugins/<id>/；写插件的说明：在终端运行 fibo --skill plugins。",
            dir = tilde(&where_)
        ))))
    }
    /// 已安装 and 发现, or one plugin opened from either, or the frame editor.
    pub(super) fn plugins_page(&self, cx: &Context<Self>) -> gpui::Div {
        if !self.shows(Page::Plugins, t!("插件"), cx) {
            return div();
        }
        if self.market.face.is_some() {
            return div().pt(px(T::S8)).child(card().child(self.face_card(cx)));
        }
        let items = catalog::items(&self.market.offers());
        if let Some(item) = self
            .market
            .open
            .as_ref()
            .and_then(|id| items.iter().find(|i| i.id == *id))
        {
            return self.item_detail(item, &items, cx);
        }
        let installed = items.iter().filter(|i| i.plugin.is_some()).count();
        let total = items.iter().filter(|i| !i.builtin()).count();
        let top = self.plugin_tabs(installed, total, cx);
        div()
            .flex()
            .flex_col()
            .child(top)
            .child(match self.market.tab {
                Tab::Discover => self.discover_tab(&items, cx),
                Tab::Installed => self.installed_tab(&items, cx),
            })
    }
}
