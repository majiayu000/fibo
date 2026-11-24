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
    /// Opens a new todo form and hands back its first field, to type into.
    pub fn visual_task_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<crate::ui::field::Field> {
        self.show_form(FormKind::Task, window, cx);
        self.tasks.form.as_ref().expect("form shown").fields[0].clone()
    }
    pub fn visual_close_form(&mut self, cx: &mut Context<Self>) {
        self.tasks.form = None;
        cx.notify();
    }
}
