//! Accessors for the native divider regression (the harness dispatches real events).
use super::*;
use gpui::{Pixels, Point};

impl Workbench {
    pub fn visual_resize_workspace(&self) -> Workspace {
        self.workspace.clone()
    }

    pub fn visual_resize_setup(
        &mut self,
        original: Workspace,
        axis: Axis,
        nested: bool,
        cx: &mut Context<Self>,
    ) -> Vec<bool> {
        self.workspace = original;
        self.layout.drag = None;
        let ids: Vec<_> = self
            .workspace
            .terminals
            .iter()
            .take(3)
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(ids.len(), 3);
        let split = Layout::Split {
            axis,
            ratio: 8000,
            first: Box::new(Layout::Leaf(ids[0].clone())),
            second: Box::new(Layout::Leaf(ids[1].clone())),
        };
        let path = if nested { vec![true] } else { vec![] };
        let tree = if nested {
            Layout::Split {
                axis: Axis::Right,
                ratio: 4000,
                first: Box::new(Layout::Leaf(ids[2].clone())),
                second: Box::new(split),
            }
        } else {
            split
        };
        let tab = &mut self.workspace.tabs[0];
        tab.layout = Some(tree);
        self.nav.tab = Some(tab.id.clone());
        self.nav.space = tab.space.clone();
        self.nav.overview = false;
        self.nav.zoomed = None;
        self.sync_panes(cx);
        cx.notify();
        path
    }

    pub fn visual_resize_geometry(
        &self,
        path: &[bool],
        axis: Axis,
        grab: f32,
    ) -> (Point<Pixels>, Pixels) {
        let bounds = self.layout.split_bounds[path];
        let usable = match axis {
            Axis::Down => bounds.size.height,
            Axis::Right => bounds.size.width,
        } - px(T::S8);
        let from = match axis {
            Axis::Down => gpui::point(bounds.center().x, bounds.top() + usable * 0.8 + px(grab)),
            Axis::Right => gpui::point(bounds.left() + usable * 0.8 + px(grab), bounds.center().y),
        };
        (from, usable)
    }

    pub fn visual_resize_ratio(&mut self, path: &[bool]) -> u16 {
        let tab = self
            .workspace
            .tabs
            .iter_mut()
            .find(|t| Some(&t.id) == self.nav.tab.as_ref())
            .unwrap();
        match tab.layout.as_mut().unwrap().at_mut(path).unwrap() {
            Layout::Split { ratio, .. } => *ratio,
            _ => panic!("expected split"),
        }
    }

    pub fn visual_resize_snapshot(&mut self, mut incoming: Workspace, cx: &mut Context<Self>) {
        assert!(self.layout.drag.is_some(), "divider must be grabbed");
        incoming.revision = self.workspace.revision + 1;
        self.apply_workspace(incoming, cx);
    }
}
