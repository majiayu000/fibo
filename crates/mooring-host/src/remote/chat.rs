//! `chat`: a conversation as chat, and with `tools` what the Agent did before each of its
//! messages (commands run, files changed), for the phone's folded tool line.
use mooring_core::protocol::*;
use mooring_remote::protocol as rp;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// A conversation as chat, named by its terminal or its session id.
pub(super) fn chat(socket: &Path, request: &Value) -> io::Result<Value> {
    let session = match (request["session"].as_str(), request["terminal"].as_str()) {
        (Some(s), _) => s.to_owned(),
        (None, Some(t)) => match rpc(socket, &Request::Workspace)? {
            Reply::Workspace(w) => w
                .current_agent(t)
                .map(|s| s.id.clone())
                .ok_or_else(|| io::Error::other("这个终端里没有 Agent 对话"))?,
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        },
        (None, None) => return Err(io::Error::other("chat 需要 terminal 或 session")),
    };
    let reply = rpc(
        socket,
        &Request::Chat {
            session: session.clone(),
            after: request["after"].as_i64(),
            before: request["before"].as_i64(),
            limit: request["limit"].as_u64().unwrap_or(50).clamp(1, 500) as usize,
            tools: request["tools"].as_bool().unwrap_or(false),
        },
    )?;
    match reply {
        Reply::Chat { messages, more } => Ok(json!(rp::Chat {
            session,
            messages: messages.into_iter().map(message).collect(),
            more,
        })),
        Reply::Error(e) => Err(io::Error::other(e)),
        other => Err(io::Error::other(format!("unexpected reply: {other:?}"))),
    }
}

fn message(m: ChatMessage) -> rp::Message {
    rp::Message {
        seq: m.seq,
        at: m.at,
        role: if m.user {
            rp::Speaker::User
        } else {
            rp::Speaker::Agent
        },
        text: m.text,
        tools: m.tools.map(|t| rp::Tools {
            commands: t.commands,
            files: (t.files.into_iter())
                .map(|(path, added, removed)| rp::FileLines {
                    path,
                    added,
                    removed,
                })
                .collect(),
        }),
    }
}
