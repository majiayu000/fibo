//! A user's request from sending to its receipt: one at a time, the rest queued; a
//! dropped link is waited out with the same request; refused or unknown, it says so
//! (docs/request-envelope/spec.md, step C).
use super::*;
use fibo_core::delivery::Settled;
#[cfg(test)]
#[path = "launch_wait_tests.rs"]
mod launch_wait_tests;

impl Workbench {
    /// Marks a user action in flight; the label appears only if it outlasts 300ms.
    pub(super) fn begin_busy(&mut self, request: Option<&Request>, cx: &mut Context<Self>) {
        let seq = self.store.begin_busy(request);
        if let Some(view) = &self.settings {
            view.update(cx, |v, cx| {
                v.busy = true;
                v.save_error = None;
                cx.notify();
            });
        }
        self.error = None;
        self.unsure = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.store.mark_slow(seq) {
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn end_busy(&mut self) {
        self.store.end_busy();
    }
    pub(super) fn request(&mut self, request: Request, cx: &mut Context<Self>) {
        if self.store.busy() {
            self.store.hold(request);
            return;
        }
        self.begin_busy(Some(&request), cx);
        let task = cx.background_spawn(self.store.deliver(request.clone()));
        cx.spawn(async move |this, cx| {
            let settled = task.await;
            let _ = this.update(cx, |this, cx| {
                this.end_busy();
                let id = match &request {
                    Request::Edit { request, .. } => request.clone(),
                    _ => None,
                };
                let confirmed = matches!(settled, Settled::Confirmed(_));
                let confirmed_revision = match &settled {
                    Settled::Confirmed(Reply::Workspace(w)) => Some(w.revision),
                    _ => None,
                };
                let resized = this.settle_resize(id.as_ref(), confirmed_revision);
                let received_workspace =
                    matches!(&settled, Settled::Confirmed(Reply::Workspace(_)));
                if let Some(words) = settled.words() {
                    this.tasks.start_after_save = None;
                    this.error = Some(words);
                    if matches!(settled, Settled::Unknown { .. }) {
                        this.unsure = Some(request);
                    }
                } else if let Settled::Confirmed(Reply::Workspace(w)) = settled {
                    this.apply_workspace(w, cx);
                    if let Some(task) = this.tasks.start_after_save.take() {
                        this.launch(false, Some(task), cx);
                    }
                }
                if resized && !confirmed {
                    this.sync(cx);
                }
                if this.hosts.embedded {
                    cx.emit(super::hosts::HostEvent::Receipt {
                        id: id.clone(),
                        error: this.error.clone(),
                    });
                }
                this.form_settled(id, confirmed);
                if let Some(view) = &this.settings {
                    view.update(cx, |v, cx| {
                        v.values = this.workspace.settings.clone();
                        v.busy = this.store.busy();
                        v.save_error = this.error.clone();
                        cx.notify();
                    });
                }
                if received_workspace && this.store.behind(this.workspace.revision) {
                    this.sync(cx);
                }
                this.run_waiting(cx);
            });
        })
        .detach();
    }
    pub(super) fn remote_receipt(
        &mut self,
        id: Option<&RequestId>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = error {
            self.error = Some(error.to_owned());
        }
        if let Some(index) = self
            .hosts
            .outcomes
            .iter()
            .position(|(pending, _)| Some(pending) == id)
        {
            let (_, text) = self.hosts.outcomes.remove(index);
            self.show_toast(error.unwrap_or(&text).to_owned(), None, error.is_some(), cx);
        }
        if self
            .hosts
            .closing
            .as_ref()
            .is_some_and(|(pending, _)| Some(pending) == id)
        {
            let (_, terminal) = self.hosts.closing.take().unwrap();
            if self.peek.as_ref().is_some_and(|p| p.terminal == terminal) {
                if let Some(error) = error {
                    let peek = self.peek.as_mut().unwrap();
                    peek.answering = false;
                    peek.confirm_end = false;
                    peek.note = Some(error.to_owned());
                } else {
                    self.close_peek(cx);
                }
            }
        }
        cx.notify();
    }
    /// Starts what waited for the action that just ended: the next request, else the
    /// held launch. The ended action's error stays shown.
    pub(super) fn run_waiting(&mut self, cx: &mut Context<Self>) {
        let (error, unsure) = (self.error.take(), self.unsure.take());
        if let Some(next) = self.store.next_queued() {
            self.request(next, cx);
        } else if let Some((split, task, replace)) = self.store.next_launch() {
            self.launch_in_slot(split, task, replace, cx);
        }
        self.error = self.error.take().or(error);
        self.unsure = self.unsure.take().or(unsure);
        cx.notify();
    }
    /// Sends an edit made on what this window shows, named so a resend applies it once:
    /// an outcome change carries the outcome and turn seen here, refused if either moved.
    pub(super) fn edit(&mut self, edit: Edit, cx: &mut Context<Self>) {
        self.edit_named(edit, cx);
    }
    /// `edit`, returning the id its receipt will carry.
    pub(super) fn edit_named(&mut self, mut edit: Edit, cx: &mut Context<Self>) -> RequestId {
        if let Edit::SetOutcome {
            id,
            outcome: Outcome::Completed,
            ..
        } = &edit
        {
            self.offer_session_todos(&id.clone(), cx);
        }
        self.workspace.stamp(&mut edit);
        let id = RequestId::fresh();
        let request = Some(id.clone());
        self.request(Request::Edit { request, edit }, cx);
        id
    }
    /// The open form's save got its receipt: it closes once confirmed and stays, with
    /// what was typed, when refused or unknown.
    fn form_settled(&mut self, id: Option<RequestId>, confirmed: bool) {
        let Some(form) = &mut self.tasks.form else {
            return;
        };
        if id.is_none() || form.sending != id {
            return;
        }
        form.sending = None;
        if confirmed {
            self.close_form();
        }
    }
    pub(super) fn close_form(&mut self) {
        self.tasks.form = None;
        self.tasks.editing_task = None;
    }
    /// The last request's error; when its outcome is unknown, a button sends it again
    /// under the same id, which the host applies at most once.
    pub(super) fn error_strip(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let error = self.error.clone()?;
        let mut strip = div()
            .flex()
            .items_center()
            .gap(px(T::S10))
            .px(px(T::S14))
            .py(px(T::S6))
            .child(label(error).text_color(Tone::Danger.ink()));
        if self.unsure.is_some() {
            strip = strip.child(button("retry-request", t!("重试")).on_click(cx.listener(
                |this, _, _, cx| {
                    if let Some(request) = this.unsure.take() {
                        this.error = None;
                        this.request(request, cx);
                    }
                },
            )));
        }
        // Every error can be put away: a message the user has read should not stay over
        // the work until something else replaces it.
        strip = strip.child(div().flex_1()).child(
            button("error-dismiss", "×")
                .aria_label(t!("关掉这条提示"))
                .w(px(T::ICON_WIDTH))
                .h(px(T::ICON_WIDTH))
                .px_0()
                .text_color(T::color(T::muted()))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.error = None;
                    this.unsure = None;
                    cx.notify();
                })),
        );
        Some(strip)
    }
}
