//! Starting an Agent in the shell and placing a todo's text in its input box.
use super::state::{Handoff, Stage};
use super::{HandoffState, Pane};
use gpui::Context;
use mooring_core::terminal::libghostty_vt::terminal::Mode;
use mooring_core::terminal::{agent_input_ready, command_missing};
use std::time::Instant;

const HANDOFF_LIMIT: u64 = 60;
/// Long enough for an Agent's paste handling to finish before Enter arrives.
const SUBMIT_AFTER: std::time::Duration = std::time::Duration::from_millis(300);
impl Pane {
    /// Launch an Agent in this shell, then send `text` to the Agent as its first ask.
    /// Nothing is ever typed into the shell but the command.
    pub fn start_agent(&mut self, command: &str, text: Option<String>, cx: &mut Context<Self>) {
        self.handoff = Some(Handoff {
            command: Some(command.into()),
            text,
            stage: Stage::Shell,
            submit: true,
            started: Instant::now(),
            released: false,
            tail: String::new(),
            still: 0,
        });
        self.watch_handoff(cx);
    }
    /// The Agent was started by hand and may be in the middle of something: the text goes
    /// into its input box and waits for the user's Enter.
    pub fn place_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.handoff = Some(Handoff {
            command: None,
            text: Some(text),
            stage: Stage::Agent,
            submit: false,
            started: Instant::now(),
            released: true,
            tail: String::new(),
            still: 0,
        });
        self.watch_handoff(cx);
    }
    pub fn retry_handoff(&mut self, cx: &mut Context<Self>) {
        if let Some(h) = &mut self.handoff
            && matches!(h.stage, Stage::NotReady | Stage::Missing)
        {
            // After a missing command the user installs it, so run the command again.
            if h.stage == Stage::Missing {
                h.stage = Stage::Shell;
                h.released = false;
            } else {
                h.stage = Stage::Agent;
                h.released = true;
            }
            h.started = Instant::now();
            h.still = 0;
            self.watch_handoff(cx);
        }
    }
    pub fn dismiss_handoff(&mut self, cx: &mut Context<Self>) {
        self.handoff = None;
        cx.notify();
    }
    /// What the bar above this pane says, or None when there is nothing to report.
    pub fn handoff_state(&self) -> Option<HandoffState> {
        let h = self.handoff.as_ref()?;
        Some(match h.stage {
            Stage::Shell | Stage::Agent => HandoffState::Waiting,
            Stage::Placed if h.submit => HandoffState::Sending,
            Stage::Placed => HandoffState::Placed,
            Stage::NotReady => HandoffState::NotReady,
            Stage::Missing => HandoffState::Missing,
        })
    }
    // A short-lived 250 ms check that ends with the handoff; idle panes keep no timer.
    fn watch_handoff(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let busy = this
                    .update(cx, |pane, cx| {
                        pane.step_handoff(cx);
                        pane.handoff
                            .as_ref()
                            .is_some_and(|h| matches!(h.stage, Stage::Shell | Stage::Agent))
                    })
                    .unwrap_or(false);
                if !busy {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    fn step_handoff(&mut self, cx: &mut Context<Self>) {
        let Some(h) = self.handoff.as_mut() else {
            return;
        };
        if !matches!(h.stage, Stage::Shell | Stage::Agent) {
            return;
        }
        let (bracketed, tail) = {
            let Ok(state) = self.connection.state.lock() else {
                return;
            };
            if !state.writable || state.ended {
                return;
            }
            let Some(vt) = state.vt.as_ref() else {
                return;
            };
            (
                vt.terminal().mode(Mode::BRACKETED_PASTE).unwrap_or(false),
                // The whole viewport, minus blank rows under the content: a tall pane
                // with a shell at the top would otherwise show only blank rows here.
                vt.tail_text(usize::from(self.requested_size.1.max(40)))
                    .unwrap_or_default()
                    .trim_end()
                    .to_owned(),
            )
        };
        if tail == h.tail {
            h.still = h.still.saturating_add(1);
        } else {
            h.tail = tail;
            h.still = 0;
        }
        let elapsed = h.started.elapsed();
        match h.stage {
            Stage::Shell => {
                // A line editor that offers bracketed paste is ready; plain shells get 2 s.
                let ready = bracketed || (h.still >= 2 && elapsed.as_secs() >= 2);
                if ready {
                    let command = h.command.clone().unwrap_or_default();
                    h.stage = if h.text.is_some() {
                        Stage::Agent
                    } else {
                        Stage::Placed
                    };
                    h.started = Instant::now();
                    h.still = 0;
                    let mut bytes = command.into_bytes();
                    bytes.push(b'\r');
                    self.input(bytes, cx);
                    if self
                        .handoff
                        .as_ref()
                        .is_some_and(|h| h.stage == Stage::Placed)
                    {
                        // A resume has no text to place; the bar has nothing more to say.
                        self.handoff = None;
                    }
                } else if elapsed.as_secs() >= HANDOFF_LIMIT {
                    h.stage = Stage::NotReady;
                }
            }
            Stage::Agent => {
                // The shell drops bracketed paste when it runs the command; the Agent turns
                // it back on even for onboarding pages. Require a visible input prompt too;
                // a menu or a security notice is not ready to receive task text.
                if !bracketed {
                    h.released = true;
                }
                // Back at the shell prompt, bracketed paste returns too: without this
                // check the task text would land on the shell's command line.
                if command_missing(&h.tail) {
                    h.stage = Stage::Missing;
                } else if h.released && bracketed && h.still >= 2 && agent_input_ready(&h.tail) {
                    let text = h.text.clone().unwrap_or_default();
                    let submit = h.submit;
                    h.stage = Stage::Placed;
                    let placed = self.prefill(&text, cx);
                    match (placed, submit) {
                        (true, true) => self.submit_handoff(cx),
                        (true, false) => (),
                        (false, _) => {
                            if let Some(h) = self.handoff.as_mut() {
                                h.stage = Stage::NotReady;
                            }
                        }
                    }
                } else if elapsed.as_secs() >= HANDOFF_LIMIT {
                    h.stage = Stage::NotReady;
                }
            }
            Stage::Placed | Stage::NotReady | Stage::Missing => (),
        }
        cx.notify();
    }
    /// The Enter after the paste, as its own write once the Agent has taken the paste in:
    /// inside the paste it would be a newline. Dismissing the bar first cancels it.
    fn submit_handoff(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SUBMIT_AFTER).await;
            let _ = this.update(cx, |pane, cx| {
                if pane
                    .handoff
                    .as_ref()
                    .is_some_and(|h| h.submit && h.stage == Stage::Placed)
                {
                    pane.input(b"\r".to_vec(), cx);
                    pane.handoff = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// Whether the text went out.
    pub fn prefill(&mut self, command: &str, cx: &mut Context<Self>) -> bool {
        let bracketed = self.bracketed();
        if !bracketed && (command.contains('\n') || command.contains('\r')) {
            self.local_error = Some("终端尚未启用安全粘贴；请等待 shell 就绪后重试".into());
            cx.notify();
            return false;
        }
        if command.contains('\x1b') {
            self.local_error = Some("待填入文本包含终端控制字符".into());
            cx.notify();
            return false;
        }
        let bytes = if bracketed {
            format!("\x1b[200~{command}\x1b[201~").into_bytes()
        } else {
            command.as_bytes().to_vec()
        };
        self.input(bytes, cx);
        true
    }
}
#[cfg(test)]
#[path = "pane_handoff_tests.rs"]
mod handoff_tests;
