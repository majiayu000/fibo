//! A workspace's fields besides its sessions and content, field by field: hashed only when
//! one changed since the last reply, written out only when it goes, and applied in a
//! window only where it changed. Writing all of them out as JSON for each refresh, hashing
//! that and dropping it, and reading all of it back in the window, was most of a refresh
//! once content was shared (11-19: 831 terminals on a long-used install).
use super::{Workspace, hash};
use serde_json::{Map, Value};

/// Every field but `agent_sessions` and `content`, in the order a workspace writes them.
macro_rules! rest_fields {
    ($m:ident) => {
        $m! {
            revision,
            last_left,
            settings,
            outcome_history,
            task_outcomes,
            task_logs,
            pending_tasks,
            lost_agents,
            agent_launch,
            agent_starts,
            quiet_spaces,
            space_last_cwd,
            pinned_terminals,
            spaces,
            tasks,
            tabs,
            terminals
        }
    };
}

macro_rules! define {
    ($($f:ident),*) => {
        /// Fails to build when the workspace gains a field the list above does not name.
        #[allow(dead_code)]
        fn named(w: Workspace) {
            let Workspace { agent_sessions: _, content: _, $($f: _),* } = w;
        }

        /// The fields as last hashed, with their hashes.
        #[derive(Debug, Default)]
        pub(super) struct Seen(Option<(Workspace, Vec<u64>)>);

        impl Seen {
            /// Each field's name and hash; a field equal to the last one seen keeps its hash.
            pub(super) fn hashes(&mut self, w: &Workspace) -> Vec<(&'static str, u64)> {
                let mut out = Vec::new();
                match &mut self.0 {
                    Some((last, hashes)) => {
                        let mut i = 0;
                        $(
                            if last.$f != w.$f {
                                hashes[i] = hash(&w.$f);
                                last.$f = w.$f.clone();
                            }
                            out.push((stringify!($f), hashes[i]));
                            i += 1;
                        )*
                        let _ = i;
                    }
                    None => {
                        let mut last = Workspace::default();
                        let mut hashes = Vec::new();
                        $(
                            last.$f = w.$f.clone();
                            hashes.push(hash(&w.$f));
                            out.push((stringify!($f), hash(&w.$f)));
                        )*
                        self.0 = Some((last, hashes));
                    }
                }
                out
            }
        }

        /// The fields named, written out; with `all`, every field and the two that go
        /// separately as empty, as a whole workspace writes them, for a client that reads
        /// the rest as a workspace.
        pub(super) fn written(
            w: &Workspace,
            all: bool,
            wanted: impl Fn(&str) -> bool,
        ) -> serde_json::Result<Map<String, Value>> {
            let mut out = Map::new();
            $(
                if all || wanted(stringify!($f)) {
                    out.insert(stringify!($f).into(), serde_json::to_value(&w.$f)?);
                }
            )*
            if all {
                out.insert("agent_sessions".into(), Value::Array(vec![]));
                out.insert("content".into(), Value::Object(Map::new()));
            }
            Ok(out)
        }

        /// `base`'s fields with those in `changed` read over them; sessions and content
        /// left empty. A name this build does not know is from a newer host: ignored, as
        /// reading the whole rest ignores it.
        pub(super) fn over(base: &Workspace, changed: Map<String, Value>) -> Result<Workspace, String> {
            let mut w = Workspace {
                $($f: base.$f.clone(),)*
                agent_sessions: Vec::new(),
                content: Default::default(),
            };
            for (name, value) in changed {
                match name.as_str() {
                    $(stringify!($f) => {
                        w.$f = serde_json::from_value(value).map_err(|e| format!("{name}: {e}"))?;
                    })*
                    _ => {}
                }
            }
            Ok(w)
        }
    };
}

rest_fields!(define);
