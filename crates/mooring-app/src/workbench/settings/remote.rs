//! 设置 › 手机与远程: phones on a relay, the operation log, and other machines.
use super::*;

impl SettingsView {
    /// The key as typed: None leaves the saved one.
    fn typed_key(&self, cx: &gpui::App) -> Option<String> {
        Some(self.relay_key.read(cx).text.trim().to_owned()).filter(|k| !k.is_empty())
    }
    fn send_remote(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let relay = self.relay.read(cx).text.trim().to_owned();
        if enabled && relay.is_empty() {
            self.remote_error = Some(t!("先填中继地址"));
            cx.notify();
            return;
        }
        self.remote_error = None;
        let key = self.typed_key(cx);
        self.relay_key.update(cx, |f, cx| {
            f.set_text("");
            cx.notify();
        });
        cx.emit(SettingsAction::Remote {
            enabled,
            relay,
            key,
        });
    }
    pub(super) fn phones_page(&self, cx: &Context<Self>) -> gpui::Div {
        let r = &self.remote;
        let mut link = Vec::new();
        if self.shows(Page::Phones, t!("让手机连过来"), cx) {
            link.push(
                self.row(
                    true,
                    t!("让手机连过来"),
                    t!("这台 Mac 一直挂在中继上，配对过的手机在哪都能连过来；关掉后手机暂时连不上，配对还在。"),
                    switch("remote-on", r.enabled).on_click(cx.listener(|this, _, _, cx| {
                        let on = !this.remote.enabled;
                        this.send_remote(on, cx)
                    })),
                )
                .into_any_element(),
            );
        }
        if self.shows(Page::Phones, t!("中继地址"), cx) {
            let field = |f: &Entity<Field>| div().w(px(T::FORM_WIDTH * 0.6)).child(f.clone());
            link.push(
                self.row(
                    link.is_empty(),
                    t!("中继地址"),
                    t!("自己部署的 mooring-relay，或部署好的 alma-relay"),
                    field(&self.relay),
                )
                .into_any_element(),
            );
            link.push(
                self.row(
                    false,
                    t!("中继密钥"),
                    if r.keyed {
                        t!("已保存，不显示；要换就填新的")
                    } else {
                        t!("中继要求注册密钥时才填")
                    },
                    field(&self.relay_key),
                )
                .into_any_element(),
            );
            let typed = self.relay.read(cx).text.trim().to_owned();
            if typed != r.relay || self.typed_key(cx).is_some() {
                link.push(
                    self.row(
                        false,
                        t!("改过的地址或密钥"),
                        t!("保存后后台按新的设置重新连接"),
                        secondary("remote-save", t!("保存")).on_click(cx.listener(
                            |this, _, _, cx| {
                                let on = this.remote.enabled;
                                this.send_remote(on, cx)
                            },
                        )),
                    )
                    .into_any_element(),
                );
            }
            let (dot, state) = super::phones::state_line(r);
            link.push(
                self.row(
                    false,
                    t!("状态"),
                    self.remote_error.unwrap_or(""),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(T::S6))
                        .max_w(px(T::FORM_WIDTH * 0.6))
                        .child(div().size(px(T::DOT)).rounded_full().bg(dot))
                        .child(label(state)),
                )
                .into_any_element(),
            );
        }
        let mut phones = Vec::new();
        if self.shows(Page::Phones, t!("操作记录"), cx) {
            phones.push(
                self.row(
                    true,
                    t!("配对的手机"),
                    t!("在标题栏的手机按钮里配对、断开、改权限或取消配对"),
                    label(t!("{len} 台", len = r.devices.len())),
                )
                .into_any_element(),
            );
            phones.push(self.path_row(
                false,
                t!("操作记录"),
                self.runtime.as_ref().map(|r| r.join("remote-log.jsonl")),
                cx,
            ));
        }
        let machines = if self.shows(Page::Phones, t!("其他电脑"), cx) {
            self.machine_rows(cx)
        } else {
            Vec::new()
        };
        self.cards(vec![
            (t!("连接"), link),
            (t!("手机"), phones),
            (t!("其他电脑"), machines),
        ])
    }
    /// 其他电脑: each saved machine with 移除, then a field to add one. Looked at only
    /// for now (docs/multi-machine/spec.md, step 1).
    fn machine_rows(&self, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let mut rows = Vec::new();
        for (index, spec) in self.machines.iter().enumerate() {
            let note = spec
                .path
                .clone()
                .unwrap_or_else(|| t!("自动找那台电脑上的 mooring").into());
            rows.push(
                self.row(
                    rows.is_empty(),
                    &spec.host,
                    &note,
                    secondary(("machine-remove", index), t!("移除")).on_click(cx.listener(
                        move |this, _, _, cx| {
                            let mut next = this.machines.clone();
                            next.remove(index);
                            this.save_machines(next, cx);
                        },
                    )),
                )
                .into_any_element(),
            );
        }
        let typed = self.machine_field.read(cx).text.trim().to_owned();
        rows.push(
            self.row(
                rows.is_empty(),
                t!("加一台电脑"),
                self.machines_error.as_deref().unwrap_or(t!(
                    "填 ssh 能免密登录的主机名；那台电脑上要装着 Fibo。侧栏里只看，不能打字"
                )),
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .w(px(T::FORM_WIDTH * 0.45))
                            .child(self.machine_field.clone()),
                    )
                    .child(
                        secondary("machine-add", t!("添加"))
                            .when(typed.is_empty(), |b| b.opacity(0.5))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let host = this.machine_field.read(cx).text.trim().to_owned();
                                if host.is_empty() {
                                    return;
                                }
                                if this.machines.iter().any(|m| m.host == host) {
                                    this.machines_error = Some(t!("{host} 已经加过了", host));
                                    cx.notify();
                                    return;
                                }
                                let mut next = this.machines.clone();
                                next.push(crate::machines::Spec::new(host));
                                this.machine_field.update(cx, |f, _| f.set_text(""));
                                this.save_machines(next, cx);
                            })),
                    ),
            )
            .into_any_element(),
        );
        rows
    }
    fn save_machines(&mut self, next: Vec<crate::machines::Spec>, cx: &mut Context<Self>) {
        match crate::machines::save_keeping_quiet(&next) {
            Ok(()) => {
                self.machines = next;
                self.machines_error = None;
                cx.emit(SettingsAction::MachinesChanged);
            }
            Err(e) => self.machines_error = Some(t!("没存上：{e}", e)),
        }
        cx.notify();
    }
}
