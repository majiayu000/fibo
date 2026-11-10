//! Resize from the pressed position, not from a fraction of the whole split:
//! the gutter consumes fixed pixels and can be grabbed anywhere across its width.
use super::*;
use gpui::{Pixels, Point};

pub(in crate::workbench) struct SplitDrag {
    pub(super) tab: String,
    pub(super) path: Vec<bool>,
    pub(super) anchors: [String; 2],
    /// After release, keep the preview until this save's receipt arrives.
    pub(super) saving: Option<RequestId>,
    axis: Axis,
    start: Point<Pixels>,
    start_ratio: u16,
    usable: Pixels,
}

impl SplitDrag {
    pub(super) fn ratio_at(&self, at: Point<Pixels>) -> u16 {
        let delta = match self.axis {
            Axis::Right => at.x - self.start.x,
            Axis::Down => at.y - self.start.y,
        };
        let start = self.start_ratio as f32 / 10000.;
        let fraction = start + delta / self.usable;
        // Keep an existing narrow pane where it was grabbed. It can move inward
        // smoothly, but cannot be made narrower than its starting size.
        (fraction.clamp(0.05_f32.min(start), 0.95_f32.max(start)) * 10000.).round() as u16
    }

    pub(super) fn split<'a>(&self, w: &'a mut Workspace) -> Option<&'a mut Layout> {
        w.tabs
            .iter_mut()
            .find(|t| t.id == self.tab)?
            .layout
            .as_mut()?
            .at_mut(&self.path)
    }

    pub(in crate::workbench) fn preserve_preview(
        &self,
        local: &mut Workspace,
        incoming: &mut Workspace,
    ) -> bool {
        let Some(Layout::Split { ratio, .. }) = self.split(local) else {
            return false;
        };
        let next = self
            .split(incoming)
            .filter(|l| l.is_divider(self.axis, &self.anchors));
        if let Some(Layout::Split {
            ratio: next_ratio, ..
        }) = next
        {
            *next_ratio = *ratio;
            true
        } else {
            false
        }
    }
}

impl Workbench {
    pub(in crate::workbench) fn settle_resize(&mut self, id: Option<&RequestId>) -> bool {
        if self
            .layout
            .drag
            .as_ref()
            .is_some_and(|drag| drag.saving.as_ref().is_some_and(|save| Some(save) == id))
        {
            self.layout.drag = None;
            true
        } else {
            false
        }
    }

    pub(super) fn begin_resize(&mut self, path: Vec<bool>, start: Point<Pixels>) {
        let Some(tab) = self.nav.tab.clone() else {
            return;
        };
        let Some(bounds) = self.layout.split_bounds.get(&path) else {
            return;
        };
        let Some(split @ Layout::Split { .. }) = self
            .workspace
            .tabs
            .iter_mut()
            .find(|t| t.id == tab)
            .and_then(|t| t.layout.as_mut()?.at_mut(&path))
        else {
            return;
        };
        let anchors = split.divider_anchors().unwrap();
        let Layout::Split { axis, ratio, .. } = split else {
            unreachable!()
        };
        let usable = match axis {
            Axis::Right => bounds.size.width,
            Axis::Down => bounds.size.height,
        } - px(T::S8);
        if usable > px(0.) {
            self.layout.drag = Some(SplitDrag {
                tab,
                path,
                anchors,
                axis: *axis,
                start,
                saving: None,
                start_ratio: *ratio,
                usable,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE_SPAN: f32 = 1000.;
    const FIXTURE_LEFT: f32 = 100.;
    const FIXTURE_TOP: f32 = 200.;

    #[test]
    fn split_drag_keeps_parent_preview_when_a_child_ratio_changes() {
        let drag = SplitDrag {
            tab: "tab".into(),
            path: vec![],
            anchors: ["a".into(), "b".into()],
            saving: None,
            axis: Axis::Right,
            start: Point::default(),
            start_ratio: 4000,
            usable: px(FIXTURE_SPAN),
        };
        let child = Layout::Split {
            axis: Axis::Down,
            ratio: 8000,
            first: Box::new(Layout::Leaf("b".into())),
            second: Box::new(Layout::Leaf("c".into())),
        };
        let mut local = Workspace::default();
        local.tabs.push(Tab {
            id: "tab".into(),
            space: "space".into(),
            default_task: None,
            title: "test".into(),
            layout: Some(Layout::Split {
                axis: Axis::Right,
                ratio: 4000,
                first: Box::new(Layout::Leaf("a".into())),
                second: Box::new(child),
            }),
        });
        let mut incoming = local.clone();
        if let Some(Layout::Split { ratio, .. }) = drag.split(&mut local) {
            *ratio = 6000;
        }
        if let Some(Layout::Split { ratio, .. }) =
            incoming.tabs[0].layout.as_mut().unwrap().at_mut(&[true])
        {
            *ratio = 7000;
        }
        let kept = drag.preserve_preview(&mut local, &mut incoming);
        assert!(
            kept,
            "same panes and axes, only child ratio 8000 -> 7000: parent drag was cancelled"
        );
        assert!(matches!(
            drag.split(&mut incoming),
            Some(Layout::Split { ratio: 6000, .. })
        ));
        assert!(
            matches!(
                incoming.tabs[0].layout.as_mut().unwrap().at_mut(&[true]),
                Some(Layout::Split { ratio: 7000, .. })
            ),
            "child update must also survive"
        );
    }

    #[test]
    fn split_drag_keeps_existing_extreme_ratios_and_moves_inward_smoothly() {
        for axis in [Axis::Down, Axis::Right] {
            for (start_ratio, outward, inward_ratio) in [(200, -1., 210), (9800, 1., 9790)] {
                let drag = SplitDrag {
                    tab: "tab".into(),
                    path: vec![],
                    anchors: ["a".into(), "b".into()],
                    saving: None,
                    axis,
                    start: Point::default(),
                    start_ratio,
                    usable: px(FIXTURE_SPAN),
                };
                let offset = match axis {
                    Axis::Down => gpui::point(px(0.), px(outward)),
                    Axis::Right => gpui::point(px(outward), px(0.)),
                };
                assert_eq!(
                    drag.ratio_at(Point::default()),
                    start_ratio,
                    "no jump on press"
                );
                assert_eq!(
                    drag.ratio_at(offset),
                    start_ratio,
                    "outward movement must not reverse"
                );
                assert_eq!(
                    drag.ratio_at(Point::default() - offset),
                    inward_ratio,
                    "inward movement must stay smooth"
                );
            }
        }
    }

    #[test]
    fn split_drag_keeps_preview_but_accepts_other_snapshot_changes() {
        let drag = SplitDrag {
            tab: "tab".into(),
            path: vec![],
            anchors: ["one".into(), "two".into()],
            axis: Axis::Down,
            start: Point::default(),
            saving: None,
            start_ratio: 8000,
            usable: px(FIXTURE_SPAN),
        };
        let mut local = Workspace::default();
        local.tabs.push(Tab {
            id: "tab".into(),
            space: "space".into(),
            default_task: None,
            title: "before".into(),
            layout: Some(Layout::Split {
                axis: Axis::Down,
                ratio: 8000,
                first: Box::new(Layout::Leaf("one".into())),
                second: Box::new(Layout::Leaf("two".into())),
            }),
        });
        let mut incoming = local.clone();
        incoming.revision = 2;
        incoming.tabs[0].title = "renamed".into();
        if let Some(Layout::Split { ratio, .. }) = drag.split(&mut local) {
            *ratio = 8200;
        }
        assert!(drag.preserve_preview(&mut local, &mut incoming));
        assert!(matches!(
            drag.split(&mut incoming),
            Some(Layout::Split { ratio: 8200, .. })
        ));
        assert_eq!(incoming.revision, 2);
        assert_eq!(incoming.tabs[0].title, "renamed");
        if let Some(Layout::Split { second, .. }) = drag.split(&mut incoming) {
            **second = Layout::Leaf("replacement".into());
        }
        assert!(!drag.preserve_preview(&mut local, &mut incoming));
        incoming.tabs.clear();
        assert!(!drag.preserve_preview(&mut local, &mut incoming));
    }

    #[test]
    fn split_drag_follows_small_moves_from_either_edge_on_both_axes() {
        for axis in [Axis::Down, Axis::Right] {
            for start_ratio in [500, 2000, 5000, 8000, 9500] {
                for grab in [0., 4., 8.] {
                    let start = gpui::point(px(FIXTURE_LEFT + grab), px(FIXTURE_TOP + grab));
                    let drag = SplitDrag {
                        tab: "tab".into(),
                        path: vec![],
                        anchors: ["a".into(), "b".into()],
                        axis,
                        start,
                        saving: None,
                        start_ratio,
                        usable: px(FIXTURE_SPAN),
                    };
                    assert_eq!(drag.ratio_at(start), start_ratio, "pressing must not jump");
                    for delta in [-20., -1., 0., 1., 20.] {
                        let at = start
                            + match axis {
                                Axis::Down => gpui::point(px(0.), px(delta)),
                                Axis::Right => gpui::point(px(delta), px(0.)),
                            };
                        let expected =
                            (start_ratio as i32 + (delta * 10.) as i32).clamp(500, 9500) as u16;
                        assert_eq!(drag.ratio_at(at), expected);
                    }
                }
            }
        }
    }
}
