//! The todo detail page, opened and read for the todo flow evidence.
use super::*;

impl Workbench {
    pub fn visual_open_task(&mut self, id: &str, cx: &mut Context<Self>) {
        self.nav.overview = false;
        self.nav.task_detail = Some(id.into());
        cx.notify();
    }
    pub fn visual_task_detail(&self) -> Option<String> {
        self.nav.task_detail.clone()
    }
}
