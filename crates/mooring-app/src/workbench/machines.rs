//! Other machines in the sidebar (docs/multi-machine/spec.md, step 1: look only). Each
//! machine is a heading with how it is connected, then its Agent sessions as that
//! machine's host worked them out: state, word and title come from its cards, never
//! decided again here. A row opens the terminal's screen in the peek overlay, read only.
use super::peek::{Peek, RemoteTitle};
use super::*;
use crate::machines::{Link, Remote};
use crate::terminal::pane::Pane;
use crate::ui::widgets::{dot, label};
use mooring_core::status::Status;
use mooring_remote::protocol::{Card, Phase};

impl Workbench {
    /// One connection per machine in `<config>/machines.json`; with none, nothing runs.
    pub(super) fn start_machines(cx: &mut Context<Self>) -> Vec<Remote> {
        let specs = crate::machines::load();
        if specs.is_empty() {
            return Vec::new();
        }
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        cx.spawn(async move |this, cx| {
            use futures::StreamExt;
            while rx.next().await.is_some() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        specs
            .into_iter()
            .map(|spec| {
                let tx = tx.clone();
                Remote::start(
                    spec,
                    |s| mooring_remote::client::Client::spawn(&s.command()),
                    move || {
                        let _ = tx.unbounded_send(());
                    },
                )
            })
            .collect()
    }

    pub(super) fn machine_rows(&self, recent: bool, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
        let mut out = Vec::new();
        for (index, remote) in self.machines.iter().enumerate() {
            let view = remote.view.lock().unwrap().clone();
            let name = view
                .machine
                .as_ref()
                .map_or_else(|| view.spec.host.clone(), |m| m.name.clone());
            let sessions: Vec<&Card> = view
                .terminals
                .iter()
                // An Agent with no conversation tied to it is listed too, as at home.
                .filter(|c| c.phase != Phase::Shell || c.note.is_some())
                .collect();
            let shells = view.terminals.len() - sessions.len();
            let (note, bad) = match &view.link {
                Link::Online => (format!("{} 个会话", sessions.len()), false),
                Link::Connecting => ("连接中…".to_owned(), false),
                Link::Offline(why) => (format!("没连上：{why}"), true),
            };
            out.push(
                div()
                    .id(gpui::SharedString::from(format!("machine-{index}")))
                    .when(recent, |d| d.mx(px(T::S8)))
                    .mt(px(T::S12))
                    .px(px(T::S8))
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(
                        label(name.clone())
                            .flex_shrink_0()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_size(px(T::META)),
                    )
                    .child(
                        label(note)
                            .min_w_0()
                            .truncate()
                            .text_size(px(T::CHIP_SIZE))
                            .text_color(if bad {
                                Tone::Danger.ink()
                            } else {
                                T::color(T::muted())
                            }),
                    )
                    .into_any_element(),
            );
            if view.link != Link::Online {
                continue;
            }
            for card in sessions {
                let status = card
                    .status
                    .as_deref()
                    .and_then(Status::from_key)
                    .or(card.note.is_some().then_some(Status::Stale));
                let title = shown_title(card);
                let word = card.status_label.clone();
                let shown = self
                    .peek
                    .as_ref()
                    .is_some_and(|p| p.terminal == remote_key(&view, &card.id));
                let open = (index, card.clone(), name.clone());
                out.push(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "machine-{index}-{}",
                            card.id
                        )))
                        .when(recent, |d| d.mx(px(T::S8)))
                        .h(px(T::ROW))
                        .px(px(T::S8))
                        .flex()
                        .items_center()
                        .gap(px(T::S8))
                        .rounded(px(T::CONTROL_RADIUS))
                        .cursor_pointer()
                        .when(shown, |d| d.bg(T::color(T::raised())))
                        .hover(|s| s.bg(T::color(T::raised())))
                        .when_some(status, |d, g| d.child(dot(g)))
                        .child(div().flex_1().min_w_0().truncate().child(title))
                        .when_some(word, |d, w| {
                            d.child(
                                label(w)
                                    .flex_shrink_0()
                                    .text_size(px(T::CHIP_SIZE))
                                    .text_color(
                                        status.map_or(T::color(T::muted()), |g| g.colors().1),
                                    ),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let (index, card, name) = open.clone();
                            this.open_remote_peek(index, card, &name, window, cx);
                        }))
                        .into_any_element(),
                );
            }
            if shells > 0 {
                out.push(
                    label(format!("另有 {shells} 个普通终端"))
                        .when(recent, |d| d.mx(px(T::S8)))
                        .px(px(T::S8))
                        .text_size(px(T::CHIP_SIZE))
                        .text_color(T::color(T::muted()))
                        .into_any_element(),
                );
            }
        }
        out
    }

    pub(super) fn open_remote_peek(
        &mut self,
        index: usize,
        card: Card,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(remote) = self.machines.get(index) else {
            return;
        };
        let key = remote_key(&remote.view.lock().unwrap(), &card.id);
        if self.peek.as_ref().is_none_or(|p| p.terminal != key) {
            let slot = remote.slot();
            let id = card.id.clone();
            let pane = cx.new(|cx| Pane::remote(slot, &id, cx));
            cx.observe(&pane, |_, _, cx| cx.notify()).detach();
            let title = shown_title(&card);
            self.peek = Some(Peek {
                terminal: key,
                session: String::new(),
                pane,
                answering: false,
                note: None,
                remote: Some(RemoteTitle {
                    title: format!("{name} · {title}"),
                    status: card.status.as_deref().and_then(Status::from_key),
                    machine: index,
                    terminal: card.id.clone(),
                    slot: remote.slot(),
                }),
            });
        }
        if let Some(peek) = &self.peek {
            window.focus(&peek.pane.focus_handle(cx), cx);
        }
        cx.notify();
    }
}

/// A peek's key for another machine's terminal: never equal to a terminal of this one.
fn remote_key(view: &crate::machines::View, terminal: &str) -> String {
    let machine = view
        .machine
        .as_ref()
        .map_or(view.spec.host.as_str(), |m| m.id.as_str());
    format!("remote:{machine}:{terminal}")
}

/// The name that machine's host gave the session (its `session_title`), else what it says
/// about an Agent it cannot read, else the folder.
fn shown_title(card: &Card) -> String {
    card.title
        .clone()
        .or_else(|| card.note.clone())
        .unwrap_or_else(|| card.cwd.clone())
}

impl Workbench {
    /// The approval another machine's terminal asks now, as that machine's list says.
    pub(super) fn remote_approval(&self, r: &super::peek::RemoteTitle) -> Option<String> {
        let view = self.machines.get(r.machine)?.view.lock().unwrap();
        let card = view.terminals.iter().find(|c| c.id == r.terminal)?;
        card.approval.clone().filter(|a| !a.is_empty())
    }
}

/// Answers another machine's approval over its link, named so a resend answers once.
pub(super) fn answer_remote(
    r: &super::peek::RemoteTitle,
    question: String,
    allow: bool,
) -> impl std::future::Future<Output = std::io::Result<mooring_core::protocol::Reply>> + use<> {
    use mooring_remote::protocol::Op;
    let linked = r.slot.lock().unwrap().clone();
    let terminal = r.terminal.clone();
    async move {
        let linked = linked.ok_or_else(|| std::io::Error::other("那台电脑没连上"))?;
        if !linked.answering {
            return Err(std::io::Error::other("那台电脑不让这里回答审批"));
        }
        let op = Op::Answer {
            terminal,
            question,
            allow,
            request: Some(mooring_core::protocol::RequestId::fresh().id),
        };
        linked
            .client
            .request(op, std::time::Duration::from_secs(10))
            .map(|_| mooring_core::protocol::Reply::Ok)
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Opens machine `index`'s terminal `id` the way a click on its sidebar row does,
    /// once that machine lists it; false until then.
    pub fn visual_peek_remote(
        &mut self,
        index: usize,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(remote) = self.machines.get(index) else {
            return false;
        };
        let (card, name) = {
            let view = remote.view.lock().unwrap();
            let Some(card) = view.terminals.iter().find(|c| c.id == id).cloned() else {
                return false;
            };
            let name = view
                .machine
                .as_ref()
                .map_or(view.spec.host.clone(), |m| m.name.clone());
            (card, name)
        };
        self.open_remote_peek(index, card, &name, window, cx);
        true
    }
    /// The open peek's pane, and whether its title bar says 只读.
    pub fn visual_peek_pane(&self) -> Option<(Entity<Pane>, bool)> {
        let peek = self.peek.as_ref()?;
        let read_only = (peek.remote.as_ref())
            .is_some_and(|r| !r.slot.lock().unwrap().as_ref().is_some_and(|l| l.typing));
        Some((peek.pane.clone(), read_only))
    }
}
