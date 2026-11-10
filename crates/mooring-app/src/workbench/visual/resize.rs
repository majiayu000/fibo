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
        initial: u16,
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
            ratio: initial,
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
        let mut tab = self.workspace.tabs[0].clone();
        tab.id = super::super::id();
        tab.layout = Some(tree);
        self.nav.tab = Some(tab.id.clone());
        self.nav.space = tab.space.clone();
        let Reply::Workspace(w) = rpc(
            self.store.socket(),
            &Request::Edit {
                request: None,
                edit: Edit::CreateTab { tab },
            },
        )
        .expect("create fixture split") else {
            panic!("fixture split refused")
        };
        self.workspace = w;
        self.nav.overview = false;
        self.nav.zoomed = None;
        self.sync_panes(cx);
        cx.notify();
        path
    }

    pub fn visual_resize_geometry(
        &mut self,
        path: &[bool],
        axis: Axis,
        grab: f32,
    ) -> (Point<Pixels>, Pixels) {
        let share = self.visual_resize_ratio(path) as f32 / 10000.;
        let bounds = self.layout.split_bounds[path];
        let usable = match axis {
            Axis::Down => bounds.size.height,
            Axis::Right => bounds.size.width,
        } - px(T::S8);
        let from = match axis {
            Axis::Down => gpui::point(bounds.center().x, bounds.top() + usable * share + px(grab)),
            Axis::Right => {
                gpui::point(bounds.left() + usable * share + px(grab), bounds.center().y)
            }
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

    pub fn visual_resize_snapshot(&mut self, cx: &mut Context<Self>) -> Workspace {
        assert!(self.layout.drag.is_some(), "divider must be grabbed");
        let Reply::Workspace(w) = rpc(
            self.store.socket(),
            &Request::Edit {
                request: None,
                edit: Edit::RenameTab {
                    id: self.nav.tab.clone().unwrap(),
                    title: format!("snapshot-{}", self.workspace.revision),
                },
            },
        )
        .unwrap() else {
            panic!("snapshot edit refused")
        };
        self.apply_workspace(w.clone(), cx);
        w
    }

    pub fn visual_resize_child_snapshot(&mut self, cx: &mut Context<Self>) -> Workspace {
        let tab = self.nav.tab.clone().unwrap();
        let child = self
            .workspace
            .tabs
            .iter_mut()
            .find(|t| t.id == tab)
            .unwrap()
            .layout
            .as_mut()
            .unwrap()
            .at_mut(&[true])
            .unwrap();
        let anchors = child.divider_anchors().unwrap();
        let Layout::Split { axis, .. } = child else {
            panic!("child split")
        };
        let Reply::Workspace(w) = rpc(
            self.store.socket(),
            &Request::Edit {
                request: None,
                edit: Edit::ResizeDivider {
                    tab,
                    path: vec![true],
                    axis: *axis,
                    anchors,
                    ratio: 7000,
                },
            },
        )
        .unwrap() else {
            panic!("child resize refused")
        };
        self.apply_workspace(w.clone(), cx);
        w
    }

    pub fn visual_resize_apply(&mut self, w: Workspace, cx: &mut Context<Self>) {
        self.apply_workspace(w, cx);
    }

    pub fn visual_resize_saved(&mut self, path: &[bool], expected: u16) -> bool {
        assert!(self.error.is_none(), "resize failed: {:?}", self.error);
        if self.store.busy() {
            return false;
        }
        assert!(
            self.layout.drag.is_none(),
            "save receipt did not end the resize"
        );
        let Reply::Workspace(mut saved) = rpc(self.store.socket(), &Request::Workspace).unwrap()
        else {
            panic!("read saved split")
        };
        let tab = saved
            .tabs
            .iter_mut()
            .find(|t| Some(&t.id) == self.nav.tab.as_ref())
            .unwrap();
        assert!(
            matches!(tab.layout.as_mut().unwrap().at_mut(path),
            Some(Layout::Split { ratio, .. }) if *ratio == expected),
            "host did not save {expected}"
        );
        assert_eq!(
            self.visual_resize_ratio(path),
            expected,
            "reply changed the preview"
        );
        true
    }
}
