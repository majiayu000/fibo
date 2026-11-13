//! Scripted steps for todo_demo_evidence: the same methods the buttons and keys call.
use super::*;

impl Workbench {
    pub fn visual_todo(
        &mut self,
        step: &str,
        arg: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> String {
        match step {
            "board" => self.show_overview(cx),
            "leave" => self.leave_overview(cx),
            "new" => self.show_quick_task(window, cx),
            "type" => {
                if let Some(q) = &self.tasks.board_quick {
                    q.field.update(cx, |f, cx| {
                        f.set_text(arg);
                        cx.notify();
                    });
                }
            }
            "compose" | "commit" => {
                use gpui::EntityInputHandler;
                if let Some(q) = &self.tasks.board_quick {
                    q.field.update(cx, |f, cx| {
                        if step == "compose" {
                            f.replace_and_mark_text_in_range(None, arg, None, window, cx);
                        } else {
                            f.replace_text_in_range(None, arg, window, cx);
                        }
                    });
                }
            }
            "enter" => self.save_board_quick(false, window, cx),
            "cmdenter" => self.save_board_quick(true, window, cx),
            "esc" => self.close_board_quick(window, cx),
            "space" => {
                if let Some(q) = &mut self.tasks.board_quick {
                    q.space = arg.to_owned();
                }
            }
            "spacemenu" => {
                if let Some(q) = &mut self.tasks.board_quick {
                    q.choosing_space = true;
                }
            }
            "newspace" => self.begin_board_space(window, cx),
            "savespace" => {
                if let Some((f, _)) =
                    (self.tasks.board_quick.as_ref()).and_then(|q| q.new_space.as_ref())
                {
                    f.update(cx, |f, _| f.set_text(arg));
                }
                self.save_board_space(window, cx);
            }
            "pick" => self.choose_agent(self.workspace.settings.default_agent, cx),
            "start" => self.ask_agent(arg.to_owned(), false, cx),
            "done" => self.complete_todo(arg, cx),
            "undo" => self.set_todo_outcome(arg, Outcome::Open, cx),
            "filter" => self.overview.filter_space = (!arg.is_empty()).then(|| arg.to_owned()),
            "scrolltodos" => {
                if let Some(c) = self.overview.columns.borrow().get(&Column::Todo) {
                    c.state.scroll_to(gpui::ListOffset {
                        item_ix: arg.parse().unwrap(),
                        offset_in_item: px(0.),
                    });
                }
            }
            "drawertask" => {
                self.overview.drawer = self.task_sessions(arg).first().map(|s| s.id.clone());
            }
            "detail" => self.nav.task_detail = Some(arg.to_owned()),
            "closedetail" => self.nav.task_detail = None,
            "hover" => self.hover_task(arg, true, cx),
            "unhover" => self.hover_task(arg, false, cx),
            _ => panic!("unknown step {step}"),
        }
        cx.notify();
        let quick = self.tasks.board_quick.as_ref();
        let todos: Vec<String> = fibo_core::status::unfinished_todos(&self.workspace, now())
            .into_iter()
            .map(|t| format!("{}:{}", t.title, self.task_sessions(&t.id).len()))
            .collect();
        format!(
            "drawer={} board_focus={} choosing={} naming={} filter={:?} toast={:?} top={} fresh={} overview={} detail={:?} quick={} focused={} text={:?} space={:?} draft={:?} picking={:?} tabs={} todos={todos:?}",
            self.overview.drawer.is_some(),
            self.overview.focus.is_focused(window),
            quick.is_some_and(|q| q.choosing_space),
            quick.is_some_and(|q| q.new_space.is_some()),
            self.overview.filter_space,
            self.overview.toast.as_ref().map(|t| t.text.clone()),
            (self.overview.columns.borrow().get(&Column::Todo))
                .map_or(0, |c| c.state.logical_scroll_top().item_ix),
            self.overview.fresh_todo.is_some(),
            self.nav.overview,
            self.nav.task_detail,
            quick.is_some(),
            quick.is_some_and(|q| q.field.focus_handle(cx).is_focused(window)),
            quick.map(|q| q.field.read(cx).text.clone()),
            quick
                .and_then(|q| (self.workspace.spaces.iter()).find(|s| s.id == q.space))
                .map(|s| s.name.clone()),
            self.tasks.board_draft,
            self.tasks.picking.as_ref().map(|p| p.0.clone()),
            self.workspace.tabs.len(),
        )
    }
}
