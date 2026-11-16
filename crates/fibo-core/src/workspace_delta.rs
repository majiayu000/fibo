//! A workspace reply as what changed since one the client holds (`Request::Brief`).
//! Almost all of a workspace is its sessions and their content; over a slow link to
//! another machine, sending all of it on every change kept the link full and typing
//! behind it. Sessions and content go only when they changed; everything else is small
//! and goes whole every time, so a window that edits its own copy of it (a divider
//! drag's preview) never keeps that edit past the next reply.
use crate::content::Content;
use crate::model::{AgentSession, Workspace};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    /// Names the sessions and content this leaves the client with.
    pub stamp: u64,
    /// The stamp this was taken against; None: every session and all content is here.
    pub base: Option<u64>,
    /// The workspace without sessions and content.
    pub rest: Workspace,
    /// With a base, the sessions that are new or changed; without, all of them in order.
    pub sessions: Vec<AgentSession>,
    /// The session ids in order, when they differ from the base's.
    pub order: Option<Vec<String>>,
    pub content: FxHashMap<String, Content>,
    /// Content the base has and this workspace does not.
    pub gone: Vec<String>,
}

/// What the host remembers of a workspace it sent, to tell what changed since.
#[derive(Debug, Clone)]
pub struct Index {
    pub stamp: u64,
    order: u64,
    sessions: FxHashMap<String, u64>,
    content: FxHashMap<String, u64>,
}

/// `w` against `base`, or whole without one; and what to remember of `w`.
pub fn diff(mut w: Workspace, base: Option<&Index>) -> (Delta, Index) {
    let sessions = std::mem::take(&mut w.agent_sessions);
    let content = std::mem::take(&mut w.content);
    let session_hashes: Vec<u64> = sessions.iter().map(hash).collect();
    let order = hash(&sessions.iter().map(|s| &s.id).collect::<Vec<_>>());
    let mut content_hashes: Vec<(&String, u64)> =
        content.iter().map(|(id, c)| (id, hash(c))).collect();
    content_hashes.sort_unstable();
    let mut stamp = std::hash::DefaultHasher::new();
    (order, &session_hashes, &content_hashes).hash(&mut stamp);
    let index = Index {
        stamp: stamp.finish(),
        order,
        sessions: (sessions.iter().map(|s| s.id.clone()))
            .zip(session_hashes.iter().copied())
            .collect(),
        content: (content_hashes.iter().map(|(id, h)| ((*id).clone(), *h))).collect(),
    };
    // Sessions are matched by id: with one twice, only the whole list says which is which.
    let base = base.filter(|_| index.sessions.len() == sessions.len());
    let Some(base) = base else {
        let delta = Delta {
            stamp: index.stamp,
            base: None,
            rest: w,
            sessions,
            order: None,
            content,
            gone: vec![],
        };
        return (delta, index);
    };
    let delta = Delta {
        stamp: index.stamp,
        base: Some(base.stamp),
        rest: w,
        order: (base.order != order).then(|| sessions.iter().map(|s| s.id.clone()).collect()),
        sessions: (sessions.into_iter())
            .filter(|s| base.sessions.get(&s.id) != index.sessions.get(&s.id))
            .collect(),
        gone: (base.content.keys())
            .filter(|id| !index.content.contains_key(*id))
            .cloned()
            .collect(),
        content: (content.into_iter())
            .filter(|(id, _)| base.content.get(id) != index.content.get(id))
            .collect(),
    };
    (delta, index)
}

/// The workspace `delta` describes, given the one it was taken against.
pub fn apply(delta: Delta, base: Option<&Workspace>) -> Result<Workspace, String> {
    let Delta {
        rest: mut w,
        sessions,
        order,
        content,
        gone,
        base: from,
        ..
    } = delta;
    let Some(base) = from.and(base) else {
        if from.is_some() {
            return Err("workspace delta without its base".into());
        }
        w.agent_sessions = sessions;
        w.content = content;
        return Ok(w);
    };
    let mut changed: FxHashMap<String, AgentSession> =
        sessions.into_iter().map(|s| (s.id.clone(), s)).collect();
    let old: FxHashMap<&str, &AgentSession> = base
        .agent_sessions
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();
    let ids = order.unwrap_or_else(|| base.agent_sessions.iter().map(|s| s.id.clone()).collect());
    w.agent_sessions = (ids.into_iter())
        .map(|id| match changed.remove(&id) {
            Some(s) => Ok(s),
            None => (old.get(id.as_str()).map(|s| (*s).clone()))
                .ok_or_else(|| format!("workspace delta names a session it lacks: {id}")),
        })
        .collect::<Result<_, _>>()?;
    w.content = base.content.clone();
    for id in &gone {
        w.content.remove(id);
    }
    w.content.extend(content);
    Ok(w)
}

fn hash(value: &impl Serialize) -> u64 {
    struct Feed(std::hash::DefaultHasher);
    impl std::io::Write for Feed {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.write(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut feed = Feed(std::hash::DefaultHasher::new());
    // A value that cannot be written hashes as what it wrote so far; serde_json fails
    // only on maps with non-string keys, which a workspace does not have.
    let _ = serde_json::to_writer(&mut feed, value);
    feed.0.finish()
}

#[cfg(test)]
#[path = "workspace_delta_tests.rs"]
mod tests;
