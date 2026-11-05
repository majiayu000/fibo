//! What a sender asks of the workspace: one `Edit`, applied by `Workspace::edit`.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edit {
    Settings(Settings),
    AddSpace(Space),
    /// Rename a space or point it at another directory.
    UpdateSpace(Space),
    AddTask(Task),
    UpdateTask(Task),
    SetTaskOutcome {
        id: String,
        outcome: Outcome,
        /// The outcome the sender saw; the edit is refused when it changed since.
        /// `Workspace::stamp` fills it from the sender's copy. Scripts may leave it out.
        #[serde(default)]
        seen: Option<Outcome>,
    },
    SetPendingTask {
        terminal: String,
        task: Option<String>,
    },
    /// Opens a tab. Panes it names that another tab shows move here, and a tab left
    /// without panes closes. Refused if the id is taken: it never overwrites a tab.
    CreateTab {
        tab: Tab,
    },
    /// An empty title hands the name back to what runs in the tab.
    RenameTab {
        id: String,
        title: String,
    },
    /// Removes the tab; the host stops every terminal it held. With `expect`, the
    /// terminals the user agreed to end: refused if the tab holds others by now.
    CloseTab {
        id: String,
        #[serde(default)]
        expect: Option<Vec<String>>,
    },
    /// Takes one terminal out of its tab, closing the tab with its last pane; the host
    /// stops that terminal.
    ClosePane {
        terminal: String,
    },
    /// Places a terminal no tab shows yet beside `target`, in `target`'s tab, applied
    /// to the layout as it is then, so it never undoes a change made meanwhile.
    SplitPane {
        target: String,
        terminal: String,
        axis: Axis,
        before: bool,
        /// The new pane's share of the space it splits, in basis points.
        share: u16,
        /// Split by the user's hand: placed even where the halves come out cramped,
        /// which scripts and other Agents (`mooring split`) are refused.
        #[serde(default)]
        forced: bool,
    },
    /// `terminal` dropped on `target`: half of its space on `side` (axis, before), out of
    /// whichever tab showed it; with no side, the two in one tab trade places.
    MovePane {
        terminal: String,
        target: String,
        side: Option<(Axis, bool)>,
    },
    /// Drags the divider of the split at `path` in `tab`; refused if no split with
    /// that axis stands there any more.
    ResizeSplit {
        tab: String,
        path: Vec<bool>,
        axis: Axis,
        /// The first side's share, in basis points.
        ratio: u16,
    },
    /// A terminal relaunched in place of an interrupted one takes its pane.
    ReplacePane {
        old: String,
        new: String,
    },
    SetOutcome {
        id: String,
        outcome: Outcome,
        /// What the sender saw: refused when the outcome changed or the Agent began
        /// another turn since. `Workspace::stamp` fills it. Scripts may leave it out.
        #[serde(default)]
        seen: Option<SeenSession>,
    },
    AssignTask {
        id: String,
        task: Option<String>,
    },
    SaveSessionTask {
        session: String,
        task: Task,
    },
    /// Names a conversation; an empty name hands it back to the CLI and the first ask.
    RenameSession {
        id: String,
        name: String,
    },
    /// Puts a tab before another one, or last; the strip shows tabs in this order.
    MoveTab {
        id: String,
        before: Option<String>,
    },
    /// Removes a space and closes its tabs; the host stops the terminals they held.
    /// Refused while it is the only space or still has unsettled tasks.
    RemoveSpace {
        id: String,
    },
}

/// The part of a session an outcome edit depends on, as its sender saw it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenSession {
    pub outcome: Outcome,
    /// The turn the sender judged (`Content::turn_key`); empty with no content.
    pub turn: String,
}
