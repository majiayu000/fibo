//! 置顶 as this window lists it. A host that keeps pins (`Workspace::pinned_terminals`)
//! owns them, so a window on another machine that looks at it sees and changes the same
//! ones; with a host from before that, this window keeps its own in `<config>/sidebar`, as
//! it did (docs/decisions/2026-11-17-pins-on-host.md).
use super::*;

impl Workbench {
    /// The terminals in 置顶, in the order pinned. Another machine's workbench in this
    /// window with an old host has none: this machine's file holds this machine's pins.
    pub(in crate::workbench) fn pins(&self) -> &[String] {
        match &self.workspace.pinned_terminals {
            Some(pins) => pins,
            None if self.hosts.embedded => &[],
            None => &self.sidebar.pinned,
        }
    }
    /// Whether 置顶 can be changed here: always at home, on another machine only when its
    /// host keeps pins.
    pub(in crate::workbench) fn can_pin(&self) -> bool {
        !self.hosts.embedded || self.workspace.pinned_terminals.is_some()
    }
    /// The session menu's 置顶: pins its terminal to 置顶, or takes it off.
    pub(in crate::workbench) fn toggle_pin(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let pinned = !self.pins().iter().any(|t| t == terminal);
        if let Some(pins) = &mut self.workspace.pinned_terminals {
            // Shown at once; the host's next workspace says what it kept.
            pins.retain(|t| t != terminal);
            if pinned {
                pins.push(terminal.to_owned());
            }
            let id = terminal.to_owned();
            self.edit(Edit::PinTerminal { id, pinned }, cx);
        } else if !self.hosts.embedded {
            let pins = &mut self.sidebar.pinned;
            pins.retain(|t| t != terminal);
            if pinned {
                pins.push(terminal.to_owned());
            }
            self.sidebar.save();
        }
        cx.notify();
    }
    /// Against a workspace the host has sent, never the empty one before it: pins this
    /// window kept itself move to a host that keeps them, once; with a host that does not,
    /// one whose terminal ended or was closed comes off.
    pub(in crate::workbench) fn prune_pins(&mut self, cx: &mut Context<Self>) {
        if self.hosts.embedded
            || self.sidebar.pinned.is_empty()
            || self.workspace.revision == 0
            || self.store.cut_off()
        {
            return;
        }
        let running = |w: &Workspace, id: &str| {
            (w.terminals.iter()).any(|t| t.id == id && t.state == ProcessState::Running)
        };
        if let Some(there) = self.workspace.pinned_terminals.clone() {
            let mine = std::mem::take(&mut self.sidebar.pinned);
            for id in mine {
                if !there.contains(&id) && running(&self.workspace, &id) {
                    self.toggle_pin(&id, cx);
                }
            }
            self.sidebar.save();
            return;
        }
        let before = self.sidebar.pinned.len();
        let w = &self.workspace;
        self.sidebar.pinned.retain(|id| running(w, id));
        if self.sidebar.pinned.len() != before {
            self.sidebar.save();
        }
    }
}
