//! `ask`: what a screen asks of its user, read here so a phone never parses a terminal.
use super::{named, ok_reply, text};
use mooring_core::protocol::*;
use mooring_host::{screen_ask, screen_rules};
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// Rows read for a prompt: a long command stands far above its question.
const ROWS: usize = 400;

fn screen(socket: &Path, terminal: &str) -> io::Result<String> {
    let read = Request::ReadScreen {
        id: terminal.to_owned(),
        rows: ROWS,
    };
    match rpc(socket, &read)? {
        Reply::Text(t) => Ok(t),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

/// What a CLI's own API said it asks, as the host put it on the card. Only the card decides:
/// this process lives on the relay and may have read the plugins before one was updated, and
/// then asking its registry reads the wrong screen, and the phone says nothing was read.
fn asked_over_api(c: &mooring_core::content::Content) -> Option<rp::Ask> {
    (!c.approval.is_empty() && !c.approval_kind.is_empty()).then(|| rp::Ask {
        question: c.approval.clone(),
        kind: c.approval_kind.clone(),
        subject: c.approval.clone(),
        always: (!c.approval_always.is_empty()).then(|| c.approval_always.clone()),
        ..Default::default()
    })
}

fn read_ask(socket: &Path, terminal: &str) -> io::Result<rp::Ask> {
    if let Reply::Workspace(w) = rpc(socket, &Request::Workspace)? {
        let c = w.current_agent(terminal).and_then(|s| w.content.get(&s.id));
        if let Some(ask) = c.and_then(asked_over_api) {
            return Ok(ask);
        }
    }
    let tail = screen(socket, terminal)?;
    // The same reading that made the card's `approval`: a question scrolled up is none.
    let seen = screen_rules::explain(screen_rules::default_rules(), &tail);
    let question = (seen.approval())
        .ok_or_else(|| io::Error::other("画面上现在没有审批"))?
        .to_owned();
    let a = screen_ask::approval_ask(&tail).unwrap_or_default();
    Ok(rp::Ask {
        question,
        kind: a.kind,
        subject: a.subject,
        note: a.note,
        detail: a.detail,
        truncated: a.truncated,
        always: seen
            .always_option()
            .map(|o| screen_ask::always_words(&o.text)),
    })
}

pub(super) fn ask(socket: &Path, terminal: &str) -> io::Result<Value> {
    Ok(json!(read_ask(socket, terminal)?))
}

/// An `answer` carrying the `subject` its client showed answers that one or nothing: the
/// question is the same words for every command, and the Agent may have moved on since. So
/// with the `always_label` its button read: what it grants is what the person agreed to.
pub(super) fn same_subject(socket: &Path, terminal: &str, request: &Value) -> io::Result<()> {
    let (showed, label) = (
        request["subject"].as_str(),
        request["always_label"].as_str(),
    );
    if showed.is_none() && label.is_none() {
        return Ok(());
    }
    let now = read_ask(socket, terminal)?;
    if showed.is_some_and(|s| s != now.subject) {
        return Err(io::Error::other("画面上的审批已经换了一条，请重新看一眼"));
    }
    if label.is_some_and(|l| now.always.as_deref() != Some(l)) {
        return Err(io::Error::other("菜单上这一项已经变了，请重新看一眼"));
    }
    Ok(())
}

/// How long a `trust` watches for the prompt to go.
const TRUST_WAIT: Duration = Duration::from_secs(5);

/// `trust`: answers the 「trust this folder?」 on the screen. The host reads the screen around
/// each key; this replies once the prompt is gone and says so if it is not.
pub(super) fn trust(socket: &Path, request: &Value, client: Option<&str>) -> io::Result<Value> {
    let id = text(request, "terminal")?.to_owned();
    let answer = Request::AnswerTrust {
        id: id.clone(),
        folder: text(request, "folder")?.to_owned(),
        trust: (request["trust"].as_bool())
            .ok_or_else(|| io::Error::other("trust 需要 true 或 false"))?,
    };
    ok_reply(rpc(socket, &named(client, request, answer))?)?;
    let until = Instant::now() + TRUST_WAIT;
    while screen_ask::trust_screen(&screen(socket, &id)?).is_some() {
        if Instant::now() >= until {
            return Err(io::Error::other(
                "按键发了，但画面上还在问，请打开终端看一眼",
            ));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Ok(json!({}))
}

/// Text for an Agent asking this has nowhere good to go: Enter on Claude Code's prompt quits.
pub(super) fn not_asking_trust(socket: &Path, terminal: &str) -> io::Result<()> {
    if screen_ask::trust_screen(&screen(socket, terminal)?).is_some() {
        return Err(io::Error::other(
            "它在等你决定要不要信任这个文件夹，答完才能发消息",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use mooring_core::content::Content;
    /// Undoing this for a registry lookup brings back 「电脑没读出它要做什么」 on a real phone
    /// whenever the plugin was updated after the relay process started (2026-11-07).
    #[test]
    fn an_approval_from_the_api_is_read_off_the_card() {
        let c = Content {
            approval: "echo hi".into(),
            approval_kind: "bash".into(),
            approval_always: "以后都允许 echo *".into(),
            ..Default::default()
        };
        let ask = asked_over_api(&c).unwrap();
        assert_eq!(ask.subject, "echo hi");
        assert_eq!(ask.always.as_deref(), Some("以后都允许 echo *"));
        let from_screen = Content {
            approval: "Do you want to proceed?".into(),
            ..Default::default()
        };
        assert_eq!(asked_over_api(&from_screen), None, "the screen says more");
    }
}
