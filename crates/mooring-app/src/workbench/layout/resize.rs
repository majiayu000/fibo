//! Resize from the pressed position, not from a fraction of the whole split:
//! the gutter consumes fixed pixels and can be grabbed anywhere across its width.
use super::*;
use gpui::{Pixels, Point};

pub(in crate::workbench) struct SplitDrag {
    pub(super) tab: String,
    pub(super) path: Vec<bool>,
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
        let fraction = self.start_ratio as f32 / 10000. + delta / self.usable;
        (fraction.clamp(0.05, 0.95) * 10000.).round() as u16
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
        match (self.split(local), self.split(incoming)) {
            (
                Some(Layout::Split {
                    axis,
                    ratio,
                    first,
                    second,
                }),
                Some(Layout::Split {
                    axis: next_axis,
                    ratio: next_ratio,
                    first: next_first,
                    second: next_second,
                }),
            ) if axis == next_axis && first == next_first && second == next_second => {
                *next_ratio = *ratio;
                true
            }
            // A closed/rearranged pane is no longer the split that was pressed.
            _ => false,
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
        let Some(Layout::Split { axis, ratio, .. }) = self
            .workspace
            .tabs
            .iter_mut()
            .find(|t| t.id == tab)
            .and_then(|t| t.layout.as_mut()?.at_mut(&path))
        else {
            return;
        };
        let usable = match axis {
            Axis::Right => bounds.size.width,
            Axis::Down => bounds.size.height,
        } - px(T::S8);
        if usable > px(0.) {
            self.layout.drag = Some(SplitDrag {
                tab,
                path,
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
    fn split_drag_keeps_preview_but_accepts_other_snapshot_changes() {
        let drag = SplitDrag {
            tab: "tab".into(),
            path: vec![],
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
