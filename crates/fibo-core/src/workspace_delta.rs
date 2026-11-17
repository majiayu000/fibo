//! A workspace reply as what changed since one the client holds (`Request::Brief`).
//! Almost all of a workspace is its sessions and their content; over a slow link to
//! another machine, sending all of it on every change kept the link full and typing
//! behind it. Sessions and content go only when they changed. The rest goes whole for
//! `Brief`; for `BriefAll` only its fields that changed, since on a machine with hundreds
//! of terminals it was a few hundred KB of the same every time. Either way a window
//! applies it to the workspace as the host sent it, not to the copy it edits itself (a
//! divider drag's preview), so such an edit never outlives the next reply.
use crate::content::Content;
use crate::model::{AgentSession, Workspace};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    /// Names the sessions and content this leaves the client with.
    pub stamp: u64,
    /// The stamp this was taken against; None: every session and all content is here.
    pub base: Option<u64>,
    /// The workspace without sessions and content, as its fields; with `fields`, only
    /// those that changed since the base.
    pub rest: Map<String, Value>,
    /// Hosts before `BriefAll` always sent the whole rest.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fields: bool,
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
    rest: FxHashMap<String, u64>,
}

/// What a client keeps of a workspace it got, to apply the next delta to.
#[derive(Debug, Clone)]
pub struct Kept {
    pub workspace: Workspace,
    rest: Map<String, Value>,
}

/// Content hashes worked out before, by session, with the content each is of. Hashing
/// writes every content out; with a dozen sessions running, the host did that for each
/// refresh of each client, the most of its CPU (11-17). Comparing to the last one is far
/// cheaper, and between two refreshes almost all content is the same.
#[derive(Debug, Default)]
pub struct Hashes {
    content: FxHashMap<String, (Content, u64)>,
}

impl Hashes {
    fn content(&mut self, id: &str, c: &Content) -> u64 {
        if let Some((seen, h)) = self.content.get(id)
            && seen == c
        {
            return *h;
        }
        let h = hash(c);
        self.content.insert(id.to_owned(), (c.clone(), h));
        h
    }
}

/// `w` against `base`, or whole without one, with only the fields of the rest that
/// changed when `fields`; and what to remember of `w`.
pub fn diff(
    w: Workspace,
    base: Option<&Index>,
    fields: bool,
) -> serde_json::Result<(Delta, Index)> {
    diff_with(w, base, fields, &mut Hashes::default())
}

/// `diff`, reusing the hashes of content that has not changed since `hashes` last saw it.
pub fn diff_with(
    mut w: Workspace,
    base: Option<&Index>,
    fields: bool,
    hashes: &mut Hashes,
) -> serde_json::Result<(Delta, Index)> {
    let sessions = std::mem::take(&mut w.agent_sessions);
    let content = std::mem::take(&mut w.content);
    let Value::Object(rest) = serde_json::to_value(&w)? else {
        return Err(serde::ser::Error::custom("a workspace is not an object"));
    };
    let mut rest_hashes: Vec<(&String, u64)> = rest.iter().map(|(k, v)| (k, hash(v))).collect();
    rest_hashes.sort_unstable();
    let session_hashes: Vec<u64> = sessions.iter().map(hash).collect();
    let order = hash(&sessions.iter().map(|s| &s.id).collect::<Vec<_>>());
    let mut content_hashes: Vec<(&String, u64)> = (content.iter())
        .map(|(id, c)| (id, hashes.content(id, c)))
        .collect();
    hashes.content.retain(|id, _| content.contains_key(id));
    content_hashes.sort_unstable();
    let mut stamp = std::hash::DefaultHasher::new();
    (order, &session_hashes, &content_hashes, &rest_hashes).hash(&mut stamp);
    let index = Index {
        stamp: stamp.finish(),
        order,
        sessions: (sessions.iter().map(|s| s.id.clone()))
            .zip(session_hashes.iter().copied())
            .collect(),
        content: (content_hashes.iter().map(|(id, h)| ((*id).clone(), *h))).collect(),
        rest: (rest_hashes.iter().map(|(k, h)| ((*k).clone(), *h))).collect(),
    };
    // Sessions are matched by id: with one twice, only the whole list says which is which.
    let base = base.filter(|_| index.sessions.len() == sessions.len());
    let Some(base) = base else {
        let delta = Delta {
            stamp: index.stamp,
            base: None,
            rest,
            fields: false,
            sessions,
            order: None,
            content,
            gone: vec![],
        };
        return Ok((delta, index));
    };
    let delta = Delta {
        stamp: index.stamp,
        base: Some(base.stamp),
        rest: match fields {
            true => (rest.into_iter())
                .filter(|(k, _)| base.rest.get(k) != index.rest.get(k))
                .collect(),
            false => rest,
        },
        fields,
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
    Ok((delta, index))
}

/// The workspace `delta` describes, given the one it was taken against.
pub fn apply(delta: Delta, base: Option<&Kept>) -> Result<Kept, String> {
    let Delta {
        mut rest,
        fields,
        sessions,
        order,
        content,
        gone,
        base: from,
        ..
    } = delta;
    let base = match (from, base) {
        (Some(_), None) => return Err("workspace delta without its base".into()),
        (Some(_), Some(base)) => Some(base),
        (None, _) => None,
    };
    // Every field of a workspace is written, empty or not: one left out is unchanged.
    if let Some(base) = base.filter(|_| fields) {
        let changed = std::mem::replace(&mut rest, base.rest.clone());
        rest.extend(changed);
    }
    let mut w: Workspace =
        serde_json::from_value(Value::Object(rest.clone())).map_err(|e| e.to_string())?;
    let Some(Kept {
        workspace: base, ..
    }) = base
    else {
        w.agent_sessions = sessions;
        w.content = content;
        return Ok(Kept { workspace: w, rest });
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
    Ok(Kept { workspace: w, rest })
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
