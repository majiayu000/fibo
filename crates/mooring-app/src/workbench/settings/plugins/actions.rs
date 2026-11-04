//! What the plugin page's buttons do: installing after asking, removing, switching. Each runs
//! off the UI thread, since a plugin's setup or teardown may take seconds.
use super::*;

impl SettingsView {
    /// Every way to install or update starts here: fetch it, read what it does, and ask
    /// when that runs commands (or an update adds to what runs). Nothing changes until then.
    pub(super) fn install_offer(&mut self, offer: Offer, cx: &mut Context<Self>) {
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
    pub(super) fn commit_install(
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
    pub(super) fn confirming(
        &self,
        offer: &Offer,
        cx: &Context<Self>,
    ) -> Option<(String, gpui::Div)> {
        let waiting = self.market.confirm.as_ref()?;
        if waiting.id != offer.entry.id {
            return None;
        }
        let e = &offer.entry;
        let lead = if waiting.added.is_empty() {
            "装上后它会："
        } else {
            ""
        };
        let note = format!("{lead}{}。确认吗？", waiting.lines().join("；"));
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
    /// Off the UI thread: the plugin's `teardown` runs first and may take seconds.
    pub(super) fn uninstall(
        &mut self,
        id: String,
        name: String,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        self.market.working = Some(id.clone());
        self.market.done = None;
        self.market.stuck = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let which = id.clone();
            let result = cx
                .background_spawn(async move { plugin::uninstall(&which, force) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.market.working = None;
                if result.is_err() {
                    this.market.stuck = Some((id, name.clone(), true));
                }
                this.market.done = Some(match result {
                    Ok(None) => Ok(format!("卸载了「{name}」")),
                    Ok(Some(w)) => Ok(format!("卸载了「{name}」。注意：{w}")),
                    Err(e) => Err(format!("没卸掉「{name}」：{e}")),
                });
                this.plugins_changed(cx);
            });
        })
        .detach();
    }
    /// Off the UI thread, like `uninstall`: turning one on or off runs its setup or teardown.
    pub(super) fn switch_plugin(
        &mut self,
        id: String,
        on: bool,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        self.market.working = Some(id.clone());
        self.market.stuck = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let which = id.clone();
            let result = cx
                .background_spawn(async move { plugin::set_enabled(&which, on, force) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.market.working = None;
                match result {
                    Ok(None) => {}
                    Ok(Some(w)) => this.market.done = Some(Ok(format!("停用了。注意：{w}"))),
                    Err(e) if on => this.text_error = Some(format!("没能打开插件：{e}")),
                    Err(e) => {
                        let name = plugin::installed(&id).map_or(id.clone(), |p| p.name.clone());
                        this.market.done = Some(Err(format!("没能停用「{name}」：{e}")));
                        this.market.stuck = Some((id, name, false));
                    }
                }
                this.plugins_changed(cx);
            });
        })
        .detach();
    }
}
