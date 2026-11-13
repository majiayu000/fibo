//! Tabs and their panes, changed one operation at a time. Each applies to the layout as
//! the host has it then, so an Agent's split, a rename and a dragged divider made at the
//! same moment all stay (docs/request-envelope/spec.md, step D).
use super::*;
use crate::layout::Layout;

impl Workspace {
    pub(super) fn edit_tabs(&mut self, edit: Edit) -> Result<(), String> {
        match edit {
            Edit::CreateTab { tab } => {
                if self.tabs.iter().any(|t| t.id == tab.id) {
                    return Err(t!("标签已存在").into());
                }
                let Some(layout) = &tab.layout else {
                    return Err(t!("标签里至少要有一个画面").into());
                };
                if !layout.valid_ratios() {
                    return Err(t!("分屏比例必须在 0 与 1 之间").into());
                }
                if !self.spaces.iter().any(|s| s.id == tab.space) {
                    return Err(t!("空间不存在").into());
                }
                if (tab.default_task.as_ref())
                    .is_some_and(|id| !self.tasks.iter().any(|t| &t.id == id))
                {
                    return Err(t!("默认待办不存在").into());
                }
                let terminals = layout.ids();
                for (i, id) in terminals.iter().enumerate() {
                    if terminals[..i].contains(id) {
                        return Err(t!("终端不能重复摆放").into());
                    }
                    if !self.terminals.iter().any(|t| &t.id == id) {
                        return Err(t!("终端不存在").into());
                    }
                }
                for id in &terminals {
                    self.take_pane(id);
                }
                self.tabs.push(tab);
            }
            Edit::RenameTab { id, title } => {
                self.tab_mut(&id)?.title = title;
            }
            Edit::MoveTab { id, before } => {
                let from = self
                    .tabs
                    .iter()
                    .position(|t| t.id == id)
                    .ok_or(t!("标签不存在"))?;
                if before.as_ref() == Some(&id) {
                    return Ok(());
                }
                let tab = self.tabs.remove(from);
                let to = match &before {
                    Some(other) => self
                        .tabs
                        .iter()
                        .position(|t| &t.id == other)
                        .ok_or(t!("目标标签不存在"))?,
                    None => self.tabs.len(),
                };
                self.tabs.insert(to, tab);
            }
            Edit::CloseTab { id, expect } => {
                let mut held = self.tab_terminals(&id);
                if !self.tabs.iter().any(|t| t.id == id) {
                    return Err(t!("标签不存在").into());
                }
                if let Some(mut expect) = expect {
                    held.sort();
                    expect.sort();
                    if held != expect {
                        return Err(t!("这个标签里的画面刚变了，再确认一次要关掉哪些").into());
                    }
                }
                self.tabs.retain(|t| t.id != id);
            }
            Edit::ClosePane { terminal } => {
                if !self.take_pane(&terminal) {
                    return Err(t!("画面不存在").into());
                }
            }
            Edit::SplitPane {
                target,
                terminal,
                axis,
                before,
                share,
                ..
            } => {
                if !(1..10000).contains(&share) {
                    return Err(t!("分屏比例必须在 0 与 1 之间").into());
                }
                if !self.terminals.iter().any(|t| t.id == terminal) {
                    return Err(t!("终端不存在").into());
                }
                if self.terminal_tab(&terminal).is_some() {
                    return Err(t!("终端已经摆在别的画面里").into());
                }
                let placed = self
                    .pane_layout(&target)?
                    .split_beside(&target, terminal, axis, before, share);
                if !placed {
                    return Err(t!("画面不存在").into());
                }
            }
            Edit::MovePane {
                terminal,
                target,
                side,
            } => {
                if !self.terminals.iter().any(|t| t.id == terminal) {
                    return Err(t!("终端不存在").into());
                }
                let together = self.terminal_tab(&target).map(|t| &t.id)
                    == self.terminal_tab(&terminal).map(|t| &t.id);
                if !together {
                    let Some((axis, before)) = side else {
                        return Err(t!("两个画面不在同一个标签里，不能互换").into());
                    };
                    self.pane_layout(&target)?;
                    self.take_pane(&terminal);
                    self.pane_layout(&target)?
                        .split_beside(&target, terminal, axis, before, 5000);
                    return Ok(());
                }
                let layout = self.pane_layout(&target)?;
                // Already there: a resend, or two hands making the same move.
                if let Some(moved) = layout.dropped(&terminal, &target, side) {
                    *layout = moved;
                }
            }
            Edit::ResizeDivider {
                tab,
                path,
                axis,
                anchors,
                ratio,
            } => {
                if !(1..10000).contains(&ratio) {
                    return Err(t!("分屏比例必须在 0 与 1 之间").into());
                }
                let node = (self.tab_mut(&tab)?.layout.as_mut())
                    .and_then(|l| l.at_mut(&path))
                    .filter(|l| l.is_divider(axis, &anchors));
                let Some(Layout::Split { ratio: current, .. }) = node else {
                    return Err(t!("这条分隔线刚被别处改掉了，再拖一次").into());
                };
                *current = ratio;
            }
            Edit::ReplacePane { old, new } => {
                if !self.terminals.iter().any(|t| t.id == new) {
                    return Err(t!("终端不存在").into());
                }
                if self.terminal_tab(&new).is_some() {
                    return Err(t!("终端已经摆在别的画面里").into());
                }
                if !self.pane_layout(&old)?.replace(&old, &new) {
                    return Err(t!("画面不存在").into());
                }
            }
            _ => unreachable!("not a tab edit"),
        }
        Ok(())
    }
    fn tab_mut(&mut self, id: &str) -> Result<&mut Tab, String> {
        self.tabs
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| t!("标签不存在").into())
    }
    /// The layout of the tab that shows `terminal`.
    fn pane_layout(&mut self, terminal: &str) -> Result<&mut Layout, String> {
        self.tabs
            .iter_mut()
            .filter_map(|t| t.layout.as_mut())
            .find(|l| l.contains(terminal))
            .ok_or_else(|| t!("画面不存在").into())
    }
    /// Takes `terminal` out of the tab showing it, which closes with its last pane.
    /// False if no tab showed it.
    fn take_pane(&mut self, terminal: &str) -> bool {
        let Some(tab) = (self.tabs.iter_mut())
            .find(|t| t.layout.as_ref().is_some_and(|l| l.contains(terminal)))
        else {
            return false;
        };
        tab.layout = tab.layout.take().and_then(|l| l.without(terminal));
        self.tabs.retain(|t| t.layout.is_some());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Axis;

    fn workspace(terminals: &[&str]) -> Workspace {
        let mut w = Workspace::default();
        w.spaces.push(Space {
            id: "s".into(),
            name: "s".into(),
            cwd: "/tmp".into(),
        });
        for id in terminals {
            w.terminals.push(Terminal {
                id: (*id).into(),
                space: "s".into(),
                argv: vec!["/bin/sh".into()],
                cwd: "/tmp".into(),
                pid: Some(1),
                state: ProcessState::Running,
                exit_code: None,
                error: None,
            });
        }
        w
    }
    fn tab(id: &str, layout: Layout) -> Edit {
        Edit::CreateTab {
            tab: Tab {
                id: id.into(),
                space: "s".into(),
                default_task: None,
                title: String::new(),
                layout: Some(layout),
            },
        }
    }
    fn leaf(id: &str) -> Layout {
        Layout::Leaf(id.into())
    }
    fn split(first: Layout, second: Layout) -> Layout {
        Layout::Split {
            axis: Axis::Right,
            ratio: 5000,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    #[test]
    fn a_queued_resize_must_not_retarget_a_collapsed_sibling_split() {
        let mut w = workspace(&["a", "b", "c"]);
        w.edit(tab("t", split(leaf("a"), split(leaf("b"), leaf("c")))))
            .unwrap();
        // The pointer was released on the outer divider while another request was busy.
        let queued = Edit::ResizeDivider {
            tab: "t".into(),
            path: vec![],
            axis: Axis::Right,
            anchors: ["a".into(), "b".into()],
            ratio: 7000,
        };
        // Another client closes a: the b|c divider now occupies the old root path.
        w.edit(Edit::ClosePane {
            terminal: "a".into(),
        })
        .unwrap();
        let before = w.tabs.clone();
        let reply = w.edit(queued);
        assert!(
            reply.is_err(),
            "stale request accepted on different divider; layout={:?}",
            w.tabs[0].layout
        );
        assert_eq!(w.tabs, before);
    }

    /// The acceptance of step D: what each hand did stays, in any order.
    #[test]
    fn an_agents_split_a_rename_and_a_dragged_divider_all_stay() {
        let mut w = workspace(&["a", "b", "agent"]);
        w.edit(tab("t", split(leaf("a"), leaf("b")))).unwrap();
        let agent = Edit::SplitPane {
            target: "b".into(),
            terminal: "agent".into(),
            axis: Axis::Down,
            before: false,
            share: 5000,
            forced: false,
        };
        let rename = Edit::RenameTab {
            id: "t".into(),
            title: "部署".into(),
        };
        let drag = Edit::ResizeDivider {
            tab: "t".into(),
            path: vec![],
            axis: Axis::Right,
            anchors: ["a".into(), "b".into()],
            ratio: 3000,
        };
        for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
            let mut w = w.clone();
            for i in order {
                w.edit([&agent, &rename, &drag][i].clone()).unwrap();
            }
            let t = &w.tabs[0];
            assert_eq!(t.title, "部署");
            assert_eq!(w.tab_terminals("t"), ["a", "b", "agent"]);
            let Some(Layout::Split { ratio, .. }) = &t.layout else {
                panic!()
            };
            assert_eq!(*ratio, 3000, "{order:?}");
        }
        // A divider that is gone, or turned, is not dragged blindly.
        w.edit(agent).unwrap();
        let stale = Edit::ResizeDivider {
            tab: "t".into(),
            path: vec![true],
            axis: Axis::Right,
            anchors: ["a".into(), "b".into()],
            ratio: 3000,
        };
        assert!(w.edit(stale).unwrap_err().contains("再拖一次"));
    }

    #[test]
    fn closing_a_tab_that_gained_a_pane_since_the_user_agreed_is_refused() {
        let mut w = workspace(&["a", "agent"]);
        w.edit(tab("t", leaf("a"))).unwrap();
        let agreed = Some(vec!["a".to_owned()]);
        w.edit(Edit::SplitPane {
            target: "a".into(),
            terminal: "agent".into(),
            axis: Axis::Right,
            before: false,
            share: 5000,
            forced: false,
        })
        .unwrap();
        let close = |expect| Edit::CloseTab {
            id: "t".into(),
            expect,
        };
        assert!(w.edit(close(agreed)).unwrap_err().contains("再确认"));
        w.edit(close(Some(vec!["agent".into(), "a".into()])))
            .unwrap();
        assert!(w.tabs.is_empty());
    }

    #[test]
    fn panes_move_beside_trade_places_and_leave_emptied_tabs_closed() {
        let mut w = workspace(&["a", "b", "c", "fresh"]);
        w.edit(tab("one", split(leaf("a"), leaf("b")))).unwrap();
        w.edit(tab("two", leaf("c"))).unwrap();
        let trade = Edit::MovePane {
            terminal: "a".into(),
            target: "b".into(),
            side: None,
        };
        w.edit(trade).unwrap();
        assert_eq!(w.tab_terminals("one"), ["b", "a"]);
        // From another tab, which closes with its last pane.
        let beside = Edit::MovePane {
            terminal: "c".into(),
            target: "b".into(),
            side: Some((Axis::Down, true)),
        };
        w.edit(beside.clone()).unwrap();
        assert!(w.tabs.iter().all(|t| t.id != "two"));
        assert_eq!(w.tab_terminals("one"), ["c", "b", "a"]);
        let once = w.clone();
        w.edit(beside).unwrap();
        assert_eq!(w.tabs, once.tabs, "the same move again changes nothing");
        // Across tabs there is nothing to trade with.
        w.edit(tab("three", leaf("fresh"))).unwrap();
        let across = Edit::MovePane {
            terminal: "fresh".into(),
            target: "a".into(),
            side: None,
        };
        assert!(w.edit(across).is_err());
        // A new tab takes its panes from where they were; it never overwrites one.
        w.edit(tab("four", leaf("fresh"))).unwrap();
        assert!(w.tabs.iter().all(|t| t.id != "three"));
        assert!(
            w.edit(tab("four", leaf("a")))
                .unwrap_err()
                .contains("已存在")
        );
        w.edit(Edit::ReplacePane {
            old: "fresh".into(),
            new: "fresh".into(),
        })
        .unwrap_err();
    }
}
