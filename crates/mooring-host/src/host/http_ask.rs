//! A CLI's approvals through its own API (`Adapter::answers`), when it has one and this
//! process opened a port. The screen still says that it waits; the API says what about, and
//! takes the answer without guessing at a cursor. Without a port: the screen alone.
use super::*;
use crate::cli_http::{self, Asked};
use mooring_core::agent_answers::Answers;
use mooring_core::content::Content;

/// A process found just after a restart may not be tied to its session yet.
const PID_TRIES: u32 = 5;

impl Host {
    fn answers_of(&self, terminal: &str) -> Option<(&'static Answers, Option<u32>)> {
        let s = self.workspace.current_agent(terminal)?;
        let api = crate::agents::adapter(s.runtime).answers()?;
        let pid = self
            .agent_pids
            .iter()
            .find(|(_, id)| **id == s.id)
            .map(|(p, _)| *p);
        Some((api, pid))
    }
    /// A quiet screen, read: asks the API while it shows a wait, forgets otherwise.
    pub(super) fn http_ask_read(
        &mut self,
        shared: &Shared,
        id: &str,
        waiting: bool,
    ) -> io::Result<()> {
        if self.answers_of(id).is_none() {
            return Ok(());
        }
        if !waiting {
            if self.http_asks.remove(id).is_some() {
                let next = self.workspace.clone();
                self.commit(next)?;
            }
            return Ok(());
        }
        let (weak, id) = (Arc::downgrade(shared), id.to_owned());
        thread::spawn(move || {
            for _ in 0..PID_TRIES {
                let Some(shared) = weak.upgrade() else { return };
                let Ok(found) = lock(&shared).map(|h| h.answers_of(&id)) else {
                    return;
                };
                let Some((api, pid)) = found else { return };
                let Some(pid) = pid else {
                    drop(shared);
                    thread::sleep(Duration::from_secs(1));
                    continue;
                };
                let asked = cli_http::endpoint(api, pid)
                    .and_then(|at| cli_http::first_asked(api, &at).ok().flatten());
                let Ok(mut host) = lock(&shared) else { return };
                let changed = match asked {
                    Some(a) => host.http_asks.insert(id.clone(), a.clone()).as_ref() != Some(&a),
                    None => host.http_asks.remove(&id).is_some(),
                };
                if changed {
                    let next = host.workspace.clone();
                    if let Err(e) = host.commit(next) {
                        eprintln!("cli api: {e}");
                    }
                }
                return;
            }
        });
        Ok(())
    }
    /// What the API said, over the card's approval.
    pub(super) fn overlay_http_ask(&self, terminal: &str, c: &mut Content) {
        if let Some(a) = self.http_asks.get(terminal) {
            c.approval = a.question.clone();
            c.approval_kind = a.kind.clone();
            c.approval_always = a.always.clone();
        }
    }
}
/// Answers through the API, `None` when this CLI has none: the screen's keys then. Asks again
/// first: `question` must still be what it asks, so an answer never lands on a newer one.
pub(super) fn answer(
    shared: &Shared,
    id: &str,
    question: &str,
    allow: bool,
    always: bool,
) -> Option<io::Result<()>> {
    let (api, pid) = lock(shared).ok()?.answers_of(id)?;
    let at = pid.and_then(|pid| cli_http::endpoint(api, pid))?;
    Some((|| {
        let gone = || io::Error::other("画面上已经没有这个审批了，可能已经答过");
        let asked: Asked = cli_http::first_asked(api, &at)?.ok_or_else(gone)?;
        if asked.question != question {
            return Err(gone());
        }
        if always && asked.always.is_empty() {
            return Err(io::Error::other("这个审批没有「不再询问」这一项"));
        }
        cli_http::reply(api, &at, &asked.id, allow, always)?;
        let mut host = lock(shared)?;
        host.http_asks.remove(id);
        host.set_screen_wait(id, None)
    })())
}
