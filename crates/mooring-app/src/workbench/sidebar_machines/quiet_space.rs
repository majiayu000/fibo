//! 先不管这个空间 (docs/quiet-space/index.html): a space you put aside. Nothing in it is removed
//! or stopped, and none of it asks for you: it counts nowhere, ⌘J skips it, and it leaves
//! the chips, the lists and the board for a gray line. The host keeps the state
//! (`Workspace::quiet_spaces`) and core decides what it means; this is how it looks. Looking
//! at or typing in its sessions never ends it: only 接着管 does.
use super::quiet::{LineFacts, LineKind, quiet_ring, quiet_words};
use super::*;
use crate::workbench::palette::Hit;
use mooring_core::status;
use std::rc::Rc;

/// More spaces than this fold under one line: the column's foot is not theirs to fill.
const OWN_LINES: usize = 2;
const LINE: f32 = 34.;
const SHOWN_LINES: usize = 6;
const SHOWN_ROWS: usize = 6;

fn fold_key(id: &str) -> String {
    format!("quiet-space:{id}")
}
/// Where the flight finds this space's chip and its line.
fn seat_key(id: &str) -> String {
    format!("space:{id}")
}
const ALL_KEY: &str = "quiet-spaces";

/// The menu's words: the second press names what keeps running, so nothing surprises you.
fn menu_words(quiet: bool, confirming: bool, running: usize) -> String {
    match (quiet, confirming) {
        (true, _) => t!("接着管这个空间").to_owned(),
        (false, true) => t!(
            "确认：{running} 个{label}会继续，不再提醒",
            running,
            label = Status::Running.label()
        ),
        (false, false) => t!("先不管这个空间").to_owned(),
    }
}
fn switch_words(name: &str, quiet: bool) -> String {
    if quiet {
        t!("接着管 空间 {name}", name)
    } else {
        t!("先不管 空间 {name}", name)
    }
}
fn all_words(spaces: usize, waiting: usize) -> String {
    if waiting == 0 {
        t!("{spaces} 个空间", spaces)
    } else {
        t!("{spaces} 个空间 · {waiting} 个在等", spaces, waiting)
    }
}

impl Workbench {
    /// `space`, or the first attended to when it is put aside: for what you are writing
    /// new, which would leave your sight at once there.
    pub(in crate::workbench) fn attended_or_first(&self, space: String) -> String {
        let w = &self.workspace;
        if !w.space_quiet(&space) {
            return space;
        }
        (w.spaces.iter())
            .find(|s| !w.space_quiet(&s.id))
            .map_or(space, |s| s.id.clone())
    }
    fn quiet_spaces(&self) -> Vec<&Space> {
        (self.workspace.spaces.iter())
            .filter(|s| self.workspace.space_quiet(&s.id))
            .collect()
    }
    /// A space's conversations, open in a tab or cut off by a restart: what asks you first,
    /// then the latest.
    fn quiet_space_sessions(&self, id: &str) -> Vec<&AgentSession> {
        let w = &self.workspace;
        let mut rows: Vec<_> = (w.agent_sessions.iter())
            .filter(|s| {
                w.session_home(s) == Some(id)
                    && sessions::automation_hidden_in(w, s).is_none()
                    // A restart ends what ran: still yours to go back to, so still listed.
                    && (self.session_open(s) || w.lost_agents.values().flatten().any(|l| l == &s.id))
            })
            .collect();
        let at = now();
        rows.sort_by_key(|s| {
            (
                !status::asks_for_you(w, s, at),
                std::cmp::Reverse(w.session_updated(s)),
            )
        });
        rows
    }
    fn quiet_space_waiting(&self, id: &str) -> usize {
        let at = now();
        (self.quiet_space_sessions(id).iter())
            .filter(|s| status::asks_for_you(&self.workspace, s, at))
            .count()
    }
    /// How many of a space's conversations are working now, which going on unattended is
    /// worth saying before it stops telling you.
    fn space_running(&self, id: &str) -> usize {
        (self.workspace.agent_sessions.iter())
            .filter(|s| {
                self.workspace.session_home(s) == Some(id)
                    && s.managed()
                    && self.session_open(s)
                    && status_of(&self.workspace, s).working()
            })
            .count()
    }
    /// Both ways between attending and 先不管. Leaving the space you are in shows the next
    /// one attended to; coming back moves nothing.
    pub(in crate::workbench) fn set_space_quiet(
        &mut self,
        id: &str,
        quiet: bool,
        cx: &mut Context<Self>,
    ) {
        self.layout.space_menu = None;
        self.layout.space_quieting = false;
        let w = &self.workspace;
        let Some(name) = (w.spaces.iter().find(|s| s.id == id)).map(|s| s.name.clone()) else {
            return;
        };
        if quiet && !w.space_quiet(id) && w.attended_spaces() <= 1 {
            self.error = Some(t!("至少要留一个在管的空间").to_owned());
            cx.notify();
            return;
        }
        let tile = T::space_tile(self.space_mesh(id));
        let w = &self.workspace;
        self.sidebar.motion.fly(&seat_key(id), &name, quiet, false);
        self.sidebar.motion.fly_as_dot(tile);
        if quiet {
            // Forgotten, not just hidden: coming back must not narrow the list again.
            if self.sidebar.space_filter.as_deref() == Some(id) {
                self.sidebar.space_filter = None;
            }
            let filter = self.overview.filter_space.take().filter(|f| f != id);
            let next = (w.spaces.iter())
                .find(|s| s.id != id && !w.space_quiet(&s.id))
                .map(|s| s.id.clone());
            if let Some(next) = next.filter(|_| self.nav.space == id) {
                self.pick_space(&next, cx);
            }
            self.overview.filter_space = filter;
        }
        self.edit(
            Edit::SetSpaceQuiet {
                id: id.to_owned(),
                quiet,
            },
            cx,
        );
        cx.notify();
    }
    /// You began work here, so the space is back: being told is what you want from work you
    /// started, and one press too many costs less than a question nobody saw. Applied here
    /// at once, or every key until the host's reply would go through this again.
    pub(in crate::workbench) fn work_began_in_space(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.workspace.space_quiet(id) {
            return;
        }
        self.set_space_quiet(id, false, cx);
        self.workspace.quiet_spaces.remove(id);
        self.facts.forget();
    }
    pub(in crate::workbench) fn work_began_on_terminal(
        &mut self,
        terminal: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(space) = self.workspace.terminal_home(terminal).map(str::to_owned) else {
            return;
        };
        self.work_began_in_space(&space, cx);
    }
    /// The menu's press: a first one only says what keeps running, when something does.
    fn press_space_quiet(&mut self, id: &str, cx: &mut Context<Self>) {
        let quiet = self.workspace.space_quiet(id);
        if !quiet && !self.layout.space_quieting && self.space_running(id) > 0 {
            self.layout.space_quieting = true;
            cx.notify();
            return;
        }
        self.set_space_quiet(id, !quiet, cx);
    }
    /// The space menu's entry, in either state.
    pub(in crate::workbench) fn space_quiet_entry(
        &self,
        id: &str,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let w = &self.workspace;
        let quiet = w.space_quiet(id);
        let last = !quiet && w.attended_spaces() <= 1;
        let words = menu_words(quiet, self.layout.space_quieting, self.space_running(id));
        let id = id.to_owned();
        button("space-menu-quiet", words)
            .w_full()
            .justify_start()
            .border_0()
            .when(last, |d| d.opacity(0.4))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !last {
                    this.press_space_quiet(&id, cx);
                }
            }))
            .into_any_element()
    }

    fn quiet_space_line(
        &self,
        space: &Space,
        first: bool,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = space.id.clone();
        let menu_id = id.clone();
        let line = self.quiet_line(
            LineFacts {
                id: format!("quiet-space-{id}"),
                resume_id: format!("quiet-space-resume-{id}"),
                fold: fold_key(&id),
                seat: seat_key(&id),
                name: space.name.clone(),
                words: quiet_words(&space.name, self.quiet_space_waiting(&id)),
                resume: Some(Rc::new(move |this, cx| {
                    this.set_space_quiet(&id, false, cx);
                })),
                also_seats: Vec::new(),
                kind: LineKind::Space {
                    dots: vec![space.id.clone()],
                    first,
                },
            },
            cx,
        );
        line.on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                this.layout.space_menu = Some((menu_id.clone(), event.position));
                this.layout.space_switch = false;
                this.layout.space_removing = false;
                this.layout.space_quieting = false;
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }
    fn quiet_space_row(&self, s: &AgentSession, cx: &Context<Self>) -> gpui::AnyElement {
        let session = s.clone();
        let time = short_ago(self.workspace.session_updated(s), now());
        div()
            .id(gpui::SharedString::from(format!(
                "quiet-space-row-{}",
                s.id
            )))
            .mx(px(T::S8))
            .px(px(T::S10))
            .py(px(F::ONE_PAD_Y))
            .flex()
            .items_center()
            .gap(px(F::ROW_GAP))
            .rounded(px(F::ROW_RADIUS))
            .font_family(T::UI_FONT)
            .cursor_pointer()
            .hover(|d| d.bg(T::color(F::selected())))
            .child(quiet_ring())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(F::TITLE))
                    .line_height(px(F::TITLE_LINE))
                    .text_color(T::color(F::third()))
                    .child(self.workspace.session_title(s)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(T::mono(cx).family)
                    .text_size(px(F::TIME))
                    .line_height(px(F::LINE_SMALL))
                    .text_color(T::color(F::third()))
                    .child(time),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.click_row(&session, false, window, cx);
            }))
            .into_any_element()
    }
    /// One space's line and, unfolded, its rows; the second value is their height.
    fn quiet_space_block(
        &self,
        space: &Space,
        first: bool,
        cx: &Context<Self>,
    ) -> (Vec<gpui::AnyElement>, f32) {
        let mut out = vec![self.quiet_space_line(space, first, cx).into_any_element()];
        let mut height = LINE;
        if self.sidebar.open(&fold_key(&space.id), false) {
            let rows = self.quiet_space_sessions(&space.id);
            let row = F::ONE_PAD_Y * 2. + F::TITLE_LINE;
            let shown = row * rows.len().min(SHOWN_ROWS) as f32;
            height += shown;
            out.push(
                div()
                    .id(gpui::SharedString::from(format!(
                        "quiet-space-rows-{}",
                        space.id
                    )))
                    .h(px(shown))
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .children(rows.iter().map(|s| self.quiet_space_row(s, cx)))
                    .into_any_element(),
            );
        }
        (out, height)
    }
    /// The gray lines of the spaces put aside. A few get a line each; more share one line
    /// that unfolds to theirs, so 先不管 can hold many without taking the column.
    pub(in crate::workbench) fn quiet_space_lines(
        &self,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let spaces = self.quiet_spaces();
        if spaces.len() <= OWN_LINES {
            return (spaces.iter())
                .enumerate()
                .flat_map(|(i, s)| self.quiet_space_block(s, i == 0, cx).0)
                .collect();
        }
        let waiting = spaces.iter().map(|s| self.quiet_space_waiting(&s.id)).sum();
        let name = t!("先不管").to_owned();
        let words = format!("{name} · {}", all_words(spaces.len(), waiting));
        let line = self.quiet_line(
            LineFacts {
                id: "quiet-spaces".to_owned(),
                resume_id: String::new(),
                fold: ALL_KEY.to_owned(),
                seat: ALL_KEY.to_owned(),
                name,
                words,
                resume: None,
                also_seats: spaces.iter().map(|s| seat_key(&s.id)).collect(),
                kind: LineKind::Space {
                    dots: spaces.iter().take(3).map(|s| s.id.clone()).collect(),
                    first: true,
                },
            },
            cx,
        );
        let mut out = vec![line.into_any_element()];
        if self.sidebar.open(ALL_KEY, false) {
            let (mut rows, mut height) = (Vec::new(), 0.);
            for space in &spaces {
                let (block, tall) = self.quiet_space_block(space, false, cx);
                rows.extend(block);
                height += tall;
            }
            // A definite height: a wrapper sized by max_h measures its content again every
            // frame (docs/performance).
            out.push(
                div()
                    .id("quiet-spaces-open")
                    .h(px(f32::min(height, LINE * SHOWN_LINES as f32)))
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .children(rows)
                    .into_any_element(),
            );
        }
        out
    }
    /// The board's line for what it leaves out, under its header.
    pub(in crate::workbench) fn quiet_space_board_lines(
        &self,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        (self.quiet_spaces().into_iter())
            .map(|space| {
                let id = space.id.clone();
                let words = quiet_words(&space.name, self.quiet_space_waiting(&id));
                self.quiet_board_line(
                    words,
                    gpui::SharedString::from(format!("board-quiet-space-resume-{id}")),
                    cx.listener(move |this, _, _, cx| {
                        this.set_space_quiet(&id, false, cx);
                    }),
                )
                .into_any_element()
            })
            .collect()
    }
    /// Over the panes while you look at a space put aside: what you read here is not counted
    /// and will not call you, and the way back is right here.
    pub(in crate::workbench) fn quiet_space_strip(&self, cx: &Context<Self>) -> Option<gpui::Div> {
        if self.nav.overview || !self.workspace.space_quiet(&self.nav.space) {
            return None;
        }
        let id = self.nav.space.clone();
        let name = (self.workspace.spaces.iter())
            .find(|s| s.id == id)
            .map_or("", |s| s.name.as_str());
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(T::S8))
                .px(px(T::S14))
                .py(px(T::S2))
                .border_b(px(T::BORDER_WIDTH))
                .border_color(T::color(T::border()))
                .text_size(px(T::META))
                .text_color(T::color(T::muted()))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(t!("「{name}」先不管中：不计数，也不提醒", name)),
                )
                .child(div().flex_1())
                .child(
                    button("quiet-space-strip-resume", t!("接着管"))
                        .flex_shrink_0()
                        .h(px(T::SEGMENT_HEIGHT))
                        .px(px(T::S6))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_space_quiet(&id, false, cx);
                        })),
                ),
        )
    }

    /// ⌘P: 先不管 or 接着管, found by those words or the space's name.
    pub(in crate::workbench) fn space_hits(&self, query: &str) -> Vec<Hit> {
        let query = query.to_lowercase();
        (self.workspace.spaces.iter())
            .filter(|s| {
                t!("先不管 接着管 恢复 空间 {name}", name = s.name)
                    .to_lowercase()
                    .contains(&query)
            })
            .map(|s| Hit::Space(s.id.clone()))
            .collect()
    }
    pub(in crate::workbench) fn space_hit_title(&self, id: &str) -> String {
        let name = (self.workspace.spaces.iter())
            .find(|s| s.id == id)
            .map_or("", |s| s.name.as_str());
        switch_words(name, self.workspace.space_quiet(id))
    }
    pub(in crate::workbench) fn switch_space_quiet(&mut self, id: &str, cx: &mut Context<Self>) {
        let quiet = self.workspace.space_quiet(id);
        self.set_space_quiet(id, !quiet, cx);
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_set_space_quiet(&mut self, id: &str, quiet: bool, cx: &mut Context<Self>) {
        self.set_space_quiet(id, quiet, cx);
    }
    pub fn visual_space_quiet(&self, id: &str) -> bool {
        self.workspace.space_quiet(id)
    }
    /// Where this space's chip was last drawn.
    pub fn visual_space_chip(&self, id: &str) -> Option<(f32, f32)> {
        self.sidebar.motion.visual_marks(&seat_key(id))[0]
    }
    /// Whether the column's rows or chips list the space's session now.
    pub fn visual_listed_in_column(&self, id: &str) -> bool {
        (self.workspace.agent_sessions.iter()).any(|s| {
            self.workspace.session_home(s) == Some(id)
                && self.session_hidden(s).is_none()
                && self.session_open(s)
        })
    }
    pub fn visual_unfold_quiet_spaces(&mut self, open: bool, cx: &mut Context<Self>) {
        self.sidebar.toggled.insert(ALL_KEY.to_owned(), open);
        cx.notify();
    }
    /// The list narrowed to this space, as a click on its chip does.
    pub fn visual_narrow_to_space(&mut self, id: &str, cx: &mut Context<Self>) {
        self.toggle_space_filter(id, cx);
    }
    pub fn visual_stored_space_filter(&self) -> Option<String> {
        self.sidebar.space_filter.clone()
    }
    /// Into the pane in front of you: keys as you type them, or a mouse report.
    pub fn visual_in_shown_pane(&mut self, text: &str, mouse: bool, cx: &mut Context<Self>) {
        let tab = (self.nav.tab.as_ref()).map(|t| self.workspace.tab_terminals(t));
        let pane = tab
            .and_then(|ids| ids.first().cloned())
            .and_then(|id| self.layout.panes.get(&id).cloned())
            .expect("a pane is shown");
        pane.update(cx, |p, cx| {
            if mouse {
                p.visual_pointer(text, cx);
            } else {
                p.visual_type(text, cx);
            }
        });
    }
    pub fn visual_quiet_space_rows(&self, id: &str) -> usize {
        self.quiet_space_sessions(id).len()
    }
    pub fn visual_quiet_space_waiting(&self, id: &str) -> usize {
        self.quiet_space_waiting(id)
    }
    pub fn visual_board_cards(&self) -> usize {
        self.board_sessions(now()).len()
    }
    pub fn visual_space_running(&self, id: &str) -> usize {
        self.space_running(id)
    }
    pub fn visual_space_menu_open(&self) -> bool {
        self.layout.space_menu.is_some()
    }
    /// A click on the space's first gray row.
    pub fn visual_open_quiet_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let first = self.quiet_space_sessions(id).first().map(|s| (*s).clone());
        let session = first.expect("the space lists a session");
        self.click_row(&session, false, window, cx);
    }
    pub fn visual_looking_at(&self) -> (String, bool) {
        (self.nav.space.clone(), self.quiet_space_strip_shown())
    }
    fn quiet_space_strip_shown(&self) -> bool {
        !self.nav.overview && self.workspace.space_quiet(&self.nav.space)
    }
    /// What ⌘P lists for spaces on `query`, and picks the first as a click would.
    pub fn visual_space_hit(&mut self, query: &str, cx: &mut Context<Self>) -> Option<String> {
        let id = self.space_hits(query).into_iter().find_map(|h| match h {
            Hit::Space(id) => Some(id),
            _ => None,
        })?;
        let title = self.space_hit_title(&id);
        self.switch_space_quiet(&id, cx);
        Some(title)
    }
    pub fn visual_unfold_quiet_space(&mut self, id: &str, open: bool, cx: &mut Context<Self>) {
        self.sidebar.toggled.insert(fold_key(id), open);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_says_what_keeps_running_only_on_the_second_press() {
        assert_eq!(menu_words(false, false, 2), "先不管这个空间");
        let confirm = menu_words(false, true, 2);
        assert!(confirm.contains("2 个"), "{confirm}");
        assert!(confirm.contains(Status::Running.label()), "{confirm}");
        assert_eq!(menu_words(true, false, 0), "接着管这个空间");
    }
    #[test]
    fn the_palette_offers_the_way_back_from_where_it_is() {
        assert_eq!(switch_words("工作", false), "先不管 空间 工作");
        assert_eq!(switch_words("工作", true), "接着管 空间 工作");
    }
    #[test]
    fn the_shared_line_counts_spaces_and_what_waits() {
        assert_eq!(all_words(3, 0), "3 个空间");
        assert_eq!(all_words(3, 2), "3 个空间 · 2 个在等");
    }
}
