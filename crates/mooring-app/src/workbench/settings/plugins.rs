//! 设置 › 插件: what is installed (switch, update, uninstall, and per-item choices such as
//! each right-click action), and what can be installed: the catalog shipped with the app
//! and, when asked, a remote index. Changes apply at once (`plugin::reload` here, then
//! `SettingsAction::PluginsChanged` for the workbench and the host); only Agent plugins
//! wait for the host's next start.
use super::*;
use mooring_core::market::{self, Offer};
use mooring_core::plugin::{self, Kind};

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
    /// A plugin setting being typed: plugin id, key, the field and its Enter/Esc events.
    editing: Option<(
        String,
        String,
        Entity<crate::ui::field::Field>,
        gpui::Subscription,
    )>,
}
impl Market {
    pub(in crate::workbench) fn read() -> Self {
        let (bundled, bundled_error) = match market::bundled() {
            Ok(b) => (b, None),
            Err(e) => (vec![], Some(e)),
        };
        Self {
            bundled,
            bundled_error,
            ..Self::default()
        }
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

fn kinds(provides: &[Kind]) -> String {
    provides
        .iter()
        .map(|k| k.label())
        .collect::<Vec<_>>()
        .join("、")
}

/// An animation plugin's name inside 完成动画, without the 「完成动画：」 its name starts with.
fn short_name(name: &str) -> String {
    name.strip_prefix("完成动画：").unwrap_or(name).to_owned()
}

/// Where a kind's choices live once installed.
fn where_to_find(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Effects => Some("装上或打开就换成它，在上面「完成动画」里能换回"),
        Kind::Sidebar => Some("在左栏顶部切换排法"),
        Kind::Actions => Some("在窗格右键菜单和 ⌘P 里"),
        Kind::Decorations => Some("显示在窗格标题栏和标题悬停卡片里"),
        Kind::Agent => Some("Agent 类要重开 Mooring 才生效"),
        Kind::Quota => Some("显示在状态栏的额度里"),
        Kind::Hooks | Kind::Rules | Kind::Setup => None,
    }
}

impl SettingsView {
    /// Reads the plugins again and tells the workbench and the host.
    pub(super) fn plugins_changed(&mut self, cx: &mut Context<Self>) {
        plugin::reload();
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
    /// Every way to install or update starts here: fetch it, read what it does, and ask
    /// when that runs commands (or an update adds to what runs). Nothing changes until then.
    fn install_offer(&mut self, offer: Offer, cx: &mut Context<Self>) {
        let id = offer.entry.id.clone();
        self.market.confirm = None;
        self.market.working = Some(id);
        self.market.done = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let name = offer.entry.name.clone();
            let version = offer.entry.version.clone();
            let prepared = cx
                .background_spawn(async move {
                    let config = plugin::config().ok_or("找不到配置目录".to_owned())?;
                    offer.prepare(&plugin::dir(config))
                })
                .await;
            let _ = this.update(cx, |this, cx| match prepared {
                Ok(p) if p.must_ask => {
                    this.market.working = None;
                    this.market.confirm = Some(p);
                    cx.notify();
                }
                Ok(p) => this.commit_install(p, name, version, cx),
                Err(e) => {
                    this.market.working = None;
                    this.market.done = Some(Err(format!("没装上「{name}」：{e}")));
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// Swaps a checked package in, off the UI thread: its setup may take seconds.
    fn commit_install(
        &mut self,
        prepared: market::Prepared,
        name: String,
        version: String,
        cx: &mut Context<Self>,
    ) {
        self.market.confirm = None;
        self.market.working = Some(prepared.id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { prepared.commit() }).await;
            let _ = this.update(cx, |this, cx| {
                this.market.working = None;
                this.market.done = Some(
                    result
                        .map(|_| format!("装好了「{name}」v{version}，已经生效"))
                        .map_err(|e| format!("没装上「{name}」：{e}")),
                );
                this.plugins_changed(cx);
            });
        })
        .detach();
    }
    /// The question for a package waiting on this offer, if one is: what to say and the
    /// two buttons, for whichever row started it.
    fn confirming(&self, offer: &Offer, cx: &Context<Self>) -> Option<(String, gpui::Div)> {
        let waiting = self.market.confirm.as_ref()?;
        if waiting.id != offer.entry.id {
            return None;
        }
        let e = &offer.entry;
        let verb = if waiting.added.is_empty() {
            "装上后它会"
        } else {
            "这次更新"
        };
        let note = format!("{verb}：{}。确认吗？", waiting.lines().join("；"));
        let (name, version) = (e.name.clone(), e.version.clone());
        let controls = div()
            .flex()
            .items_center()
            .gap(px(T::S8))
            .child(
                secondary(format!("plugin-cancel-{}", e.id), "取消").on_click(cx.listener(
                    |this, _, _, cx| {
                        // Dropping it discards the staged copy.
                        this.market.confirm = None;
                        cx.notify();
                    },
                )),
            )
            .child(
                secondary(format!("plugin-confirm-{}", e.id), "确认安装").on_click(cx.listener(
                    move |this, _, _, cx| {
                        if this.market.working.is_none()
                            && let Some(p) = this.market.confirm.take()
                        {
                            this.commit_install(p, name.clone(), version.clone(), cx);
                        }
                    },
                )),
            );
        Some((note, controls))
    }
    /// One plugin setting under its plugin.
    fn option_row(
        &self,
        id: &str,
        o: mooring_core::plugin_context::Opt,
        v: serde_json::Value,
        cx: &Context<Self>,
    ) -> gpui::Div {
        use mooring_core::plugin_context::{Opt, OptKind};
        let note = o.description.clone().unwrap_or_default();
        let title = format!("　└ {}", o.title);
        let (pid, key) = (id.to_owned(), o.key.clone());
        let save = move |this: &mut Self, value: serde_json::Value, cx: &mut Context<Self>| {
            this.text_error = plugin::config()
                .map(plugin::dir)
                .ok_or_else(|| "找不到配置目录".to_owned())
                .and_then(|d| mooring_core::plugin_context::set(&d, &pid, &key, value))
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
                                    "（空）".to_owned()
                                } else {
                                    shown.clone()
                                }),
                        )
                        .child(secondary(key_id, "改").on_click(cx.listener(
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
        opt: &mooring_core::plugin_context::Opt,
        current: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = cx.new(|cx| crate::ui::field::Field::new(&opt.title, current, cx).compact());
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
                            .ok_or_else(|| "找不到配置目录".to_owned())
                            .and_then(|d| mooring_core::plugin_context::set(&d, &pid, &opt.key, v))
                    })
                    .err();
            }
            this.market.editing = None;
            cx.notify();
        });
        self.market.editing = Some((id.to_owned(), key, field, events));
        cx.notify();
    }
    /// Off the UI thread: the plugin's `teardown` runs first and may take seconds.
    fn uninstall(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        self.market.working = Some(id.clone());
        self.market.done = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { plugin::uninstall(&id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.market.working = None;
                this.market.done = Some(
                    result
                        .map(|()| format!("卸载了「{name}」"))
                        .map_err(|e| format!("没卸掉「{name}」：{e}")),
                );
                this.plugins_changed(cx);
            });
        })
        .detach();
    }
    /// Off the UI thread, like `uninstall`: turning one on or off runs its setup or teardown.
    fn switch_plugin(&mut self, id: String, on: bool, cx: &mut Context<Self>) {
        self.market.working = Some(id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { plugin::set_enabled(&id, on) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.market.working = None;
                if let Err(e) = result {
                    this.text_error = Some(format!("没能切换插件：{e}"));
                }
                this.plugins_changed(cx);
            });
        })
        .detach();
    }

    /// 完成动画: every animation 标记完成 can play, one row each: the one playing marked
    /// 正在用, the others 用这个, each with 试播; animation plugins not installed yet
    /// offer 安装, which also makes them the one playing.
    fn animations(
        &self,
        offers: &[Offer],
        busy: bool,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        use mooring_core::effect::{self, Complete, Pick, Style};
        let picked = effect::picked();
        let presets = effect::presets();
        // A picked plugin since turned off or removed plays the seal.
        let playing = match &picked {
            Some(Pick::Plugin(id)) if !presets.iter().any(|p| &p.id == id) => {
                Pick::Builtin(Style::Seal)
            }
            Some(p) => p.clone(),
            None => Pick::Builtin(Style::Seal),
        };
        let mut choices: Vec<(String, String, Pick, Complete)> = presets
            .iter()
            .map(|p| {
                let description = plugin::installed(&p.id)
                    .and_then(|i| i.description.clone())
                    .unwrap_or_else(|| format!("{}，插件 {}", p.complete.style.label(), p.id));
                (
                    short_name(&p.name),
                    description,
                    Pick::Plugin(p.id.clone()),
                    p.complete,
                )
            })
            .collect();
        choices.extend(Style::BUILTIN.into_iter().map(|s| {
            (
                s.label().to_owned(),
                match s {
                    Style::Fade => "内置；系统开了「减弱动态效果」时都播它".to_owned(),
                    _ => "内置；按钮填满，印章盖在窗格上，窗格淡出".to_owned(),
                },
                Pick::Builtin(s),
                Complete::of(s),
            )
        }));
        let mut rows = Vec::new();
        for (title, note, choice, complete) in choices {
            let on = choice == playing;
            let key = choice.name().replace(':', "-");
            let mut controls = div().flex().items_center().gap(px(T::S8));
            if on {
                controls = controls.child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::primary()))
                        .child("● 正在用"),
                );
            }
            controls = controls.child(secondary(format!("complete-try-{key}"), "试播").on_click(
                cx.listener(move |_, _, _, cx| {
                    cx.emit(SettingsAction::PreviewComplete(Some(complete)))
                }),
            ));
            if !on {
                controls =
                    controls.child(secondary(format!("complete-use-{key}"), "用这个").on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.text_error = effect::pick(Some(choice.clone())).err();
                            cx.notify();
                        }),
                    ));
            }
            rows.push(
                self.row(rows.is_empty(), &title, &note, controls)
                    .into_any_element(),
            );
        }
        // Turned off in 已安装: turning it back on makes it the one playing.
        for p in plugin::report()
            .iter()
            .filter_map(|f| f.result.as_ref().ok())
            .filter(|p| !p.enabled && p.provides.contains(&Kind::Effects))
        {
            let id = p.id.clone();
            let title = short_name(&p.name);
            rows.push(
                self.row(
                    rows.is_empty(),
                    &title,
                    "已停用 · 打开插件就换成它",
                    secondary(format!("complete-enable-{}", p.id), "打开并用这个").on_click(
                        cx.listener(move |this, _, _, cx| {
                            if this.market.working.is_none() {
                                this.switch_plugin(id.clone(), true, cx);
                            }
                        }),
                    ),
                )
                .into_any_element(),
            );
        }
        let have = |id: &str| plugin::installed(id).is_some();
        for o in offers
            .iter()
            .filter(|o| o.entry.provides.contains(&Kind::Effects) && !have(&o.entry.id))
        {
            let e = &o.entry;
            let working = self.market.working.as_deref() == Some(e.id.as_str());
            let title = short_name(&e.name);
            let note = format!(
                "未安装 · {}",
                e.description.as_deref().unwrap_or("装上就换成它")
            );
            let asked = self.confirming(o, cx);
            let note = asked.as_ref().map_or(note.clone(), |(n, _)| n.clone());
            let control = if let Some((_, buttons)) = asked {
                buttons
            } else if working {
                div()
                    .text_size(px(T::SMALL))
                    .text_color(T::color(T::secondary()))
                    .child("正在安装…")
            } else {
                let offer = o.clone();
                div().child(
                    secondary(format!("complete-install-{}", e.id), "安装")
                        .when(busy, |b| b.opacity(0.5))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.market.working.is_none() {
                                this.install_offer(offer.clone(), cx);
                            }
                        })),
                )
            };
            rows.push(
                self.row(rows.is_empty(), &title, &note, control)
                    .into_any_element(),
            );
        }
        rows
    }

    /// Installed plugins with their switches, then the catalog, then each declaration
    /// file's outcome for whoever writes plugins.
    pub(super) fn plugins_page(&self, cx: &Context<Self>) -> gpui::Div {
        use mooring_core::agent;
        if !self.shows(Page::Plugins, "插件", cx) {
            return div();
        }
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
        let offers = self.market.offers();
        let busy = self.market.working.is_some();

        let animations = self.animations(&offers, busy, cx);

        // ── 已安装 ──
        let mut installed = Vec::new();
        if let Some(done) = &self.market.done {
            let (ok, text) = match done {
                Ok(t) => (true, t.clone()),
                Err(e) => (false, e.clone()),
            };
            installed.push(
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
        let found = plugin::report();
        if found.is_empty() {
            installed.push(
                self.row(
                    installed.is_empty(),
                    "还没有装插件",
                    "从下面「可安装」里挑一个，点安装就生效。",
                    div(),
                )
                .into_any_element(),
            );
        }
        for f in found {
            let first = installed.is_empty();
            let Ok(p) = &f.result else {
                let name = f
                    .dir
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                installed.push(
                    self.row(
                        first,
                        &name,
                        f.result.as_ref().err().map_or("", String::as_str),
                        status(false, "没有加载".into()),
                    )
                    .into_any_element(),
                );
                continue;
            };
            let update = offers.iter().find(|o| {
                o.entry.id == p.id
                    && p.version
                        .as_deref()
                        .is_none_or(|v| market::newer(&o.entry.version, v))
            });
            let mut note = vec![format!(
                "{}{}",
                p.version
                    .as_deref()
                    .map_or_else(String::new, |v| format!("v{v} · ")),
                kinds(&p.provides)
            )];
            if let Some(d) = &p.description {
                note.push(d.clone());
            }
            if p.enabled {
                note.extend(
                    p.provides
                        .iter()
                        .filter_map(|k| where_to_find(*k))
                        .map(str::to_owned),
                );
            }
            if let Some(u) = update {
                note.push(format!("{}有新版本 v{}", u.origin(), u.entry.version));
            }
            if !p.touches.is_empty() {
                note.push(format!("改了别的程序的文件：{}", p.touches.join("、")));
            }
            note.extend(p.warnings.iter().cloned());
            let runs = plugin::config()
                .map(|c| mooring_core::plugin_context::runs(&plugin::dir(c), &p.id))
                .unwrap_or_default();
            let now = mooring_core::content::now();
            for r in mooring_core::plugin_context::failing(&runs)
                .into_iter()
                .filter(|r| now.saturating_sub(r.at) < 7 * 86_400)
            {
                note.push(format!(
                    "上次运行失败（{}）：{}；终端里 mooring plugins log {} 看全部",
                    r.what, r.detail, p.id
                ));
            }
            let on = p.enabled;
            let (id, name) = (p.id.clone(), p.name.clone());
            let mut controls = div().flex().items_center().gap(px(T::S8));
            let asked = update.and_then(|u| self.confirming(u, cx));
            if let Some((question, buttons)) = asked {
                note.push(question);
                controls = controls.child(buttons);
            } else if let Some(u) = update.cloned() {
                controls = controls.child(
                    secondary(format!("plugin-update-{}", p.id), "更新")
                        .when(busy, |b| b.opacity(0.5))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.market.working.is_none() {
                                this.install_offer(u.clone(), cx);
                            }
                        })),
                );
            }
            let removable = f
                .dir
                .parent()
                .is_some_and(|d| plugin::config().is_some_and(|c| d == plugin::dir(c)));
            if removable {
                let (id, name) = (id.clone(), name.clone());
                controls = controls.child(
                    secondary(format!("plugin-remove-{}", p.id), "卸载")
                        .when(busy, |b| b.opacity(0.5))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.market.working.is_none() {
                                this.uninstall(id.clone(), name.clone(), cx);
                            }
                        })),
                );
            }
            if self.market.working.as_deref() == Some(p.id.as_str()) {
                controls = controls.child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::secondary()))
                        .child("正在处理…"),
                );
            }
            controls = controls.child(switch(format!("plugin-switch-{}", p.id), on).on_click(
                cx.listener(move |this, _, _, cx| {
                    if this.market.working.is_none() {
                        this.switch_plugin(id.clone(), !on, cx);
                    }
                }),
            ));
            installed.push(
                self.row(first, &p.name, &note.join(" · "), controls)
                    .into_any_element(),
            );
            // Its settings, each a row: a switch, a choice to step through, or text to type.
            if p.enabled
                && let Some(plugins) = plugin::config().map(plugin::dir)
            {
                for (o, v) in mooring_core::plugin_context::values(&plugins, p) {
                    installed.push(self.option_row(&p.id, o, v, cx).into_any_element());
                }
            }
            // Each right-click action of the plugin, switched one by one.
            if p.enabled && p.provides.contains(&Kind::Actions) {
                for a in mooring_core::action::all()
                    .iter()
                    .filter(|a| a.file.parent() == Some(p.dir.as_path()))
                {
                    let on = a.enabled();
                    let aid = a.id.clone();
                    installed.push(
                        self.row(
                            false,
                            &format!("　└ {}", a.title),
                            &a.command,
                            switch(format!("action-switch-{}", a.id), on).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.text_error =
                                        mooring_core::action::set_enabled(&aid, !on).err();
                                    cx.notify();
                                },
                            )),
                        )
                        .into_any_element(),
                    );
                }
            }
        }
        installed.push(
            self.row(
                false,
                "重新读取",
                "手动改了插件目录，或在终端里用 mooring plugins install 装了插件后，点这里马上生效",
                secondary("plugins-reload", "重新读取").on_click(cx.listener(|this, _, _, cx| {
                    this.market = Market {
                        remote: this.market.remote.take(),
                        ..Market::read()
                    };
                    this.plugins_changed(cx);
                })),
            )
            .into_any_element(),
        );

        // ── 可安装 ──
        let mut available = Vec::new();
        let have = |id: &str| {
            plugin::report()
                .iter()
                .any(|f| f.result.as_ref().is_ok_and(|p| p.id == id))
        };
        for o in offers.iter().filter(|o| !have(&o.entry.id)) {
            let e = &o.entry;
            let first = available.is_empty();
            let working = self.market.working.as_deref() == Some(e.id.as_str());
            let mut note = vec![format!(
                "v{} · {} · {}",
                e.version,
                kinds(&e.provides),
                o.origin()
            )];
            if let Some(d) = &e.description {
                note.push(d.clone());
            }
            let control = if working {
                div().child(status(true, "正在安装…".into()))
            } else if let Some((question, buttons)) = self.confirming(o, cx) {
                note = vec![question];
                buttons
            } else {
                let offer = o.clone();
                div().child(
                    secondary(format!("plugin-install-{}", e.id), "安装")
                        .when(busy, |b| b.opacity(0.5))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.market.working.is_none() {
                                this.install_offer(offer.clone(), cx);
                            }
                        })),
                )
            };
            available.push(
                self.row(first, &e.name, &note.join(" · "), control)
                    .into_any_element(),
            );
        }
        if available.is_empty() {
            available.push(
                self.row(
                    true,
                    "目录里的插件都装上了",
                    self.market
                        .bundled_error
                        .as_deref()
                        .unwrap_or("可以查一下远程目录有没有新的。"),
                    div(),
                )
                .into_any_element(),
            );
        }
        let url = market::remote_url();
        let remote_note = match &self.market.remote {
            _ if self.market.fetching => "正在获取…".to_owned(),
            None => format!("{url}；只在点了之后才联网"),
            Some(Ok(r)) => format!("{url}；有 {} 个插件，更新的和新的已列在上面", r.len()),
            Some(Err(e)) => format!("获取失败：{e}"),
        };
        available.push(
            self.row(
                false,
                "远程目录",
                &remote_note,
                secondary("plugins-fetch", "检查远程目录")
                    .when(self.market.fetching, |b| b.opacity(0.5))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.market.fetching {
                            this.fetch_remote(cx);
                        }
                    })),
            )
            .into_any_element(),
        );

        // ── 声明文件明细 ──
        let where_ = agent::config_dir().map_or_else(
            || "~/.config/mooring".to_owned(),
            |d| d.display().to_string(),
        );
        let files = |rows: Vec<(String, Result<String, String>)>| {
            rows.into_iter()
                .enumerate()
                .map(|(i, (file, result))| {
                    match result {
                        Ok(did) => {
                            self.row(i == 0, &tilde(&file), &did, status(true, "生效".into()))
                        }
                        Err(e) => {
                            self.row(i == 0, &tilde(&file), &e, status(false, "没有生效".into()))
                        }
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
        detail.extend(report(mooring_core::action::report(), |t| {
            format!("加进右键菜单和 ⌘P：{t}")
        }));
        detail.extend(report(mooring_core::rule::report(), |t| {
            format!("这些命令算跑过验证：{t}")
        }));
        detail.extend(report(mooring_core::hook::report(), |t| {
            format!("会话状态变化时由后台调用：{t}；终端里 mooring plugins log 看运行记录")
        }));
        detail.extend(report(mooring_core::sidebar::report(), |t| {
            format!("左栏顶部多了：{t}")
        }));
        detail.extend(report(mooring_core::decoration::report(), |t| {
            format!("窗格标题栏或悬停卡片里显示：{t}")
        }));
        detail.extend(report(mooring_core::effect::report(), |t| t));
        detail.extend(report(mooring_core::quota::report(), |t| {
            format!("状态栏额度多了：{t}")
        }));
        detail.extend(report(agent::report(), |t| format!("注册为 {t}")));

        self.cards(vec![
            ("完成动画", animations),
            ("已安装", installed),
            ("可安装", available),
            ("声明文件明细", files(detail)),
        ])
        .child(div().pt(px(T::S12)).px(px(T::S4)).child(label(format!(
            "插件放在 {}/plugins/<id>/；写插件的说明：在终端运行 mooring --skill plugins。",
            tilde(&where_)
        ))))
    }
}
