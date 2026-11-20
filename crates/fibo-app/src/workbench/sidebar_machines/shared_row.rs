//! Another machine's sessions in this column: listed from that machine's own workbench
//! here by the rules this machine's list uses, and drawn by `flat_row` as this machine's
//! rows are. What is added for the other machine (its mark, 断开, 先不管) and where its
//! clicks go is here.
use super::*;
use crate::model::session_state::resume_terminal;
use crate::workbench::diagnostics::TipLine;
use crate::workbench::sidebar_list::{End, Listed, RowDetail, SessionAction};
use fibo_core::status::live_terminal;

/// One session of another machine, as its row is drawn and sorted.
pub(in crate::workbench) struct MachineRow<'a> {
    pub(in crate::workbench) index: usize,
    /// That machine's workbench here: what the row reads.
    pub(in crate::workbench) from: &'a Workbench,
    pub(in crate::workbench) l: Listed<'a>,
    /// The name you gave it in `machines.json`, for this column.
    machine: String,
    pub(in crate::workbench) online: bool,
    why: Option<String>,
    /// Keys its peek, folds and subagents' fold, unique across machines.
    key: String,
    /// Its running terminal there; None once it ended.
    terminal: Option<String>,
    pub(in crate::workbench) bucket: Bucket,
    /// When it last moved; a running turn's start, so running rows hold still.
    pub(in crate::workbench) when: u64,
    pub(in crate::workbench) shown: bool,
    /// Its machine is 先不管: gray, and links coming and going do not show.
    quiet: bool,
    /// Where it is in its machine's 置顶, when it is there.
    pub(in crate::workbench) pin: Option<usize>,
    pub(in crate::workbench) space: String,
    pub(in crate::workbench) needs_you: bool,
}

impl MachineRow<'_> {
    pub(in crate::workbench) fn key(&self) -> &str {
        &self.key
    }
    /// What its clicks and menu act on there: its terminal, or the session once it ended.
    pub(in crate::workbench) fn target(&self) -> &str {
        self.terminal.as_deref().unwrap_or(&self.l.0.id)
    }
    pub(in crate::workbench) fn quiet(&self) -> bool {
        self.quiet
    }
    /// Dropped off: faded, says 断开, opens nothing.
    pub(in crate::workbench) fn dropped(&self) -> bool {
        !self.online && !self.quiet
    }
}

impl Workbench {
    /// Machine `index`'s sessions this column lists: open ones and the newest ended ones,
    /// left out where this machine's list leaves them out (automation, 先不管).
    pub(in crate::workbench) fn session_rows_of<'a>(
        &'a self,
        index: usize,
        cx: &'a gpui::App,
    ) -> Vec<MachineRow<'a>> {
        let (Some(remote), Some(view)) = (self.machines.get(index), self.machine_view(index))
        else {
            return Vec::new();
        };
        let w: &'a Workbench = view.read(cx);
        let v = remote.view.lock().unwrap();
        let machine = machine_label(&v.spec.host).to_owned();
        let online = v.link == Link::Online;
        let why = match &v.link {
            Link::Offline(why) => Some(why.clone()),
            _ => None,
        };
        let now = crate::model::time::now();
        let row = |s: &'a AgentSession, terminal: Option<String>| {
            let (g, attention) =
                crate::model::session_state::session_status(&w.workspace, &w.attention.unseen, s);
            let l = (s, g, attention);
            let bucket = w.row_bucket(&l);
            let key = remote_key(&v, terminal.as_deref().unwrap_or(&s.id));
            let when = match bucket {
                Bucket::Running => w.turn_started(s),
                _ => None,
            }
            .unwrap_or_else(|| w.session_updated(s));
            MachineRow {
                index,
                from: w,
                l,
                machine: machine.clone(),
                online,
                why: why.clone(),
                shown: self.peek.as_ref().is_some_and(|p| p.terminal == key),
                key,
                pin: (terminal.as_ref()).and_then(|t| v.pins.iter().flatten().position(|p| p == t)),
                terminal,
                bucket,
                when,
                quiet: v.spec.quiet,
                space: w.workspace.session_space(s).unwrap_or_default().to_owned(),
                needs_you: w.workspace.needs_you(s, now),
            }
        };
        let listed = (w.workspace.agent_sessions.iter()).filter(|s| w.session_hidden(s).is_none());
        let mut ended: Vec<_> = listed.clone().filter(|s| w.session_closed(s)).collect();
        ended.sort_by_key(|s| std::cmp::Reverse(w.session_updated(s)));
        ended.truncate(crate::workbench::sidebar_wasm::CLOSED_SHOWN);
        let open: Vec<_> = listed
            .filter(|s| w.session_open(s))
            .map(|s| {
                let terminal = live_terminal(&w.workspace, s)
                    .or_else(|| resume_terminal(&w.workspace, &w.resuming.resumed, s));
                row(s, terminal.map(|t| t.id.clone()))
            })
            .collect();
        open.into_iter()
            .chain(ended.into_iter().map(|s| row(s, None)))
            .collect()
    }
    /// `r` drawn by `flat_row` from its machine's workbench, with `detail` its second line.
    pub(in crate::workbench) fn machine_row(
        &self,
        r: &MachineRow,
        detail: Option<RowDetail>,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        self.flat_row(r.from, Some(r), r.l, None, detail, End::State, cx)
            .into_any_element()
    }
    /// Its second line as the chosen preset shows it.
    pub(in crate::workbench) fn machine_detail(&self, r: &MachineRow) -> Option<RowDetail> {
        self.listed_detail(r.from, &r.l, r.shown)
    }
    /// Its machine's square, when this list holds more than one machine.
    pub(in crate::workbench) fn there_mark(&self, r: &MachineRow) -> Option<gpui::Div> {
        (!r.quiet && self.machine_layout() == Machines::Mixed && self.machine_scope().is_none())
            .then(|| machine_mark(machine_letter(&r.machine), !r.online))
    }
    /// Its space's tag, gray on a machine put aside.
    pub(in crate::workbench) fn there_tag(
        &self,
        r: &MachineRow,
        id: &str,
        name: &str,
    ) -> gpui::Div {
        if r.quiet {
            quiet::quiet_tag(name)
        } else {
            self.space_tag(id, name)
        }
    }
    /// The hover card while its machine is not connected.
    pub(in crate::workbench) fn there_tip(&self, r: &MachineRow) -> Option<Vec<TipLine>> {
        let text = match (&r.why, r.online) {
            (_, true) => return None,
            _ if r.quiet => t!("{machine} 现在没连上", machine = r.machine),
            (Some(why), _) => t!("{machine} 没连上：{why}", machine = r.machine, why),
            (None, false) => t!("正在连 {machine}", machine = r.machine),
        };
        Some(vec![TipLine::Title(text)])
    }
    /// A click on its row or a branch under it: open it here, or rename it on a double
    /// click, both by its machine's workbench.
    pub(in crate::workbench) fn click_there(
        &mut self,
        index: usize,
        target: &str,
        rename: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = if rename {
            SessionAction::Rename
        } else {
            SessionAction::Open
        };
        self.remote_action(index, target, &action, window, cx);
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// A plain click on machine `index`'s row for `terminal`, as its row's click does.
    pub fn visual_click_remote_row(
        &mut self,
        index: usize,
        terminal: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = (self.session_rows_of(index, cx).into_iter())
            .find(|r| r.terminal.as_deref() == Some(terminal))
            .map(|r| r.target().to_owned());
        if let Some(target) = target {
            self.click_there(index, &target, false, window, cx);
        }
    }
    /// The sessions machine `index` lists in this column, by id.
    pub fn visual_machine_sessions(&self, index: usize, cx: &gpui::App) -> Vec<String> {
        (self.session_rows_of(index, cx).iter())
            .map(|r| r.l.0.id.clone())
            .collect()
    }
}
