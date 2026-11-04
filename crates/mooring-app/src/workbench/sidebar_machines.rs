//! Other machines' sessions in the left column (`Preset::machines`, docs/multi-machine/
//! spec.md): which rows there are and how one is drawn. State, words and titles come from
//! each machine's own host; a machine that dropped off keeps its last rows, faded.
//! `sidebar_machine_views` holds the three layouts around them.
use super::machines::{remote_key, shown_title};
use super::sidebar_flat::flat_state;
use super::sidebar_list::{ASKS, bucket};
use super::*;
use crate::machines::Link;
use crate::ui::theme::Flat as F;
use mooring_core::sidebar::{Bucket, Machines, Preset, all as all_presets};
use mooring_remote::protocol::{Card, Phase};

/// What another machine's row shows, worked out once per draw.
pub(super) struct RemoteRow {
    pub(super) index: usize,
    /// The name you gave it in `machines.json`, for this column.
    pub(super) machine: String,
    /// The name its host reports, which its peek's title shows.
    host_name: String,
    pub(super) online: bool,
    why: Option<String>,
    key: String,
    pub(super) card: Card,
    status: Status,
    pub(super) bucket: Bucket,
    /// When it last moved; a terminal that does not say counts as now, since it is live.
    pub(super) when: u64,
    pub(super) shown: bool,
}

/// `user@host` reads as `host`: the short name is what fits a row.
pub(super) fn machine_label(host: &str) -> &str {
    host.rsplit('@').next().unwrap_or(host)
}
/// The letter in a machine's square mark.
pub(super) fn machine_letter(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().collect())
}
/// As `row_bucket` does at home: 要你处理 holds only what that machine's host counts.
fn remote_bucket(status: Status, needs_you: bool) -> Bucket {
    match bucket(status) {
        Bucket::Attention if !needs_you => Bucket::Stopped,
        b => b,
    }
}
/// The entry after `at`, or the first when none is current.
fn next_index(len: usize, at: Option<usize>) -> usize {
    at.map_or(0, |i| (i + 1) % len)
}

/// A machine's square letter: gray so it never reads as a state or a space's color.
impl RemoteRow {
    /// What it asks you, the second line of its row.
    fn asks(&self) -> Option<String> {
        (self.online && self.status.blocked())
            .then(|| self.card.approval.as_ref().or(self.card.waiting.as_ref()))
            .flatten()
            .and_then(|t| t.lines().find(|l| !l.trim().is_empty()))
            .map(|t| t.replacen(ASKS, "问：", 1))
    }
    pub(super) fn is_tall(&self) -> bool {
        self.asks().is_some()
    }
}

pub(super) fn machine_mark(letter: String, faded: bool) -> gpui::Div {
    const MARK: f32 = 15.;
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
                self.machines
                    .iter()
                    .position(|m| m.view.lock().unwrap().spec.host == host)
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
    /// What a host older than this window cannot do yet, in the user's words.
    pub(super) fn machine_limit(&self, index: usize) -> Option<&'static str> {
        let slot = self.machines.get(index)?.slot();
        let linked = slot.lock().unwrap();
        match linked.as_ref()? {
            l if !l.typing => Some("旧版，只能看"),
            l if !l.managing => Some("旧版，不能开关终端"),
            _ => None,
        }
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
        let now = crate::model::time::now();
        let machine = machine_label(&view.spec.host).to_owned();
        let host_name = view
            .machine
            .as_ref()
            .map_or_else(|| view.spec.host.clone(), |m| m.name.clone());
        let online = view.link == Link::Online;
        let why = match &view.link {
            Link::Offline(why) => Some(why.clone()),
            _ => None,
        };
        view.terminals
            .iter()
            // An Agent with no conversation tied to it is listed too, as at home.
            .filter(|c| c.phase != Phase::Shell || c.note.is_some())
            .filter(|c| c.outcome.as_deref() != Some("abandoned"))
            .map(|card| {
                let status = card
                    .status
                    .as_deref()
                    .and_then(Status::from_key)
                    .unwrap_or(Status::Stale);
                let key = remote_key(&view, &card.id);
                RemoteRow {
                    index,
                    machine: machine.clone(),
                    host_name: host_name.clone(),
                    online,
                    why: why.clone(),
                    shown: self.peek.as_ref().is_some_and(|p| p.terminal == key),
                    key,
                    status,
                    bucket: remote_bucket(status, card.needs_you),
                    when: card.updated.unwrap_or(now),
                    card: card.clone(),
                }
            })
            .collect()
    }
    /// The other machines' rows that go in 会话优先's own list.
    pub(super) fn listed_remote_rows(&self) -> Vec<RemoteRow> {
        match (self.machine_scope(), self.machine_layout()) {
            (Some(index), _) => self.remote_rows_of(index),
            (None, Machines::Mixed) if self.space_filter().is_none() => (0..self.machines.len())
                .flat_map(|i| self.remote_rows_of(i))
                .collect(),
            _ => Vec::new(),
        }
    }
    /// How many of a machine's sessions need you now. A machine that dropped off cannot
    /// say, so it counts none.
    pub(super) fn remote_needs(&self, index: usize) -> usize {
        self.remote_rows_of(index)
            .iter()
            .filter(|r| r.online && r.card.needs_you)
            .count()
    }

    /// One session of another machine, in the same shape as `flat_row`. A dropped machine's
    /// row is faded, says 断开 where its time goes, and opens nothing.
    pub(super) fn remote_row(&self, r: &RemoteRow, cx: &Context<Self>) -> gpui::AnyElement {
        let card = &r.card;
        let asks = r.asks();
        let two = asks.is_some();
        let time = if !r.online {
            "断开".to_owned()
        } else {
            card.updated
                .map_or_else(String::new, |at| short_ago(at, now()))
        };
        // Named when this list holds more than one machine.
        let mark = (self.machine_layout() == Machines::Mixed && self.machine_scope().is_none())
            .then(|| machine_mark(machine_letter(&r.machine), !r.online));
        let space = self.machines.get(r.index).and_then(|m| {
            let view = m.view.lock().unwrap();
            view.spaces
                .iter()
                .find(|s| s.id == card.space)
                .map(|s| self.space_tag(&s.id, &s.name))
        });
        let title_ink = match r.bucket {
            Bucket::Attention | Bucket::Running | Bucket::Done => F::title(),
            Bucket::Stopped | Bucket::Fresh => F::quiet(),
            Bucket::Stale => F::second(),
        };
        let open = r
            .online
            .then(|| (r.index, card.clone(), r.host_name.clone()));
        let tip = match (&r.why, r.online) {
            (Some(why), _) => format!("{} 没连上：{why}", r.machine),
            (None, false) => format!("正在连 {}", r.machine),
            (None, true) => format!("{} · {}", r.machine, card.cwd),
        };
        div()
            .id(gpui::SharedString::from(format!("remote-{}", r.key)))
            .relative()
            .mx(px(T::S8))
            .px(px(T::S10))
            .py(px(if two { F::ROW_PAD_Y } else { F::ONE_PAD_Y }))
            .flex()
            .when(!two, |d| d.items_center())
            .gap(px(F::ROW_GAP))
            .rounded(px(F::ROW_RADIUS))
            .font_family(T::UI_FONT)
            .when(!r.online, |d| d.opacity(0.55))
            .when(r.online, |d| {
                d.cursor_pointer().hover(|s| s.bg(T::color(F::selected())))
            })
            .when(r.shown, |d| d.bg(T::color(F::selected())))
            .child(
                div()
                    .flex_shrink_0()
                    .when(two, |d| d.mt(px(T::S4)))
                    .child(flat_state(r.status, false)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(T::S8))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(F::TITLE))
                                    .line_height(px(F::TITLE_LINE))
                                    .text_color(T::color(title_ink))
                                    .child(shown_title(card)),
                            )
                            .children(mark)
                            .children(space)
                            .child(
                                div()
                                    .min_w(px(F::TIME_WIDTH))
                                    .flex_shrink_0()
                                    .flex()
                                    .justify_end()
                                    .font_family(T::mono(cx).family)
                                    .text_size(px(F::TIME))
                                    .line_height(px(F::LINE_SMALL))
                                    .text_color(T::color(F::third()))
                                    .child(time),
                            ),
                    )
                    .when_some(asks, |d, text| {
                        d.child(
                            div()
                                .mt(px(T::S2))
                                .min_w_0()
                                .truncate()
                                .text_size(px(F::SUB))
                                .line_height(px(F::SUB_LINE))
                                .text_color(r.status.colors().1)
                                .child(text),
                        )
                    }),
            )
            .when(r.shown, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(F::ROW_RADIUS))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(F::selected_line())),
                )
            })
            .tooltip(super::diagnostics::tip(vec![tip]))
            .when_some(open, |d, (index, card, name)| {
                d.on_click(cx.listener(move |this, _, window, cx| {
                    this.open_remote_peek(index, card.clone(), &name, window, cx);
                }))
            })
            .into_any_element()
    }

    /// ⌘J: the next session that needs you, on any machine; the column follows it there.
    pub(super) fn next_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        enum Next {
            Here(AgentSession),
            There(usize, Card, String),
        }
        let mut all: Vec<(String, Next)> = self
            .needs_you()
            .into_iter()
            .map(|s| (s.id.clone(), Next::Here(s.clone())))
            .collect();
        for index in 0..self.machines.len() {
            for r in self.remote_rows_of(index) {
                if r.online && r.card.needs_you {
                    all.push((r.key, Next::There(index, r.card, r.host_name)));
                }
            }
        }
        if all.is_empty() {
            return;
        }
        let current = match self.peek.as_ref().filter(|p| p.remote.is_some()) {
            Some(peek) => Some(peek.terminal.clone()),
            None => self
                .nav
                .focused
                .as_ref()
                .and_then(|id| self.workspace.current_agent(id))
                .map(|s| s.id.clone())
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
                self.reveal_machine(None);
                self.enter_card(&session, window, cx);
            }
            Next::There(index, card, name) => {
                self.reveal_machine(Some(index));
                self.open_remote_peek(index, card, &name, window, cx);
            }
        }
    }
    /// Makes sure the row ⌘J went to is on the list.
    fn reveal_machine(&mut self, index: Option<usize>) {
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
    pub fn visual_column(&self) -> (usize, Option<usize>, bool) {
        (
            self.listed_remote_rows().len(),
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

    /// 要你处理 holds what that machine's host counts, as it does at home.
    #[test]
    fn a_remote_row_needs_you_only_when_its_host_says_so() {
        assert_eq!(remote_bucket(Status::Review, true), Bucket::Attention);
        assert_eq!(remote_bucket(Status::Waiting, false), Bucket::Stopped);
        assert_eq!(remote_bucket(Status::Running, false), Bucket::Running);
        assert_eq!(remote_bucket(Status::Stale, false), Bucket::Stale);
    }

    #[test]
    fn the_next_one_cycles_and_starts_at_the_first() {
        assert_eq!(next_index(3, None), 0);
        assert_eq!(next_index(3, Some(0)), 1);
        assert_eq!(next_index(3, Some(2)), 0);
    }
}
