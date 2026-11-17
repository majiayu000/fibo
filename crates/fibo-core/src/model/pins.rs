//! 置顶: the terminals whose session the sidebar lists first. The host keeps them in the
//! workspace, so a window on another machine looking at this one sees and changes the same
//! pins (docs/decisions/2026-11-17-pins-on-host.md).
use super::*;

impl Workspace {
    /// Pins or unpins a running terminal. A pin lasts as long as its terminal runs: those
    /// of terminals that ended or were closed come off with any change.
    pub(super) fn pin_terminal(&mut self, id: &str, pinned: bool) -> Result<(), String> {
        let running = |w: &Self, id: &str| {
            (w.terminals.iter()).any(|t| t.id == id && t.state == ProcessState::Running)
        };
        if pinned && !running(self, id) {
            return Err(t!("终端不存在").into());
        }
        let mut pins = self.pinned_terminals.take().unwrap_or_default();
        pins.retain(|p| p != id && running(self, p));
        if pinned {
            pins.push(id.to_owned());
        }
        self.pinned_terminals = Some(pins);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> Workspace {
        let mut w = crate::sample::every_state();
        w.pinned_terminals = Some(Vec::new());
        w
    }
    fn running(w: &Workspace) -> Vec<String> {
        (w.terminals.iter())
            .filter(|t| t.state == ProcessState::Running)
            .map(|t| t.id.clone())
            .collect()
    }

    #[test]
    fn pins_keep_their_order_and_come_off_again() {
        let mut w = workspace();
        let ids = running(&w);
        assert!(ids.len() >= 2, "the sample needs two running terminals");
        for id in [&ids[1], &ids[0]] {
            w.edit(Edit::PinTerminal {
                id: id.clone(),
                pinned: true,
            })
            .unwrap();
        }
        assert_eq!(
            w.pinned_terminals,
            Some(vec![ids[1].clone(), ids[0].clone()])
        );
        // Pinning again does not list it twice.
        w.edit(Edit::PinTerminal {
            id: ids[1].clone(),
            pinned: true,
        })
        .unwrap();
        assert_eq!(
            w.pinned_terminals,
            Some(vec![ids[0].clone(), ids[1].clone()])
        );
        w.edit(Edit::PinTerminal {
            id: ids[0].clone(),
            pinned: false,
        })
        .unwrap();
        assert_eq!(w.pinned_terminals, Some(vec![ids[1].clone()]));
    }

    #[test]
    fn a_terminal_that_ended_cannot_be_pinned_and_drops_its_pin() {
        let mut w = workspace();
        let ids = running(&w);
        w.edit(Edit::PinTerminal {
            id: ids[0].clone(),
            pinned: true,
        })
        .unwrap();
        assert!(
            w.edit(Edit::PinTerminal {
                id: "nobody".into(),
                pinned: true
            })
            .is_err()
        );
        let t = w.terminals.iter_mut().find(|t| t.id == ids[0]).unwrap();
        t.state = ProcessState::Exited;
        w.edit(Edit::PinTerminal {
            id: ids[1].clone(),
            pinned: true,
        })
        .unwrap();
        assert_eq!(w.pinned_terminals, Some(vec![ids[1].clone()]));
    }

    /// A window from before pins moved to the host reads a workspace that has them, and a
    /// new window reads one from an old host as having none of its own.
    #[test]
    fn pins_pass_old_readers_and_old_hosts() {
        let mut w = workspace();
        w.pinned_terminals = Some(vec!["a".into()]);
        let json = serde_json::to_value(&w).unwrap();
        assert_eq!(json["pinned_terminals"], serde_json::json!(["a"]));
        let mut old = json.clone();
        old.as_object_mut().unwrap().remove("pinned_terminals");
        let read: Workspace = serde_json::from_value(old).unwrap();
        assert_eq!(read.pinned_terminals, None);
    }
}
