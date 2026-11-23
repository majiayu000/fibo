//! Outcome changes in the order they happened. Each status read asks for one
//! conversation's last change; walking the whole list backwards for every conversation
//! cost conversations × changes on each commit and redraw (11-24 review), so the last
//! change of each is looked up from an index built once per list.
use super::OutcomeChange;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Written and read as the plain list it always was. Only appended to, through `push`,
/// so the index a copy shares stays right for it until that copy pushes.
#[derive(Clone, Default)]
pub struct History {
    list: Vec<OutcomeChange>,
    index: Arc<OnceLock<Index>>,
}

#[derive(Default)]
struct Index {
    /// Position of each conversation's last change.
    last: HashMap<String, usize>,
    /// Position of each conversation's last 暂存到期.
    last_review: HashMap<String, usize>,
}

const REVIEW: &str = crate::not_ui!("暂存到期");

impl History {
    pub fn push(&mut self, change: OutcomeChange) {
        self.list.push(change);
        self.index = Arc::default();
    }
    fn index(&self) -> &Index {
        self.index.get_or_init(|| {
            let mut index = Index::default();
            for (i, h) in self.list.iter().enumerate() {
                index.last.insert(h.session.clone(), i);
                if h.reason == REVIEW {
                    index.last_review.insert(h.session.clone(), i);
                }
            }
            index
        })
    }
    /// The conversation's last outcome change.
    pub fn last(&self, session: &str) -> Option<&OutcomeChange> {
        self.index().last.get(session).map(|&i| &self.list[i])
    }
    /// When the conversation's put-off time last came.
    pub fn last_review(&self, session: &str) -> Option<u64> {
        self.index()
            .last_review
            .get(session)
            .map(|&i| self.list[i].at)
    }
}

impl std::ops::Deref for History {
    type Target = [OutcomeChange];
    fn deref(&self) -> &[OutcomeChange] {
        &self.list
    }
}
impl From<Vec<OutcomeChange>> for History {
    fn from(list: Vec<OutcomeChange>) -> Self {
        Self {
            list,
            index: Arc::default(),
        }
    }
}
impl PartialEq for History {
    fn eq(&self, other: &Self) -> bool {
        self.list == other.list
    }
}
impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.list.fmt(f)
    }
}
impl Serialize for History {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.list.serialize(s)
    }
}
impl<'de> Deserialize<'de> for History {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::deserialize(d).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Outcome;

    fn change(session: &str, after: Outcome, at: u64, reason: &str) -> OutcomeChange {
        OutcomeChange {
            session: session.into(),
            before: Outcome::Open,
            after,
            at,
            reason: reason.into(),
        }
    }

    #[test]
    fn last_change_follows_pushes_even_on_a_copy_that_shared_the_index() {
        let mut h = History::default();
        h.push(change("a", Outcome::Completed, 1, REVIEW));
        h.push(change("b", Outcome::Open, 2, "新回合"));
        assert_eq!(h.last("a").map(|c| c.at), Some(1));
        let mut copy = h.clone();
        copy.push(change("a", Outcome::Open, 3, "新回合"));
        // The copy sees its own push; the original keeps its index.
        assert_eq!(copy.last("a").map(|c| c.at), Some(3));
        assert_eq!(copy.last_review("a"), Some(1));
        assert_eq!(h.last("a").map(|c| c.at), Some(1));
        assert_eq!(h.last("c"), None);
        assert_eq!(h.last_review("b"), None);
    }

    #[test]
    fn reads_and_writes_the_plain_list_older_builds_wrote() {
        let h: History = vec![change("a", Outcome::Completed, 5, "x")].into();
        let json = serde_json::to_string(&h).unwrap();
        let list: Vec<OutcomeChange> = serde_json::from_str(&json).unwrap();
        assert_eq!(list.len(), 1);
        let back: History = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
        assert_eq!(back.last("a").map(|c| c.at), Some(5));
    }
}
