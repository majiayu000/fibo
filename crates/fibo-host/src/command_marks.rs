//! Reads the marks a shell puts around each command it runs (OSC 133, as Ghostty, iTerm2
//! and kitty use them): `C` when the command starts, `D;<exit>` when the shell is back at
//! its prompt. A `D` with no `C` before it is a prompt with no command (an empty line, or
//! the first prompt), so only a `C`…`D` pair is a finished command. Marks may be split
//! across reads; the unread start of one is kept for the next read.
use std::time::{Duration, Instant};

const START: &[u8] = b"\x1b]133;";
/// A mark longer than this is not one the shells write; it is dropped, not buffered.
const LONGEST: usize = 256;

#[derive(Default)]
pub struct Marks {
    partial: Vec<u8>,
    started: Option<Instant>,
    awaiting_handoff_prompt: bool,
    handoff_prompt_end: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Finished {
    /// None when the shell did not say.
    pub exit: Option<i32>,
    pub took: Duration,
    /// The command the terminal was opened to run, often an Agent, ended: `D;<exit>;fibo=launch`,
    /// written by `shell::run_then_stay` with no start mark. `took` is zero; it is not one of
    /// the shell's commands and leaves a command the shell marked started alone.
    pub launch: bool,
    /// Where in the read it was reported: just past the end mark.
    pub end: usize,
}

impl Marks {
    /// A running terminal may have reported its command start to the previous host.
    pub fn after_handoff() -> Self {
        Self {
            awaiting_handoff_prompt: true,
            ..Self::default()
        }
    }

    /// An unpaired end can clean up modes only when the original shell has the PTY back.
    pub fn take_handoff_prompt(
        &mut self,
        shell_is_foreground: impl FnOnce() -> bool,
    ) -> Option<usize> {
        let end = self.handoff_prompt_end.take()?;
        if !shell_is_foreground() {
            return None;
        }
        self.awaiting_handoff_prompt = false;
        Some(end)
    }

    /// The commands that finished in `bytes`, read at `at`.
    pub fn scan(&mut self, bytes: &[u8], at: Instant) -> Vec<Finished> {
        self.handoff_prompt_end = None;
        let mut found = vec![];
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
            if rest.len() < START.len() {
                if START.starts_with(rest) {
                    self.partial = rest.to_vec();
                }
                break;
            }
            if !rest.starts_with(START) {
                i += off + 1;
                continue;
            }
            let body = &rest[START.len()..];
            // End the mark at BEL or a complete ST so it is safe to insert output there.
            match body.iter().position(|&b| b == 0x07 || b == 0x1b) {
                Some(end) if body[end] == 0x1b && body.get(end + 1).is_none() => {
                    if rest.len() <= LONGEST {
                        self.partial = rest.to_vec();
                    }
                    break;
                }
                Some(end) if body[end] == 0x1b && body[end + 1] != b'\\' => {
                    i += off + START.len() + end;
                }
                Some(end) => {
                    i += off + START.len() + end + 1 + usize::from(body[end] == 0x1b);
                    // Offsets count in this read; a kept start came from the one before.
                    let carried = data.len() - bytes.len();
                    self.mark(&body[..end], at, i - carried, &mut found);
                }
                None => {
                    if rest.len() <= LONGEST {
                        self.partial = rest.to_vec();
                    }
                    break;
                }
            }
        }
        found
    }
    fn mark(&mut self, body: &[u8], at: Instant, end: usize, found: &mut Vec<Finished>) {
        let mut fields = body.split(|&b| b == b';');
        match fields.next() {
            Some(b"C") => {
                self.awaiting_handoff_prompt = false;
                self.handoff_prompt_end = None;
                self.started = Some(at);
            }
            Some(b"D") => {
                let exit = fields
                    .next()
                    .and_then(|f| std::str::from_utf8(f).ok())
                    .and_then(|f| f.trim().parse().ok());
                if fields.any(|f| f == b"fibo=launch") {
                    self.awaiting_handoff_prompt = false;
                    self.handoff_prompt_end = None;
                    found.push(Finished {
                        exit,
                        took: Duration::ZERO,
                        launch: true,
                        end,
                    });
                } else if let Some(started) = self.started.take() {
                    found.push(Finished {
                        exit,
                        took: at.saturating_duration_since(started),
                        launch: false,
                        end,
                    });
                } else if self.awaiting_handoff_prompt {
                    // The foreground is sampled after the read. A program can have
                    // printed an earlier D before the shell took the foreground back.
                    self.handoff_prompt_end = Some(end);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_a_start_mark_then_an_end_mark_with_its_exit_code() {
        let mut m = Marks::default();
        let t0 = Instant::now();
        // The first prompt and an empty line end nothing.
        assert!(m.scan(b"\x1b]133;D;0\x07$ ", t0).is_empty());
        assert!(m.scan(b"\x1b]133;C\x07", t0).is_empty());
        let t1 = t0 + Duration::from_millis(1500);
        assert_eq!(
            m.scan(b"out\r\n\x1b]133;D;2\x1b\\\x1b]133;A\x07$ ", t1),
            vec![Finished {
                exit: Some(2),
                took: Duration::from_millis(1500),
                launch: false,
                end: 16,
            }]
        );
        assert!(
            m.scan(b"\x1b]133;D;0\x07", t1).is_empty(),
            "one end per start"
        );
    }

    #[test]
    fn marks_split_across_reads_are_still_read_and_several_fit_in_one() {
        let mut m = Marks::default();
        let t = Instant::now();
        let all = b"\x1b]133;C\x07a\x1b]133;D;0\x07\x1b]133;C\x07b\x1b]133;D;1\x07";
        for split in 0..all.len() {
            let mut found = m.scan(&all[..split], t);
            found.extend(m.scan(&all[split..], t));
            let exits: Vec<_> = found.iter().map(|f| f.exit).collect();
            assert_eq!(exits, [Some(0), Some(1)], "split at {split}");
            // Each end, counted in the read that finished it, is just past its D mark.
            let marks = [b"a\x1b]133;D;0\x07".len() + 8, all.len()];
            let mut m2 = Marks::default();
            let first = m2.scan(&all[..split], t);
            let second = m2.scan(&all[split..], t);
            let ends: Vec<_> = first
                .iter()
                .map(|f| f.end)
                .chain(second.iter().map(|f| f.end + split))
                .collect();
            assert_eq!(ends, marks, "split at {split}");
        }
    }

    #[test]
    fn an_inherited_prompt_requires_a_complete_mark_and_the_original_foreground_shell() {
        let now = Instant::now();
        let st = b"\x1b]133;D;143\x1b\\";
        let bel = b"\x1b]133;D;0\x07";
        let mut fresh = Marks::default();
        assert!(fresh.scan(bel, now).is_empty());
        assert_eq!(fresh.take_handoff_prompt(|| unreachable!()), None);

        for split in 0..st.len() {
            let mut marks = Marks::after_handoff();
            assert!(marks.scan(&st[..split], now).is_empty());
            assert_eq!(marks.take_handoff_prompt(|| unreachable!()), None);
            assert!(marks.scan(&st[split..], now).is_empty());
            // A program still owns the foreground. Its mark does not consume the prompt.
            assert_eq!(marks.take_handoff_prompt(|| false), None);
            assert!(marks.scan(bel, now).is_empty());
            assert_eq!(marks.take_handoff_prompt(|| true), Some(bel.len()));
            assert!(marks.scan(bel, now).is_empty());
            assert_eq!(marks.take_handoff_prompt(|| unreachable!()), None);

            let mut marks = Marks::after_handoff();
            marks.scan(&st[..split], now);
            marks.scan(&st[split..], now);
            assert_eq!(
                marks.take_handoff_prompt(|| true),
                Some(st.len() - split),
                "split at {split}"
            );
        }
    }

    #[test]
    fn a_new_start_revokes_the_inherited_prompt_and_keeps_normal_exit_tracking() {
        let now = Instant::now();
        let mut marks = Marks::after_handoff();
        let through_exit = b"\x1b]133;D;0\x07\x1b]133;C\x07\x1b]133;D;7\x07";
        let empty_prompt = b"\x1b]133;D;0\x07";
        let read = [&through_exit[..], empty_prompt].concat();
        let finished = marks.scan(&read, now);
        assert_eq!(marks.take_handoff_prompt(|| unreachable!()), None);
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].exit, Some(7));
        assert_eq!(finished[0].end, through_exit.len());
        assert!(marks.scan(empty_prompt, now).is_empty());
        assert_eq!(marks.take_handoff_prompt(|| unreachable!()), None);
    }

    #[test]
    fn other_escapes_and_endless_marks_are_skipped() {
        let mut m = Marks::default();
        let t = Instant::now();
        assert!(m
            .scan(b"\x1b[31m\x1b]7;file://h/x\x07\x1b]1337;y\x07", t)
            .is_empty());
        let mut long = b"\x1b]133;C".to_vec();
        long.resize(LONGEST + 10, b'x');
        assert!(m.scan(&long, t).is_empty());
        assert!(m.partial.is_empty(), "dropped, not buffered");
        // An end with no code is still an end.
        m.scan(b"\x1b]133;C\x07", t);
        assert_eq!(m.scan(b"\x1b]133;D\x07", t)[0].exit, None);
    }

    #[test]
    fn the_launched_commands_end_is_its_own_and_leaves_a_started_command_alone() {
        let mut m = Marks::default();
        let t = Instant::now();
        let ended = m.scan(b"bye\r\n\x1b]133;D;130;fibo=launch\x07\x1b[?1000l", t);
        assert_eq!(
            ended,
            vec![Finished {
                exit: Some(130),
                took: Duration::ZERO,
                launch: true,
                end: 29,
            }]
        );
        m.scan(b"\x1b]133;C\x07", t);
        assert!(m.scan(b"\x1b]133;D;9;fibo=launch\x07", t)[0].launch);
        let later = t + Duration::from_secs(2);
        let shells = m.scan(b"\x1b]133;D;0\x07", later);
        assert_eq!(shells.len(), 1, "the shell's own command still ends");
        assert!(!shells[0].launch);
        assert_eq!(shells[0].took, Duration::from_secs(2));
    }
}
