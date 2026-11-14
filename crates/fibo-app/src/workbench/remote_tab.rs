//! Another machine's work in the main area, as this machine's is in a tab: its terminal
//! docked where the panes go, worked from the start and sized to it, and 「＋」's start page
//! that offers its Agents as an empty tab does. Either stays until the tab underneath, or
//! the view, changes.
use super::render::marked_button;
use super::*;

/// 「＋」 on a machine: where to start there, shown in place of the current tab.
pub(super) struct Start {
    pub(super) machine: usize,
    /// The folder there to start in; unset starts where that machine picks.
    pub(super) cwd: Option<String>,
    /// The tab this covers; showing another one drops it.
    pub(super) home: Option<String>,
}

impl Workbench {
    pub(super) fn show_remote_start(
        &mut self,
        machine: usize,
        cwd: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.leave_for_dock(cx);
        self.peek = None;
        self.remote_start = Some(Start {
            machine,
            cwd,
            home: self.nav.tab.clone(),
        });
        cx.notify();
    }
    /// The board and a todo's page give way, as they do when a local session opens.
    pub(super) fn leave_for_dock(&mut self, cx: &mut Context<Self>) {
        self.nav.task_detail = None;
        if self.nav.overview {
            self.leave_overview(cx);
        }
    }
    /// Another tab shown, or the board: what was docked over the old one goes.
    pub(super) fn drop_moved_dock(&mut self) {
        let away = |home: &Option<String>| self.nav.overview || home != &self.nav.tab;
        if self
            .peek
            .as_ref()
            .is_some_and(|p| p.docked && away(&p.home))
        {
            self.peek = None;
        }
        if self.remote_start.as_ref().is_some_and(|s| away(&s.home)) {
            self.remote_start = None;
        }
    }
    pub(super) fn close_dock(&mut self, cx: &mut Context<Self>) {
        self.remote_start = None;
        self.close_peek(cx);
    }
    /// What the main area shows instead of the tab's panes, if anything is docked there.
    pub(super) fn docked_view(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        if let Some(peek) = self.peek.as_ref().filter(|p| p.docked) {
            return Some(self.peek_view(peek, cx));
        }
        let start = self.remote_start.as_ref()?;
        Some(self.remote_start_view(start, cx))
    }
    fn remote_start_view(&self, start: &Start, cx: &Context<Self>) -> gpui::AnyElement {
        let index = start.machine;
        let (name, known) = match self.machines.get(index) {
            Some(remote) => {
                let view = remote.view.lock().unwrap();
                match &view.machine {
                    Some(m) => (m.name.clone(), m.agent_names.clone()),
                    None => (view.spec.host.clone(), Default::default()),
                }
            }
            None => return div().into_any_element(),
        };
        // The Agents that machine knows; an older one that names none is offered them all.
        let agents: Vec<Agent> = Agent::all()
            .into_iter()
            .filter(|a| known.is_empty() || known.contains_key(&a.key().to_lowercase()))
            .collect();
        let first = Some(self.first_agent())
            .filter(|a| agents.contains(a))
            .or(agents.first().copied());
        let mark = |agent: Agent, ink| super::board::agent_mark_in(agent, ink, T::SMALL);
        let run = |agent: Option<Agent>| {
            let cwd = start.cwd.clone();
            cx.listener(move |this: &mut Self, _: &gpui::ClickEvent, window, cx| {
                this.open_remote_shell(index, cwd.clone(), agent, window, cx)
            })
        };
        let mut others: Vec<gpui::AnyElement> = agents
            .iter()
            .copied()
            .filter(|a| Some(*a) != first)
            .map(|agent| {
                marked_button(
                    format!("remote-start-{}", agent.executable()),
                    agent.name().to_owned(),
                    Some(mark(agent, T::color(T::link()))),
                )
                .px(px(T::S4))
                .text_color(T::color(T::link()))
                .on_click(run(Some(agent)))
                .into_any_element()
            })
            .collect();
        others.push(
            marked_button("remote-start-shell".into(), t!("空终端").into(), None)
                .px(px(T::S4))
                .text_color(T::color(T::link()))
                .on_click(run(None))
                .into_any_element(),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(T::S12))
            .child(
                div()
                    .text_size(px(T::CARD_TITLE))
                    .text_color(T::color(T::primary()))
                    .child(t!("在 {name} 上开始", name)),
            )
            .when_some(start.cwd.clone(), |d, cwd| {
                d.child(label(cwd).text_color(T::color(T::muted())))
            })
            .when_some(first, |d, first| {
                d.child(
                    super::overview::solid(marked_button(
                        format!("remote-start-{}", first.executable()),
                        t!("运行 {name}", name = first.name()),
                        Some(mark(first, T::color(T::on_accent()))),
                    ))
                    .px(px(T::S16))
                    .on_click(run(Some(first))),
                )
            })
            .child(
                div()
                    .max_w(px(T::HERE_WIDTH * 1.5))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .items_center()
                    .gap_x(px(T::S12))
                    .gap_y(px(T::S4))
                    .text_size(px(T::SMALL))
                    .children(others),
            )
            .child(
                button("remote-start-close", t!("取消"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_dock(cx))),
            )
            .into_any_element()
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_remote_start(&mut self, machine: usize, cx: &mut Context<Self>) {
        self.show_remote_start(machine, None, cx);
    }
    pub fn visual_remote_start_shown(&self) -> bool {
        self.remote_start.is_some()
    }
    /// The start page's button for `agent` on `machine`.
    pub fn visual_open_remote_agent(
        &mut self,
        machine: usize,
        agent: Agent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_remote_shell(machine, None, Some(agent), window, cx);
    }
    /// The open peek: whether it is docked in the main area, and whether its terminal is
    /// worked (taken and sized here) rather than looked at.
    pub fn visual_peek_docked(&self, cx: &gpui::App) -> Option<(bool, bool)> {
        let peek = self.peek.as_ref()?;
        Some((peek.docked, !peek.pane.read(cx).peek))
    }
}
