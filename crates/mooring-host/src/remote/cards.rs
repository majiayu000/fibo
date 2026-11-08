//! `cards`: `list` as what changed since this connection last asked. A phone refreshes on each
//! burst of events, and most of the list has not changed: 25 terminals are 13 KB a time.
//! What was last sent is kept per connection, so one that stops asking costs nothing more.
use serde_json::{json, Map, Value};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Listed {
    rev: u64,
    machine: Value,
    spaces: Value,
    quiet: Value,
    todos: Value,
    order: Vec<String>,
    cards: HashMap<String, Value>,
}

impl Listed {
    /// `full` is what `list` says now. `since` other than the last `rev` gets all of it, so a
    /// client that lost track starts over rather than patching what it does not have.
    pub(super) fn reply(&mut self, mut full: Value, since: Option<u64>) -> Value {
        self.rev += 1;
        let mut out = Map::new();
        out.insert("rev".into(), json!(self.rev));
        let fresh = since != Some(self.rev - 1) || self.rev == 1;
        if fresh {
            self.order.clear();
            self.cards.clear();
        }
        let same = |name: &str, kept: &mut Value, now: Value, out: &mut Map<String, Value>| {
            if fresh || *kept != now {
                out.insert(name.to_owned(), now.clone());
                *kept = now;
            }
        };
        same(
            "machine",
            &mut self.machine,
            full["machine"].take(),
            &mut out,
        );
        same("spaces", &mut self.spaces, full["spaces"].take(), &mut out);
        same("quiet", &mut self.quiet, full["quiet"].take(), &mut out);
        same("todos", &mut self.todos, full["todos"].take(), &mut out);
        let terminals = match full["terminals"].take() {
            Value::Array(all) => all,
            _ => vec![],
        };
        let id = |c: &Value| c["id"].as_str().unwrap_or_default().to_owned();
        let order: Vec<String> = terminals.iter().map(id).collect();
        if fresh || order != self.order {
            out.insert("order".into(), json!(order));
            self.order = order;
        }
        let changed: Vec<Value> = (terminals.iter())
            .filter(|c| self.cards.get(&id(c)) != Some(*c))
            .cloned()
            .collect();
        self.cards = terminals.into_iter().map(|c| (id(&c), c)).collect();
        if !changed.is_empty() {
            out.insert("changed".into(), Value::Array(changed));
        }
        Value::Object(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    fn card(id: &str, doing: &str) -> Value {
        json!({ "id": id, "doing": doing })
    }
    fn full(cards: Vec<Value>) -> Value {
        json!({ "machine": { "name": "m" }, "spaces": [{ "id": "s" }], "todos": [], "terminals": cards })
    }

    /// Replays replies the way the phone does (`mobile/src/remote/cards.ts`).
    fn apply(have: &mut Vec<Value>, reply: &Value) {
        let by_id: HashMap<String, Value> = (have
            .iter()
            .chain(reply["changed"].as_array().map_or(&[][..], |c| &c[..])))
        .map(|c| (c["id"].as_str().unwrap().to_owned(), c.clone()))
        .collect();
        let order: Vec<String> = match reply["order"].as_array() {
            Some(o) => o.iter().map(|i| i.as_str().unwrap().to_owned()).collect(),
            None => have
                .iter()
                .map(|c| c["id"].as_str().unwrap().to_owned())
                .collect(),
        };
        *have = order.iter().map(|i| by_id[i].clone()).collect();
    }

    #[test]
    fn the_first_reply_says_everything_and_later_ones_only_what_changed() {
        let mut l = Listed::default();
        let first = l.reply(full(vec![card("a", "x"), card("b", "y")]), None);
        assert_eq!(first["rev"], 1);
        assert_eq!(first["order"], json!(["a", "b"]));
        assert_eq!(first["changed"].as_array().unwrap().len(), 2);
        assert_eq!(first["machine"]["name"], "m");
        assert_eq!(first["spaces"], json!([{ "id": "s" }]));
        assert_eq!(first["todos"], json!([]));

        let quiet = l.reply(full(vec![card("a", "x"), card("b", "y")]), Some(1));
        assert_eq!(
            quiet,
            json!({ "rev": 2 }),
            "nothing changed: only the version"
        );

        let one = l.reply(full(vec![card("a", "x"), card("b", "z")]), Some(2));
        assert_eq!(one, json!({ "rev": 3, "changed": [card("b", "z")] }));
    }

    #[test]
    fn a_space_put_aside_or_taken_back_says_the_spaces_put_aside() {
        let mut l = Listed::default();
        let with = |quiet: Value| {
            let mut f = full(vec![card("a", "")]);
            f["quiet"] = quiet;
            f
        };
        l.reply(with(json!([])), None);
        let aside = l.reply(with(json!([{ "id": "q" }])), Some(1));
        assert_eq!(aside, json!({ "rev": 2, "quiet": [{ "id": "q" }] }));
        let back = l.reply(with(json!([])), Some(2));
        assert_eq!(back, json!({ "rev": 3, "quiet": [] }));
    }

    #[test]
    fn a_terminal_going_or_moving_says_the_new_order() {
        let mut l = Listed::default();
        l.reply(
            full(vec![card("a", ""), card("b", ""), card("c", "")]),
            None,
        );
        let moved = l.reply(
            full(vec![card("b", ""), card("a", ""), card("c", "")]),
            Some(1),
        );
        assert_eq!(moved, json!({ "rev": 2, "order": ["b", "a", "c"] }));
        let gone = l.reply(full(vec![card("b", ""), card("c", "")]), Some(2));
        assert_eq!(gone, json!({ "rev": 3, "order": ["b", "c"] }));
        let empty = l.reply(full(vec![]), Some(3));
        assert_eq!(empty, json!({ "rev": 4, "order": [] }));
    }

    #[test]
    fn a_change_elsewhere_in_the_list_says_only_that() {
        let mut l = Listed::default();
        l.reply(full(vec![card("a", "")]), None);
        let mut next = full(vec![card("a", "")]);
        next["todos"] = json!([{ "id": "t" }]);
        assert_eq!(
            l.reply(next, Some(1)),
            json!({ "rev": 2, "todos": [{ "id": "t" }] })
        );
        let mut next = full(vec![card("a", "")]);
        next["todos"] = json!([{ "id": "t" }]);
        next["spaces"] = json!([]);
        next["machine"] = json!({ "name": "n" });
        assert_eq!(
            l.reply(next, Some(2)),
            json!({ "rev": 3, "spaces": [], "machine": { "name": "n" } })
        );
    }

    /// A client out of step (a second copy of this phone's request, an answer lost) must not
    /// patch a list it does not have.
    #[test]
    fn a_since_this_connection_did_not_just_give_gets_all_of_it() {
        let mut l = Listed::default();
        l.reply(full(vec![card("a", "")]), None);
        l.reply(full(vec![card("a", "")]), Some(1));
        for stale in [None, Some(1), Some(99), Some(0)] {
            let all = l.reply(full(vec![card("a", "")]), stale);
            assert_eq!(all["order"], json!(["a"]), "{stale:?}");
            assert_eq!(all["changed"], json!([card("a", "")]), "{stale:?}");
            assert_eq!(all["spaces"], json!([{ "id": "s" }]), "{stale:?}");
            assert_eq!(all["todos"], json!([]), "{stale:?}");
        }
        // A new connection's first ask, even with the number an old one was given.
        let mut new = Listed::default();
        assert_eq!(
            new.reply(full(vec![card("a", "")]), Some(0))["order"],
            json!(["a"])
        );
    }

    /// Whatever the steps, what a client rebuilds is what `list` says.
    #[test]
    fn replaying_the_replies_gives_what_list_says() {
        let mut l = Listed::default();
        let mut have = vec![];
        let mut rev = None;
        let steps = [
            vec![card("a", "1"), card("b", "1")],
            vec![card("a", "2"), card("b", "1")],
            vec![card("c", "1"), card("a", "2"), card("b", "1")],
            vec![card("a", "3")],
            vec![],
            vec![card("b", "9")],
        ];
        for s in steps {
            let reply = l.reply(full(s.clone()), rev);
            rev = reply["rev"].as_u64();
            apply(&mut have, &reply);
            assert_eq!(have, s);
        }
    }

    /// What `serve` hands its output, one entry per write.
    #[derive(Clone, Default)]
    struct Writes(Arc<Mutex<Vec<Vec<u8>>>>);
    impl Write for Writes {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().push(b.to_vec());
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Over a relay each write the pump finds apart is one encrypted message, one ACK from the
    /// phone and one TCP packet each way: a 13 KB list cost the phone 350 KB/min of ACKs.
    #[test]
    fn each_line_reaches_the_connection_as_one_write() {
        let dir = std::env::temp_dir().join(format!("mooring-oneline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let writes = Writes::default();
        let input = b"{\"id\":1,\"op\":\"hello\"}\n{\"id\":2,\"op\":\"ping\"}\n";
        crate::remote::serve(
            &input[..],
            writes.clone(),
            &dir.join("sock"),
            crate::remote::Access::trusted(),
        )
        .unwrap();
        let got = writes.0.lock().unwrap();
        assert_eq!(got.len(), 2, "one write per line");
        for w in got.iter() {
            assert_eq!(w.iter().filter(|b| **b == b'\n').count(), 1);
            assert_eq!(w.last(), Some(&b'\n'));
            assert!(serde_json::from_slice::<Value>(w).is_ok());
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
