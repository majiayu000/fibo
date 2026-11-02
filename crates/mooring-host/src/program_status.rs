//! What programs say about themselves through OSC 7501, the Program Status Protocol:
//! `ESC ] 7501 ; key=value:key=value ST`, ST being `ESC \` or BEL. The protocol only
//! carries state; how it shows is the terminal's choice.
//!
//! Kept per terminal, as the protocol has it:
//! - one record per `id` (`build/test` is a child of `build`; no `id` is the root); every
//!   report replaces its record whole; `state=clear` removes a record and its children,
//!   without an `id` every record;
//! - no heartbeat: a new shell prompt (OSC 133 `A`) removes `working`, `blocked` and
//!   `idle` records, while `done` and `error` stay until the next command starts (OSC 133
//!   `C`), so a script that reports `done` and exits is still seen;
//! - a malformed pair is skipped; a report over the limits, with invalid base64 or with
//!   control characters in its text is dropped whole; `OSC 7501 ; ?` is answered with the
//!   states and kinds supported;
//! - OSC 9;4 progress stands in for the root record until the first 7501 report;
//!   a full reset (`ESC c`) removes every record.
//!
//! The host shows one record per terminal: the root, else the most urgent child.
use base64::Engine;
use std::collections::BTreeMap;

const OSC: &[u8] = b"\x1b]";
/// The longest sequence kept; a longer one is dropped, not buffered.
const LONGEST: usize = 4096;
const MAX_RECORDS: usize = 256;
const MAX_APP: usize = 32;
const MAX_TITLE: (usize, usize) = (256, 192);
const MAX_MSG: (usize, usize) = (2732, 2048);
const MAX_ID: usize = 128;
const MAX_SEGMENT: usize = 32;
const MAX_DEPTH: usize = 8;
const STATES: &[&str] = &["idle", "working", "done", "blocked", "error"];
const KINDS: &[&str] = &["permission", "question", "auth"];
/// The answer to `OSC 7501 ; ?`.
pub const PROBE_REPLY: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\";

/// One record as the program reported it, its `app` inherited when it gave none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub state: String,
    pub kind: Option<String>,
    pub progress: Option<u8>,
    pub app: String,
    pub title: String,
    pub msg: String,
}

/// What one read changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// `Some(None)` when the shown record went away, `Some(Some(_))` when it changed.
    pub change: Option<Option<Said>>,
    /// Bytes to write back to the program: the answer to a probe.
    pub reply: Vec<u8>,
}

#[derive(Default)]
pub struct Scanner {
    partial: Vec<u8>,
    records: BTreeMap<String, Said>,
    /// A 7501 report arrived since the last full reset: OSC 9;4 no longer maps.
    reported: bool,
}

impl Scanner {
    pub fn scan(&mut self, bytes: &[u8]) -> Scan {
        let before = self.shown();
        let mut reply = vec![];
        let joined;
        let data = if self.partial.is_empty() {
            bytes
        } else {
            let mut j = std::mem::take(&mut self.partial);
            j.extend_from_slice(bytes);
            joined = j;
            &joined[..]
        };
        let mut i = 0;
        while let Some(off) = data[i..].iter().position(|&b| b == 0x1b) {
            let rest = &data[i + off..];
            if rest.len() < 2 {
                self.partial = rest.to_vec();
                break;
            }
            // Full reset (RIS): every record goes, and OSC 9;4 maps again.
            if rest[1] == b'c' {
                self.records.clear();
                self.reported = false;
                i += off + 2;
                continue;
            }
            if !rest.starts_with(OSC) {
                i += off + 1;
                continue;
            }
            let body = &rest[OSC.len()..];
            // BEL, or the ESC of ESC \, ends it.
            let Some(end) = body.iter().position(|&b| b == 0x07 || b == 0x1b) else {
                if rest.len() <= LONGEST {
                    self.partial = rest.to_vec();
                }
                break;
            };
            if end <= LONGEST {
                self.osc(&body[..end], &mut reply);
            }
            i += off + OSC.len() + end + 1;
        }
        let after = self.shown();
        Scan {
            change: (after != before).then_some(after),
            reply,
        }
    }

    fn osc(&mut self, body: &[u8], reply: &mut Vec<u8>) {
        if let Some(report) = body.strip_prefix(b"7501;") {
            if report == b"?" {
                reply.extend_from_slice(PROBE_REPLY);
            } else {
                self.report(report);
            }
        } else if let Some(mark) = body.strip_prefix(b"133;") {
            match mark.split(|&b| b == b';').next() {
                // A new prompt: what was running or waiting is over.
                Some(b"A") => self
                    .records
                    .retain(|_, r| matches!(r.state.as_str(), "done" | "error")),
                // A new command: the last one's outcome has been seen.
                Some(b"C") => self.records.clear(),
                _ => {}
            }
        } else if let Some(progress) = body.strip_prefix(b"9;4;") {
            if !self.reported {
                self.progress(progress);
            }
        }
    }

    fn report(&mut self, body: &[u8]) {
        let Some(r) = parse(body) else {
            return;
        };
        self.reported = true;
        match r {
            Report::Clear(id) => match id {
                None => self.records.clear(),
                Some(id) => {
                    let child = format!("{id}/");
                    self.records
                        .retain(|k, _| *k != id && !k.starts_with(&child));
                }
            },
            Report::Set(id, said) => {
                let key = id.unwrap_or_default();
                if self.records.len() < MAX_RECORDS || self.records.contains_key(&key) {
                    self.records.insert(key, said);
                }
            }
        }
    }

    /// OSC 9;4;st;pr: 0 removes, 1 and 4 run at pr %, 2 failed, 3 runs with no percent.
    fn progress(&mut self, body: &[u8]) {
        let text = String::from_utf8_lossy(body);
        let mut parts = text.split(';');
        let st = parts.next().unwrap_or("");
        let pr = parts
            .next()
            .and_then(|p| p.parse().ok())
            .filter(|p| *p <= 100);
        let said = |state: &str, progress| Said {
            state: state.into(),
            kind: None,
            progress,
            app: String::new(),
            title: String::new(),
            msg: String::new(),
        };
        match st {
            "0" => {
                self.records.remove("");
            }
            "1" | "4" => {
                self.records.insert(String::new(), said("working", pr));
            }
            "2" => {
                self.records.insert(String::new(), said("error", None));
            }
            "3" => {
                self.records.insert(String::new(), said("working", None));
            }
            _ => {}
        }
    }

    /// The record the host shows: the root, else the most urgent child, its `app` taken
    /// from the nearest ancestor that has one.
    fn shown(&self) -> Option<Said> {
        let urgency = |s: &str| match s {
            "blocked" => 0,
            "error" => 1,
            "working" => 2,
            "done" => 3,
            _ => 4,
        };
        let (id, said) = self.records.get_key_value("").or_else(|| {
            self.records
                .iter()
                .min_by_key(|(id, r)| (urgency(&r.state), id.matches('/').count()))
        })?;
        let mut said = said.clone();
        let mut at = id.as_str();
        while said.app.is_empty() && !at.is_empty() {
            at = at.rsplit_once('/').map_or("", |(parent, _)| parent);
            if let Some(a) = self.records.get(at).filter(|a| !a.app.is_empty()) {
                said.app = a.app.clone();
            }
        }
        Some(said)
    }
}

enum Report {
    Set(Option<String>, Said),
    Clear(Option<String>),
}

/// One report's body, or `None` for one to ignore or drop.
fn parse(body: &[u8]) -> Option<Report> {
    let body = std::str::from_utf8(body).ok()?;
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    for pair in body.split(':') {
        let Some((key, value)) = pair.split_once('=') else {
            continue; // a malformed pair is skipped
        };
        if key.is_empty() || !value.bytes().all(value_char) {
            continue;
        }
        fields.insert(key, value); // the last of a repeated key wins
    }
    let state = *fields.get("state")?;
    let id = match fields.get("id") {
        None => None,
        Some(id) => Some(valid_id(id)?.to_owned()),
    };
    if state == "clear" {
        return Some(Report::Clear(id));
    }
    if !STATES.contains(&state) {
        return None;
    }
    let app = fields.get("app").copied().unwrap_or("");
    if app.len() > MAX_APP {
        return None;
    }
    let said = Said {
        state: state.into(),
        kind: fields
            .get("kind")
            .filter(|k| state == "blocked" && KINDS.contains(k))
            .map(|k| (*k).to_owned()),
        progress: fields
            .get("progress")
            .filter(|_| matches!(state, "working" | "blocked"))
            .and_then(|p| p.parse().ok())
            .filter(|p| *p <= 100),
        app: app.into(),
        title: text(fields.get("title").copied(), MAX_TITLE)?,
        msg: text(fields.get("msg").copied(), MAX_MSG)?,
    };
    Some(Report::Set(id, said))
}

fn value_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_.,+/=-".contains(&b)
}

/// `a/b/c`: up to 8 segments of up to 32 bytes, 128 in all, none empty.
fn valid_id(id: &str) -> Option<&str> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID
        && id.split('/').count() <= MAX_DEPTH
        && id
            .split('/')
            .all(|s| !s.is_empty() && s.len() <= MAX_SEGMENT);
    ok.then_some(id)
}

/// A base64 field (padding optional) within its encoded and decoded limits, with no
/// control characters; `None` drops the whole report. Invisible direction marks, which
/// could reorder what is shown around it, are removed.
fn text(value: Option<&str>, (encoded, decoded): (usize, usize)) -> Option<String> {
    let Some(value) = value else {
        return Some(String::new());
    };
    if value.len() > encoded {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(value.trim_end_matches('='))
        .ok()?;
    if bytes.len() > decoded {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    Some(
        text.chars()
            .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(s)
    }
    fn shown(s: &Scanner) -> Option<(String, String)> {
        s.shown().map(|r| (r.state, r.msg))
    }
    fn said(state: &str) -> Said {
        Said {
            state: state.into(),
            kind: None,
            progress: None,
            app: String::new(),
            title: String::new(),
            msg: String::new(),
        }
    }

    #[test]
    fn a_report_carries_its_fields_and_replaces_its_record_whole() {
        let mut s = Scanner::default();
        let msg = b64("Apply these changes?");
        let got = s.scan(
            format!("x\x1b]7501;state=blocked:kind=permission:progress=40:app=claude-code:title={}:msg={msg}\x1b\\", b64("Run tests"))
                .as_bytes(),
        );
        assert_eq!(
            got.change,
            Some(Some(Said {
                state: "blocked".into(),
                kind: Some("permission".into()),
                progress: Some(40),
                app: "claude-code".into(),
                title: "Run tests".into(),
                msg: "Apply these changes?".into(),
            }))
        );
        // A new report replaces the record: its app is gone unless repeated.
        let got = s.scan(b"\x1b]7501;state=working\x07");
        assert_eq!(got.change, Some(Some(said("working"))));
        assert_eq!(s.scan(b"plain output").change, None, "nothing said");
    }

    #[test]
    fn malformed_pairs_are_skipped_and_bad_reports_ignored() {
        let mut s = Scanner::default();
        // A pair without = and one with a bad character are skipped; the rest applies.
        let got = s.scan(b"\x1b]7501;nonsense:state=working:app=a b:progress=5\x07");
        assert_eq!(
            got.change.flatten().map(|r| (r.state, r.app, r.progress)),
            Some(("working".into(), String::new(), Some(5)))
        );
        // The last of a repeated key wins.
        s.scan(b"\x1b]7501;state=idle:state=done\x07");
        assert_eq!(shown(&s).unwrap().0, "done");
        // No state, an unknown state, or a bad id: the report is ignored.
        for bad in ["app=x", "state=sleeping", "state=working:id=a//b"] {
            assert_eq!(
                s.scan(format!("\x1b]7501;{bad}\x07").as_bytes()).change,
                None,
                "{bad}"
            );
        }
        // kind only on blocked, progress only on working and blocked, 0–100.
        s.scan(b"\x1b]7501;state=done:kind=permission:progress=50\x07");
        assert_eq!(s.shown().map(|r| (r.kind, r.progress)), Some((None, None)));
        s.scan(b"\x1b]7501;state=blocked:kind=telepathy:progress=300\x07");
        assert_eq!(s.shown().map(|r| (r.kind, r.progress)), Some((None, None)));
    }

    #[test]
    fn over_the_limits_bad_base64_or_control_characters_drop_the_report() {
        let mut s = Scanner::default();
        s.scan(b"\x1b]7501;state=working\x07");
        let long_app = "a".repeat(33);
        let long_msg = b64(&"m".repeat(2049));
        let newline = b64("two\nlines");
        for bad in [
            format!("state=done:app={long_app}"),
            format!("state=done:msg={long_msg}"),
            // Inside the value characters but not base64.
            "state=done:msg=A".to_owned(),
            format!("state=done:msg={newline}"),
        ] {
            assert_eq!(
                s.scan(format!("\x1b]7501;{bad}\x07").as_bytes()).change,
                None,
                "{bad}"
            );
        }
        assert_eq!(shown(&s).unwrap().0, "working", "nothing applied partly");
        // Padding is optional; direction marks are removed.
        s.scan(
            format!(
                "\x1b]7501;state=done:msg={}\x07",
                b64("ok\u{202e}!").trim_end_matches('=')
            )
            .as_bytes(),
        );
        assert_eq!(shown(&s).unwrap().1, "ok!");
        let mut huge = b"\x1b]7501;state=working:msg=".to_vec();
        huge.resize(LONGEST + 10, b'A');
        assert_eq!(s.scan(&huge).change, None);
        assert!(s.partial.is_empty(), "dropped, not buffered");
    }

    #[test]
    fn a_new_prompt_ends_running_and_waiting_records_but_keeps_outcomes() {
        let mut s = Scanner::default();
        s.scan(b"\x1b]7501;state=working\x07");
        assert_eq!(
            s.scan(b"\x1b]133;A\x07$ ").change,
            Some(None),
            "working ends at the prompt"
        );
        // A script reports done and exits: the outcome stays past the prompt.
        s.scan(b"\x1b]133;C\x07\x1b]7501;state=done\x07\x1b]133;D;0\x07\x1b]133;A\x07$ ");
        assert_eq!(shown(&s).unwrap().0, "done");
        // ...until the next command starts.
        assert_eq!(s.scan(b"\x1b]133;C\x07").change, Some(None));
        s.scan(b"\x1b]7501;state=error\x07\x1b]133;A\x07");
        assert_eq!(shown(&s).unwrap().0, "error", "error stays too");
    }

    #[test]
    fn child_records_form_a_tree_and_inherit_their_app() {
        let mut s = Scanner::default();
        s.scan(b"\x1b]7501;state=working:id=deploy:app=terraform\x07");
        s.scan(
            format!(
                "\x1b]7501;state=blocked:kind=permission:id=deploy/eu-west:title={}\x07",
                b64("EU West")
            )
            .as_bytes(),
        );
        s.scan(b"\x1b]7501;state=done:id=deploy/us-east\x07");
        // No root: the most urgent child shows, with its parent's app.
        let r = s.shown().unwrap();
        assert_eq!(
            (r.state.as_str(), r.title.as_str(), r.app.as_str()),
            ("blocked", "EU West", "terraform")
        );
        // Clearing a record clears its children.
        s.scan(b"\x1b]7501;state=clear:id=deploy\x07");
        assert!(s.records.is_empty());
        // A root report shows over children; clear without id clears everything.
        s.scan(b"\x1b]7501;state=blocked:id=x\x07\x1b]7501;state=idle\x07");
        assert_eq!(shown(&s).unwrap().0, "idle");
        s.scan(b"\x1b]7501;state=clear\x07");
        assert!(s.records.is_empty());
    }

    #[test]
    fn a_probe_is_answered() {
        let mut s = Scanner::default();
        let got = s.scan(b"\x1b]7501;?\x1b\\");
        assert_eq!(got.reply, PROBE_REPLY);
        assert_eq!(got.change, None);
    }

    #[test]
    fn osc_9_4_stands_in_until_a_real_report_and_a_full_reset_clears_all() {
        let mut s = Scanner::default();
        s.scan(b"\x1b]9;4;1;30\x07");
        assert_eq!(
            s.shown().map(|r| (r.state, r.progress)),
            Some(("working".into(), Some(30)))
        );
        s.scan(b"\x1b]9;4;0\x07");
        assert_eq!(s.shown(), None);
        s.scan(b"\x1b]7501;state=blocked\x07\x1b]9;4;1;80\x07");
        assert_eq!(
            shown(&s).unwrap().0,
            "blocked",
            "9;4 no longer maps after a 7501 report"
        );
        assert_eq!(s.scan(b"\x1bc").change, Some(None), "full reset");
        s.scan(b"\x1b]9;4;2\x07");
        assert_eq!(shown(&s).unwrap().0, "error", "and 9;4 maps again");
    }

    #[test]
    fn sequences_split_across_reads_are_still_read() {
        let all = format!(
            "a\x1b]7501;state=working\x1b\\b\x1b]133;C\x07\x1b]7501;state=error:msg={}\x07",
            b64("boom")
        );
        let all = all.as_bytes();
        for split in 0..all.len() {
            let mut s = Scanner::default();
            s.scan(&all[..split]);
            s.scan(&all[split..]);
            assert_eq!(
                shown(&s),
                Some(("error".into(), "boom".into())),
                "split at {split}"
            );
        }
    }
}
