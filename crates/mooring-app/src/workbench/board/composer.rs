//! 「＋ 新待办」 on the board: a card that opens in the 待办 column, so writing a todo
//! never leaves the board.
//!
//! The card is either closed or open, and every way out closes it:
//! - Enter or 记下: saved; closed, the keyboard back on the board, the new card first.
//! - Cmd-Enter or 开始: the same, and how to start is asked on the new card.
//! - Esc, a click anywhere outside it, or something else taking over (leaving 鸟瞰, a
//!   todo's detail, settings, search): closed, what was written and the space it was for
//!   waiting for the next time.
//! - With no title Enter does nothing, and the card stays for you to write one.
//! - While an input method composes, every key is its own: nothing above happens.
//!
//! Inside it Esc steps back first: the space list folds, the new space's name is dropped.
use super::*;

#[cfg(feature = "visual-test")]
#[path = "composer/demo_visual.rs"]
mod demo_visual;

pub(in crate::workbench) struct BoardQuick {
    pub(in crate::workbench) field: Entity<Field>,
    _events: gpui::Subscription,
    pub(super) space: String,
    pub(super) choosing_space: bool,
    pub(super) new_space: Option<(Entity<Field>, gpui::Subscription)>,
}

/// How long a todo just written stands out, on its card and in the card it came from.
pub(super) const FRESH_FOR: Duration = Duration::from_millis(1800);

pub(in crate::workbench) struct FreshTodo {
    pub(super) id: String,
    /// The list has been brought back to its top, where the new card is.
    pub(super) revealed: std::cell::Cell<bool>,
}

/// Slides a todo just written into place and lets its outline fade.
pub(super) fn fresh_card(card: gpui::Stateful<gpui::Div>, id: &str) -> impl IntoElement + use<> {
    let still = mooring_core::effect::reduce_motion();
    let (from, to) = (T::color(T::link()), T::color(T::card_border()));
    card.with_animation(
        gpui::SharedString::from(format!("fresh-{id}")),
        Animation::new(FRESH_FOR),
        move |d, t| {
            let fade = ((t - 0.35) / 0.65).clamp(0., 1.);
            let d = d.border_color(gpui::Hsla {
                h: from.h + (to.h - from.h) * fade,
                s: from.s + (to.s - from.s) * fade,
                l: from.l + (to.l - from.l) * fade,
                a: from.a + (to.a - from.a) * fade,
            });
            if still {
                return d;
            }
            let rise = gpui::ease_out_quint()((t / 0.2).clamp(0., 1.));
            d.top(px(T::S12 * (1. - rise))).opacity(rise)
        },
    )
}

impl Workbench {
    pub(in crate::workbench) fn open_board_quick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(q) = &self.tasks.board_quick {
            window.focus(&q.field.focus_handle(cx), cx);
            return;
        }
        self.tasks.picking = None;
        self.overview.drawer = None;
        self.overview.card_menu = None;
        let draft = std::mem::take(&mut self.tasks.board_draft);
        let draft_space = self.tasks.board_draft_space.take();
        let field = cx.new(|cx| {
            Field::new(t!("要做什么"), &draft, cx)
                .compact()
                .placeholder(t!("要做什么"))
        });
        let events = cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
            crate::ui::field::FieldEvent::Submit => this.save_board_quick(false, window, cx),
            crate::ui::field::FieldEvent::Cancel => {
                match &mut this.tasks.board_quick {
                    Some(q) if q.choosing_space => q.choosing_space = false,
                    _ => this.close_board_quick(window, cx),
                }
                cx.notify();
            }
            crate::ui::field::FieldEvent::ClickOut => {}
        });
        window.focus(&field.focus_handle(cx), cx);
        let space = (draft_space.filter(|id| self.workspace.spaces.iter().any(|s| &s.id == id)))
            .or_else(|| self.overview.filter_space.clone())
            .unwrap_or_else(|| self.nav.space.clone());
        // Written into a space put aside it would leave the board at once.
        let space = self.attended_or_first(space);
        self.tasks.board_quick = Some(BoardQuick {
            field,
            _events: events,
            space,
            choosing_space: false,
            new_space: None,
        });
        cx.notify();
    }
    /// Closes the card; what was written comes back when it opens again.
    pub(in crate::workbench) fn close_board_quick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stash_board_quick(cx);
        window.focus(&self.overview.focus, cx);
        cx.notify();
    }
    /// The many ways out of the board (ten places set it aside) all end up here: what
    /// belongs to it doesn't wait open behind whatever took over.
    pub(in crate::workbench) fn settle_board_transients(&mut self, cx: &mut Context<Self>) {
        let taken_over = !self.nav.overview
            || self.nav.task_detail.is_some()
            || self.tasks.form.is_some()
            || !self.overview.cleanup.is_empty()
            || self.settings.is_some()
            || self.diagnostics.is_some()
            || self.search.is_open();
        if taken_over {
            self.stash_board_quick(cx);
        }
        if self.overview.was_shown && !self.nav.overview {
            self.tasks.picking = None;
        }
        self.overview.was_shown = self.nav.overview;
    }
    /// Closes the card without taking the keyboard, for when something else has it now.
    pub(in crate::workbench) fn stash_board_quick(&mut self, cx: &mut Context<Self>) {
        if let Some(q) = self.tasks.board_quick.take() {
            self.tasks.board_draft = q.field.read(cx).text.clone();
            self.tasks.board_draft_space =
                (!self.tasks.board_draft.trim().is_empty()).then_some(q.space);
        }
    }
    pub(in crate::workbench) fn save_board_quick(
        &mut self,
        start: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(q) = &self.tasks.board_quick else {
            return;
        };
        let title = q.field.read(cx).text.trim().to_owned();
        if title.is_empty() || q.field.read(cx).composing() {
            return;
        }
        // Filtered away, the card can't say it arrived, so a note does.
        let hidden = self.overview.changed_only
            || (self.overview.filter_space.as_ref()).is_some_and(|id| id != &q.space);
        let toast = hidden.then(|| {
            let space = (self.workspace.spaces.iter()).find(|s| s.id == q.space);
            t!(
                "「{title}」记到了「{space}」，现在的筛选下看不到",
                title,
                space = space.map_or(t!("外部"), |s| s.name.as_str())
            )
        });
        let task = Task {
            id: id(),
            space: q.space.clone(),
            title,
            description: String::new(),
            acceptance: vec![],
            agent: self.workspace.settings.default_agent,
        };
        self.tasks.board_quick = None;
        window.focus(&self.overview.focus, cx);
        self.mark_fresh_todo(task.id.clone(), cx);
        if start {
            // Asks how to start on the card, once the host has it; the filter can't hide it.
            self.tasks.picking = Some((task.id.clone(), false));
            self.overview.filter_space = None;
            self.overview.changed_only = false;
        } else if let Some(text) = toast {
            self.show_toast(text, None, false, cx);
        }
        self.edit(Edit::AddTask(task), cx);
    }
    fn mark_fresh_todo(&mut self, id: String, cx: &mut Context<Self>) {
        self.overview.fresh_todo = Some(FreshTodo {
            id: id.clone(),
            revealed: std::cell::Cell::new(false),
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FRESH_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if (this.overview.fresh_todo.as_ref()).is_some_and(|f| f.id == id) {
                    this.overview.fresh_todo = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn begin_board_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = super::super::sidebar_spaces::new_space_name(&self.workspace.spaces);
        let field = cx.new(|cx| {
            let mut f = Field::new(t!("新空间的名字"), &name, cx).compact();
            f.select_all();
            f
        });
        let events = cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
            crate::ui::field::FieldEvent::Submit => this.save_board_space(window, cx),
            crate::ui::field::FieldEvent::Cancel => this.cancel_board_space(window, cx),
            crate::ui::field::FieldEvent::ClickOut => {}
        });
        window.focus(&field.focus_handle(cx), cx);
        if let Some(q) = &mut self.tasks.board_quick {
            q.choosing_space = false;
            q.new_space = Some((field, events));
        }
        cx.notify();
    }
    /// A space needs only its name here, as in the column: its folder can come later.
    fn save_board_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(q) = &mut self.tasks.board_quick else {
            return;
        };
        let Some((field, _)) = &q.new_space else {
            return;
        };
        let name = field.read(cx).text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let space = id();
        q.space = space.clone();
        q.new_space = None;
        let title = q.field.focus_handle(cx);
        window.focus(&title, cx);
        self.edit(
            Edit::AddSpace(Space {
                id: space,
                name,
                cwd: String::new(),
            }),
            cx,
        );
    }
    fn cancel_board_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(q) = &mut self.tasks.board_quick {
            q.new_space = None;
            let title = q.field.focus_handle(cx);
            window.focus(&title, cx);
        }
        cx.notify();
    }
    pub(super) fn board_quick_view(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let q = self.tasks.board_quick.as_ref()?;
        let small = |id: &'static str, text: String| {
            button(id, text)
                .h(px(PILL + 4.))
                .px(px(T::S8))
                .text_size(px(T::META))
        };
        let space_name = (self.workspace.spaces.iter())
            .find(|s| s.id == q.space)
            .map_or_else(String::new, |s| s.name.clone());
        let mut body = div()
            .id("board-quick")
            .mb(px(CARD_GAP))
            .px(px(T::S12))
            .py(px(T::S10))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(T::BOARD_CARD_GAP))
            .rounded(px(T::CARD_RADIUS))
            .bg(T::color(T::popover()))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::secondary()))
            // A click outside closes the card when it is released, not when pressed: closing
            // moves the cards up under the pointer, and a press on one of them would miss it.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.overview.fold_on_release = false;
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down_out(cx.listener(|this, e: &gpui::MouseDownEvent, _, _| {
                this.overview.fold_on_release = e.button == gpui::MouseButton::Left;
            }))
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if std::mem::take(&mut this.overview.fold_on_release) {
                        this.close_board_quick(window, cx);
                    }
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                // What an input method is composing isn't written yet, and its keys are its
                // own: they neither save half a word nor reach 鸟瞰 to leave it.
                let composing = (this.tasks.board_quick.as_ref()).is_some_and(|q| {
                    q.field.read(cx).composing()
                        || (q.new_space.as_ref()).is_some_and(|(f, _)| f.read(cx).composing())
                });
                if composing {
                    cx.stop_propagation();
                    return;
                }
                let k = &event.keystroke;
                if k.key == "enter" && k.modifiers.secondary() {
                    this.save_board_quick(true, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(q.field.clone())
            .child(
                div().flex().child(
                    button("board-quick-space", "")
                        .aria_label(t!("这条待办记到哪个空间"))
                        .h(px(PILL + 2.))
                        .px(px(T::S8))
                        .gap(px(T::S6))
                        .text_size(px(T::META))
                        .child(mesh(self.space_mesh(&q.space), MARK))
                        .child(space_name)
                        .child(div().text_color(T::color(T::muted())).child("▾"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(q) = &mut this.tasks.board_quick {
                                q.choosing_space = !q.choosing_space;
                            }
                            cx.notify();
                        })),
                ),
            );
        if q.choosing_space {
            let mut chips = div().flex().flex_wrap().gap(px(T::S4));
            let attended =
                (self.workspace.spaces.iter()).filter(|s| !self.workspace.space_quiet(&s.id));
            for space in attended {
                let sid = space.id.clone();
                chips = chips.child(
                    button(
                        gpui::SharedString::from(format!("board-quick-space-{sid}")),
                        space.name.clone(),
                    )
                    .h(px(PILL + 2.))
                    .px(px(T::S8))
                    .text_size(px(T::META))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(q) = &mut this.tasks.board_quick {
                            q.space = sid.clone();
                            q.choosing_space = false;
                            let title = q.field.focus_handle(cx);
                            window.focus(&title, cx);
                        }
                        cx.notify();
                    })),
                );
            }
            body = body.child(
                chips.child(
                    button("board-quick-new-space", t!("＋ 新建空间"))
                        .h(px(PILL + 2.))
                        .px(px(T::S8))
                        .text_size(px(T::META))
                        .text_color(T::color(T::link()))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.begin_board_space(window, cx)),
                        ),
                ),
            );
        }
        if let Some((field, _)) = &q.new_space {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(T::S4))
                    .child(field.clone())
                    .child(
                        div()
                            .text_size(px(T::META))
                            .text_color(T::color(T::muted()))
                            .child(t!("回车建好 · Esc 取消 · 目录以后再设，没设时从 ~ 开始")),
                    ),
            );
        }
        Some(
            body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S4))
                    .child(div().flex_1())
                    .child(small("board-quick-save", t!("记下 ↵").into()).on_click(
                        cx.listener(|this, _, window, cx| this.save_board_quick(false, window, cx)),
                    ))
                    .child(
                        small(
                            "board-quick-start",
                            crate::ui::words::local_field(t!("开始 ⌘↵")),
                        )
                        .bg(T::color(T::accent()))
                        .text_color(T::color(T::on_accent()))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_board_quick(true, window, cx)
                        })),
                    ),
            )
            .into_any_element(),
        )
    }
    /// A todo you marked complete, in 做完了: there may be no session to stand for it.
    pub(super) fn done_todo_card(
        &self,
        task: &Task,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tid = task.id.clone();
        let sessions = self.task_sessions(&task.id).len();
        card_frame(gpui::SharedString::from(format!("board-done-{}", task.id)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S6))
                    .text_size(px(T::META))
                    .text_color(T::color(T::muted()))
                    .child(agent_mark(
                        (self.task_sessions(&task.id).last()).map_or(task.agent, |s| s.runtime),
                        T::color(T::muted()),
                    ))
                    .child({
                        let when = (self.workspace.task_logs.get(&task.id))
                            .and_then(|l| l.closed)
                            .map_or_else(String::new, |at| format!(" · {}", short_ago(at, now())));
                        if sessions > 0 {
                            t!("✓ 你标完成 · {sessions} 个会话{when}", sessions, when)
                        } else {
                            t!("✓ 自己完成{when}", when)
                        }
                    })
                    .child(div().flex_1())
                    .child(mesh(self.space_mesh(&task.space), AVATAR)),
            )
            .child(card_title(task.title.clone()))
            .child(
                div().flex().child(div().flex_1()).child(
                    button(
                        gpui::SharedString::from(format!("todo-undo-{}", task.id)),
                        t!("撤销"),
                    )
                    .h(px(PILL + 4.))
                    .px(px(T::S8))
                    .text_size(px(T::META))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_todo_outcome(&tid, Outcome::Open, cx)
                    })),
                ),
            )
    }
    /// You closed the todo, so what was left open for it goes too: a session that has
    /// stopped can be resumed from its card. One still working, or asking you something,
    /// keeps its terminal: ending it is not what 完成 says, and a note says it stayed.
    pub(in crate::workbench) fn complete_todo(&mut self, task: &str, cx: &mut Context<Self>) {
        if self.tasks.picking.as_ref().is_some_and(|(t, _)| t == task) {
            self.tasks.picking = None;
        }
        self.set_todo_outcome(task, Outcome::Completed, cx);
        let mut terminals: Vec<String> = (self.task_sessions(task).into_iter())
            .filter_map(|s| live_terminal(&self.workspace, s))
            .map(|t| t.id.clone())
            .collect();
        terminals.sort_unstable();
        terminals.dedup();
        let mut kept = 0;
        for terminal in terminals {
            let closing = Closing::Pane(terminal);
            if interrupts(&self.running_agents(&closing)) {
                kept += 1;
            } else {
                self.close_now(closing, cx);
            }
        }
        if kept > 0 {
            self.show_toast(
                t!("待办完成了；还有 {kept} 个 Agent 没停，终端留着", kept),
                None,
                false,
                cx,
            );
        }
    }
    pub(super) fn set_todo_outcome(&mut self, task: &str, to: Outcome, cx: &mut Context<Self>) {
        self.edit(
            Edit::SetTaskOutcome {
                seen: None,
                id: task.to_owned(),
                outcome: to,
            },
            cx,
        );
    }
    pub(super) fn task_sessions(&self, task: &str) -> Vec<&AgentSession> {
        (self.workspace.agent_sessions.iter())
            .filter(|s| s.task.as_deref() == Some(task))
            .collect()
    }
    /// Lights a todo's card and its sessions' together while the pointer is on one.
    pub(super) fn hover_task(&mut self, task: &str, on: bool, cx: &mut Context<Self>) {
        let next = if on {
            Some(task.to_owned())
        } else {
            self.overview.hover_task.take().filter(|t| t != task)
        };
        if self.overview.hover_task != next {
            self.overview.hover_task = next;
            cx.notify();
        }
    }
}
