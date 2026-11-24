//! Other machines' sessions in the left column (`Preset::machines`, docs/multi-machine/
//! spec.md): which rows there are and how one is drawn. State, words and titles come from
//! each machine's own host; a machine that dropped off keeps its last rows, faded.
//! `sidebar_machine_views` holds the three layouts around them.
use super::machines::remote_key;
use super::*;
use crate::machines::Link;
use crate::ui::theme::Flat as F;
use fibo_core::sidebar::{Bucket, Machines, Preset, all as all_presets};
use fibo_remote::protocol::{Card, Phase};

mod add_space;
pub(super) mod quiet;
mod quiet_space;
mod remote_menu;
mod shared_row;
pub(in crate::workbench) use shared_row::MachineRow;

pub(in crate::workbench) use add_space::AddingSpace;

/// A running terminal of another machine, as its host listed it: what the counts of
/// what needs you, ⌘J and the right-click menu go by. The column's rows are its sessions
/// (`MachineRow`).
pub(super) struct RemoteRow {
    /// The name its host reports, which its peek's title shows.
    host_name: String,
    pub(super) online: bool,
    key: String,
    pub(super) card: Card,
    /// Its machine is 先不管: gray, and links coming and going do not show.
    quiet: bool,
}

/// `user@host` reads as `host`: the short name is what fits a row.
pub(super) fn machine_label(host: &str) -> &str {
    host.rsplit('@').next().unwrap_or(host)
}
/// The letter in a machine's square mark.
pub(in crate::workbench) fn machine_letter(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().collect())
}
/// The entry after `at`, or the first when none is current.
fn next_index(len: usize, at: Option<usize>) -> usize {
    at.map_or(0, |i| (i + 1) % len)
}

/// A machine's square letter: gray so it never reads as a state or a space's color.
/// Whether a machine's terminal is a row: an Agent with no conversation tied to it is
/// listed too, as at home.
fn listed_card(c: &Card) -> bool {
    (c.phase != Phase::Shell || c.note.is_some()) && c.outcome.as_deref() != Some("abandoned")
}

/// How wide a machine's square mark is.
pub(in crate::workbench) const MARK: f32 = 15.;

pub(in crate::workbench) fn machine_mark(letter: String, faded: bool) -> gpui::Div {
    div()
        .size(px(MARK))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(T::S4))
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(if faded {
            F::chip_line()
        } else {
            F::chip_on_line()
        }))
        .text_size(px(F::BADGE_TEXT))
        .line_height(px(MARK))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(T::color(if faded { F::third() } else { F::quiet() }))
        .child(letter)
}

impl Workbench {
    /// The presets on offer: a layout for other machines only once there are any.
    pub(super) fn sidebar_presets(&self) -> impl Iterator<Item = &'static Preset> + use<'_> {
        all_presets()
            .iter()
            .filter(|p| !p.only_with_machines() || !self.machines.is_empty())
    }
    pub(super) fn machine_layout(&self) -> Machines {
        if self.machines.is_empty() {
            Machines::Below
        } else {
            self.sidebar.preset().machines
        }
    }
    /// The machine the column shows alone, while it is still configured.
    pub(super) fn machine_scope(&self) -> Option<usize> {
        let host = self.sidebar.machine_scope.as_deref()?;
        matches!(self.machine_layout(), Machines::Mixed | Machines::Rail)
            .then(|| {
                // A quiet machine is never shown alone.
                self.machines.iter().position(|m| {
                    let view = m.view.lock().unwrap();
                    view.spec.host == host && !view.spec.quiet
                })
            })
            .flatten()
    }
    /// Whether this machine's rows are off the list: another machine shown alone, or its
    /// section folded.
    pub(super) fn local_hidden(&self) -> bool {
        self.machine_scope().is_some()
            || (self.machine_layout() == Machines::Sections
                && !self.sidebar.showing("machine:local", true))
    }
    pub(super) fn machine_host(&self, index: usize) -> Option<String> {
        let view = self.machines.get(index)?.view.lock().unwrap();
        Some(view.spec.host.clone())
    }
    pub(super) fn machine_section_key(host: &str) -> String {
        format!("machine:{host}")
    }

    /// Machine `index`'s sessions, as its host listed them last.
    pub(super) fn remote_rows_of(&self, index: usize) -> Vec<RemoteRow> {
        let Some(remote) = self.machines.get(index) else {
            return Vec::new();
        };
        let view = remote.view.lock().unwrap().clone();
        let host_name = view
            .machine
            .as_ref()
            .map_or_else(|| view.spec.host.clone(), |m| m.name.clone());
        let online = view.link == Link::Online;
        view.terminals
            .iter()
            .filter(|c| listed_card(c))
            .map(|card| RemoteRow {
                host_name: host_name.clone(),
                online,
                key: remote_key(&view, &card.id),
                quiet: view.spec.quiet,
                card: card.clone(),
            })
            .collect()
    }
    /// The other machines' rows that go in 会话优先's own list.
    pub(super) fn listed_remote_rows<'a>(&'a self, cx: &'a gpui::App) -> Vec<MachineRow<'a>> {
        match (self.machine_scope(), self.machine_layout()) {
            (Some(index), _) => {
                let spaces = self.machines[index].view.lock().unwrap().spaces.clone();
                // A space that has since gone lists everything rather than nothing.
                let only = (self.sidebar.machine_space.as_ref())
                    .filter(|id| spaces.iter().any(|s| &s.id == *id));
                let mut rows = self.session_rows_of(index, cx);
                rows.retain(|r| only.is_none_or(|id| &r.space == id));
                rows
            }
            (None, Machines::Mixed) if self.space_filter().is_none() => (0..self.machines.len())
                .filter(|i| !self.machine_quiet(*i))
                .flat_map(|i| self.session_rows_of(i, cx))
                .collect(),
            _ => Vec::new(),
        }
    }
    /// How many of a machine's sessions need you now. A machine that dropped off cannot
    /// say, so it counts none.
    pub(super) fn remote_needs(&self, index: usize) -> usize {
        self.remote_rows_of(index)
            .iter()
            .filter(|r| r.online && !r.quiet && r.card.needs_you)
            .count()
    }
    /// What a quiet machine's line counts: the last it listed, connected or not.
    pub(super) fn quiet_waiting(&self, index: usize) -> usize {
        self.machines.get(index).map_or(0, |m| {
            let view = m.view.lock().unwrap();
            let waits = |c: &&Card| c.needs_you && listed_card(c);
            view.terminals.iter().filter(waits).count()
        })
    }

    /// ⌘J: the next session that needs you, on any machine; the column follows it there.
    /// What needs you on the other machines: the count and whether any of it asks or
    /// failed. The title row draws on every terminal frame, so this copies nothing.
    fn remote_wants(&self) -> (usize, bool, bool) {
        let (mut count, mut asks, mut failed) = (0, false, false);
        for machine in &self.machines {
            let view = machine.view.lock().unwrap();
            if view.link != Link::Online || view.spec.quiet {
                continue;
            }
            for card in view
                .terminals
                .iter()
                .filter(|c| c.needs_you && listed_card(c))
            {
                count += 1;
                match card.status.as_deref().and_then(Status::from_key) {
                    Some(Status::Waiting) => asks = true,
                    Some(Status::Error) => failed = true,
                    _ => {}
                }
            }
        }
        (count, asks, failed)
    }
    /// This machine's terminals asking to trust their folder that 要你处理 counts beside its
    /// sessions (`status::trust_needs_you`), in a fixed order.
    pub(super) fn trust_asks(&self) -> Vec<String> {
        let now = crate::model::time::now();
        let mut asks: Vec<String> = (self.store.recognition().terminal_trust.keys())
            .filter(|t| fibo_core::status::trust_needs_you(&self.workspace, t, now))
            .cloned()
            .collect();
        asks.sort();
        asks
    }
    /// What 要你处理 counts on this machine: its sessions and its trust prompts.
    pub(super) fn local_needs(&self) -> usize {
        self.needs_you().len() + self.trust_asks().len()
    }
    /// Whether the title row shows 下一个要你的.
    pub(super) fn anything_waits(&self) -> bool {
        self.wants().0 > 0
    }
    /// How many need you on every machine and the most urgent kind among them (asking,
    /// then failed, then finished for you to check). Drawn as another machine's main area,
    /// the window's count, which sees every machine (`hosts`).
    pub(super) fn wants(&self) -> (usize, Status) {
        if let Some(window) = self.hosts.window_wants {
            return window;
        }
        // Worked out once a second per snapshot: the title row asks every frame.
        let facts = self.facts();
        let (waiting, error) = (facts.needs_asking, facts.needs_failed);
        let needs = self.local_needs();
        let (there, asks, failed) = self.remote_wants();
        let group = if asks || waiting || needs > self.needs_you().len() {
            Status::Waiting
        } else if failed || error {
            Status::Error
        } else {
            Status::Review
        };
        (needs + there, group)
    }
    /// 下一个要你的: every session 要你处理 counts, on every machine, since the key goes to
    /// all of them. The chip takes the color of the most urgent kind it holds (asking, then
    /// failed, then finished for you to check).
    pub(super) fn next_waiting_chip(&self, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        // One count for everything that needs you; the color says the most urgent kind.
        let (count, group) = self.wants();
        let text = fibo_core::status::needs_you_word();
        let (dot_color, ink, wash) = group.colors();
        let key = crate::keymap::current(cx)
            .chords(crate::keymap::Action::NextWaiting)
            .first()
            .map(|c| c.display());
        button("next-waiting", "")
            .aria_label(t!("{text}，共 {count} 个", text, count))
            .h(px(T::NEXT_CHIP))
            .pl(px(T::S8))
            .pr(px(T::S10))
            .gap(px(T::S6))
            .bg(wash)
            .text_color(ink)
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(div().size(px(T::DOT)).rounded_full().bg(dot_color))
            .child(text)
            .child(div().font_features(T::tabular()).child(count.to_string()))
            .when_some(key, |d, key| {
                d.child(
                    div()
                        .px(px(T::S4))
                        .rounded(px(T::CHIP_RADIUS))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(ink)
                        .opacity(T::KBD_OPACITY)
                        .text_size(px(T::KBD_SIZE))
                        .font_weight(gpui::FontWeight::NORMAL)
                        .child(key),
                )
            })
            .on_click(cx.listener(|this, _, window, cx| this.next_waiting(window, cx)))
    }
    pub(super) fn next_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // As another machine's main area, the window's key: it goes to every machine.
        if self.hosts.embedded {
            cx.emit(super::hosts::HostEvent::NextWaiting);
            return;
        }
        enum Next {
            Here(AgentSession),
            Trust(String),
            There(usize, Box<Card>, String),
        }
        let mut all: Vec<(String, Next)> = self
            .needs_you()
            .into_iter()
            .map(|s| (s.id.clone(), Next::Here(s.clone())))
            .collect();
        all.extend(
            self.trust_asks()
                .into_iter()
                .map(|t| (t.clone(), Next::Trust(t))),
        );
        for index in 0..self.machines.len() {
            for r in self.remote_rows_of(index) {
                if r.online && !r.quiet && r.card.needs_you {
                    all.push((r.key, Next::There(index, Box::new(r.card), r.host_name)));
                }
            }
        }
        if all.is_empty() {
            return;
        }
        let current = match self.peek.as_ref().filter(|p| p.remote.is_some()) {
            Some(peek) => Some(peek.terminal.clone()),
            None if self.hosted_view().is_some() => self.hosted_focus(cx),
            None => self
                .nav
                .focused
                .as_ref()
                .and_then(|id| self.current_agent(id))
                .map(|s| s.id.clone())
                .or_else(|| {
                    (self.nav.focused.clone())
                        .filter(|t| self.store.recognition().terminal_trust.contains_key(t))
                })
                .or_else(|| self.overview.here.clone()),
        };
        let at = all
            .iter()
            .position(|(key, _)| Some(key) == current.as_ref());
        let index = next_index(all.len(), at);
        match all.swap_remove(index).1 {
            Next::Here(session) => {
                if self.peek.as_ref().is_some_and(|p| p.remote.is_some()) {
                    self.close_peek(cx);
                }
                let space = self.workspace.session_space(&session).map(str::to_owned);
                self.reveal_machine(None, space.as_deref());
                self.enter_card(&session, window, cx);
            }
            Next::Trust(terminal) => {
                if self.peek.as_ref().is_some_and(|p| p.remote.is_some()) {
                    self.close_peek(cx);
                }
                let space = self
                    .workspace
                    .terminal_tab(&terminal)
                    .map(|t| t.space.clone());
                self.reveal_machine(None, space.as_deref());
                self.focus_terminal(&terminal, window, cx);
            }
            Next::There(index, card, name) => {
                self.reveal_machine(Some(index), Some(&card.space));
                self.open_remote_peek(index, *card, &name, window, cx);
            }
        }
    }
    /// Makes sure the row ⌘J went to, in `space`, is on the list: a space shown alone stays
    /// so when the row is in it, on this machine and on another alike.
    fn reveal_machine(&mut self, index: Option<usize>, space: Option<&str>) {
        let filter = match index {
            None => &mut self.sidebar.space_filter,
            Some(_) => {
                // This machine's space alone lists no other machine's rows.
                self.sidebar.space_filter = None;
                &mut self.sidebar.machine_space
            }
        };
        if filter.as_deref() != space {
            *filter = None;
        }
        let host = index.and_then(|i| self.machine_host(i));
        match self.machine_layout() {
            Machines::Rail => self.set_machine_scope(host),
            Machines::Mixed => {
                if self.sidebar.machine_scope != host {
                    self.set_machine_scope(None);
                }
            }
            Machines::Sections => {
                let key = host.map_or_else(
                    || "machine:local".to_owned(),
                    |h| Self::machine_section_key(&h),
                );
                self.open_sidebar_group(key);
            }
            Machines::Below => {}
        }
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_choose_preset(&mut self, id: &str, cx: &mut Context<Self>) {
        self.choose_sidebar_preset(id, cx);
    }
    /// What the list shows now: other machines' rows on it, the machine shown alone, and
    /// whether this machine's rows are off the list.
    pub fn visual_column(&self, cx: &gpui::App) -> (usize, Option<usize>, bool) {
        (
            self.listed_remote_rows(cx).len(),
            self.machine_scope(),
            self.local_hidden(),
        )
    }
    pub fn visual_scope_machine(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.set_machine_scope(index.and_then(|i| self.machine_host(i)));
        cx.notify();
    }
    pub fn visual_next_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.next_waiting(window, cx);
    }
    pub fn visual_wants(&self) -> usize {
        self.local_needs() + self.remote_wants().0
    }
    pub fn visual_rail_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_settings_page(settings::Page::Phones, window, cx);
    }
    pub fn visual_machine_space_ids(&self, index: usize) -> Vec<String> {
        let view = self.machines[index].view.lock().unwrap();
        view.spaces.iter().map(|s| s.id.clone()).collect()
    }
    pub fn visual_machine_online(&self, index: usize) -> Option<bool> {
        let view = self.machines.get(index)?.view.lock().unwrap();
        Some(view.link == Link::Online)
    }
    pub fn visual_sidebar_presets(&self) -> Vec<String> {
        self.sidebar_presets().map(|p| p.id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_reads_by_its_short_name() {
        assert_eq!(machine_label("apple"), "apple");
        assert_eq!(machine_label("me@apple.local"), "apple.local");
        assert_eq!(machine_letter("apple"), "A");
        assert_eq!(machine_letter("苹果"), "苹");
        assert_eq!(machine_letter(""), "");
    }

    #[test]
    fn the_next_one_cycles_and_starts_at_the_first() {
        assert_eq!(next_index(3, None), 0);
        assert_eq!(next_index(3, Some(0)), 1);
        assert_eq!(next_index(3, Some(2)), 0);
    }

    /// 「为啥有个圆圈」(11-10): a state this build cannot tell draws `?`, not 停着's ring,
    /// on a terminal with no conversation. Another machine's rows are its sessions, whose
    /// state this build works out itself.
    #[test]
    fn a_state_nobody_said_is_not_drawn_as_stopped() {
        let below = include_str!("machines.rs");
        assert!(!below.contains("then_some(Status::Stale)"));
        assert!(below.contains("widgets::unknown_mark("));
        let here = include_str!("sidebar/view.rs");
        assert!(!here.contains("map_or(Status::Stale"));
        assert_eq!(Status::from_key("someday"), None);
    }
}
