//! Another machine's row's right-click menu: 置顶 / 取消置顶, sent to that machine's host
//! through its workbench here, so its own window and every other one shows the same pin.
//! A host from before it kept pins offers no menu: there is nowhere to keep one.
use super::*;

impl Workbench {
    /// Right-click on another machine's row: its menu at the pointer.
    pub(in crate::workbench) fn open_remote_menu(
        &self,
        r: &RemoteRow,
        cx: &Context<Self>,
    ) -> impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        let (index, terminal) = (r.index, r.card.id.clone());
        cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
            if this.remote_menu_entries(index, &terminal).is_empty() {
                return;
            }
            this.layout.remote_menu = Some((index, terminal.clone(), event.position));
            cx.stop_propagation();
            cx.notify();
        })
    }
    /// What it lists: 置顶 or 取消置顶, when that machine's host keeps pins.
    pub(in crate::workbench) fn remote_menu_entries(
        &self,
        index: usize,
        terminal: &str,
    ) -> Vec<&'static str> {
        let Some(machine) = self.machines.get(index) else {
            return Vec::new();
        };
        let view = machine.view.lock().unwrap();
        match &view.pins {
            Some(pins) if pins.iter().any(|p| p == terminal) => vec![t!("取消置顶")],
            Some(_) => vec![t!("置顶")],
            None => Vec::new(),
        }
    }
    pub(in crate::workbench) fn remote_menu_view(
        &self,
        index: usize,
        terminal: &str,
        position: gpui::Point<gpui::Pixels>,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let entries: Vec<_> = (self.remote_menu_entries(index, terminal).into_iter())
            .map(|name| {
                let terminal = terminal.to_owned();
                button(
                    gpui::SharedString::from(format!("remote-menu-{name}")),
                    name,
                )
                .w_full()
                .justify_start()
                .border_0()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.layout.remote_menu = None;
                    this.pin_remote(index, &terminal, cx);
                    cx.notify();
                }))
                .into_any_element()
            })
            .collect();
        (!entries.is_empty())
            .then(|| self.popup(position, entries, cx, |this| this.layout.remote_menu = None))
    }
    /// Pins or unpins `terminal` on machine `index`, by its host.
    pub(in crate::workbench) fn pin_remote(
        &mut self,
        index: usize,
        terminal: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.machine_view(index) {
            view.update(cx, |w, cx| w.toggle_pin(terminal, cx));
        }
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// What this workbench's host keeps in 置顶 now, asked of it directly.
    pub fn visual_host_pins(&self) -> Option<Vec<String>> {
        match crate::rpc(self.store.socket(), &Request::Workspace) {
            Ok(Reply::Workspace(w)) => w.pinned_terminals,
            _ => None,
        }
    }
    /// Another window on this workbench's host pins or unpins `terminal` there.
    pub fn visual_host_pin(&self, terminal: &str, pinned: bool) {
        let id = terminal.to_owned();
        let edit = Edit::PinTerminal { id, pinned };
        let _ = crate::rpc(
            self.store.socket(),
            &Request::Edit {
                request: None,
                edit,
            },
        );
    }
    /// The first terminal of tab `id`.
    pub fn visual_tab_terminal(&self, id: &str) -> Option<String> {
        self.workspace.tab_terminals(id).into_iter().next()
    }
    /// Machine `index`'s pins as this window's sidebar has them.
    pub fn visual_machine_pins(&self, index: usize) -> Option<Vec<String>> {
        self.machines.get(index)?.view.lock().unwrap().pins.clone()
    }
    /// Right-click on machine `index`'s row for `terminal`, then its menu's one entry: what
    /// that entry said.
    pub fn visual_remote_menu_pin(
        &mut self,
        index: usize,
        terminal: &str,
        cx: &mut Context<Self>,
    ) -> Option<&'static str> {
        let entry = self.remote_menu_entries(index, terminal).first().copied()?;
        self.pin_remote(index, terminal, cx);
        Some(entry)
    }
}
