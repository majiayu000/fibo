//! Runs the plugins' hooks (`mooring_core::hook`) when a session in a Mooring terminal
//! enters a state, or a command typed in one finishes. The host calls `observe` after every
//! commit and `command` when a terminal's shell marks a command finished; with no hook
//! installed both return at once. Each hook runs on its own thread under its timeout, so a
//! slow or broken hook costs the host nothing, and plugins take turns for the slots
//! (`Scheduler`), so it costs other plugins nothing either; outcomes go to the plugin's runs.
use mooring_core::content::{now, short, Turn};
use mooring_core::hook::{self, Event, Hook, Payload};
use mooring_core::model::Workspace;
use mooring_core::plugin_command::{Failed, Leftovers, Run};
use mooring_core::status::{self, Status};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Hooks running at once, in all.
const MAX_IN_FLIGHT: usize = 8;
/// Hooks of one plugin running at once: a plugin with slow hooks waits on itself and leaves
/// the rest of the slots to the others.
const MAX_PER_PLUGIN: usize = 2;
/// Events waiting per plugin; past this its oldest is dropped, and that goes to its runs.
const MAX_QUEUED: usize = 16;

/// Who runs next. Each plugin has its own line and they take turns, so no plugin's burst or
/// slow hook holds up another's.
struct Scheduler<J> {
    plugins: Vec<Line<J>>,
    /// The line that goes next.
    up_next: usize,
    running: usize,
}
struct Line<J> {
    plugin: PathBuf,
    waiting: VecDeque<J>,
    running: usize,
}
impl<J> Scheduler<J> {
    const fn new() -> Self {
        Self {
            plugins: Vec::new(),
            up_next: 0,
            running: 0,
        }
    }
    /// Queues a job; the plugin's oldest waiting job, when its line was full.
    fn submit(&mut self, plugin: &Path, job: J) -> Option<J> {
        let at = match self.plugins.iter().position(|l| l.plugin == plugin) {
            Some(at) => at,
            None => {
                self.plugins.push(Line {
                    plugin: plugin.to_owned(),
                    waiting: VecDeque::new(),
                    running: 0,
                });
                self.plugins.len() - 1
            }
        };
        let line = &mut self.plugins[at];
        line.waiting.push_back(job);
        (line.waiting.len() > MAX_QUEUED)
            .then(|| line.waiting.pop_front())
            .flatten()
    }
    /// The next job to start, if a slot is free and anyone may run; the caller must call
    /// `finished` when it ends.
    fn next(&mut self) -> Option<(PathBuf, J)> {
        if self.running >= MAX_IN_FLIGHT {
            return None;
        }
        let n = self.plugins.len();
        for i in 0..n {
            let at = (self.up_next + i) % n;
            let line = &mut self.plugins[at];
            if line.running >= MAX_PER_PLUGIN {
                continue;
            }
            if let Some(job) = line.waiting.pop_front() {
                line.running += 1;
                self.running += 1;
                self.up_next = (at + 1) % n;
                return Some((line.plugin.clone(), job));
            }
        }
        None
    }
    fn finished(&mut self, plugin: &Path) {
        if let Some(line) = self.plugins.iter_mut().find(|l| l.plugin == plugin) {
            line.running = line.running.saturating_sub(1);
        }
        self.running = self.running.saturating_sub(1);
        // A plugin with nothing left gives up its line; plugins come and go with reloads.
        self.plugins
            .retain(|l| l.running > 0 || !l.waiting.is_empty());
        self.up_next = if self.plugins.is_empty() {
            0
        } else {
            self.up_next % self.plugins.len()
        };
    }
}

static SCHEDULER: Mutex<Scheduler<(Hook, Payload)>> = Mutex::new(Scheduler::new());

/// The state each session was last seen in, so a hook fires on entering it, once.
#[derive(Default)]
pub struct Hooks {
    last: HashMap<String, String>,
    /// The first look only records: a host restart is not every session changing state.
    seeded: bool,
}

/// `done` is any turn that ended, claimed finished or not: paused is still yours to look at.
pub(crate) fn event(status: Status, turn: Turn) -> Option<Event> {
    match status {
        Status::Running => Some(Event::Started),
        Status::Waiting => Some(Event::Waiting),
        Status::Error => Some(Event::Error),
        Status::Review | Status::Background => Some(Event::Done),
        Status::Paused if matches!(turn, Turn::Completed | Turn::Interrupted) => Some(Event::Done),
        _ => None,
    }
}

impl Hooks {
    /// Every session's hookable state now, as if each had just entered it.
    pub fn current(w: &Workspace) -> Vec<Payload> {
        Hooks {
            last: HashMap::new(),
            seeded: true,
        }
        .changes(w)
    }
    pub fn seeded(&self) -> bool {
        self.seeded
    }
    pub fn observe(&mut self, w: &Workspace) {
        if hook::all().is_empty() {
            return;
        }
        let fired = self.changes(w);
        for p in fired {
            for h in hook::all().iter().filter(|h| h.on.contains(&p.event)) {
                spawn(h, p.clone());
            }
        }
    }
    /// The sessions that entered a hookable state since the last look.
    pub fn changes(&mut self, w: &Workspace) -> Vec<Payload> {
        let at = now();
        let mut seen = HashMap::new();
        let mut fired = vec![];
        for s in w.agent_sessions.iter().filter(|s| s.managed()) {
            let Some(c) = w.content.get(&s.id) else {
                continue;
            };
            if c.hidden_for(s).is_some() {
                continue;
            }
            let Some(e) = event(status::of(w, s, at), c.turn) else {
                continue;
            };
            // One turn is one start and one finish; one dialog is one wait however often
            // its text is refined; an error is keyed by its text.
            let detail = match e {
                Event::Waiting => c.waiting.clone(),
                Event::Error => c.error.clone(),
                Event::Done => c.last_reply.clone(),
                Event::Started | Event::CommandDone | Event::CommandFailed => String::new(),
            };
            let key = match e {
                Event::Waiting => {
                    format!("{}:{}:{}", c.turn_key, c.turn_started, c.waiting_episode)
                }
                Event::Error => format!("{}:{}:{}", c.turn_key, c.turn_started, c.error),
                _ => format!("{}:{}", c.turn_key, c.turn_started),
            };
            let key = format!("{}:{key}", e.name());
            // Seen but not told, as the banners do: coming back sends no burst, and the next
            // new fact is told again.
            if self.seeded && self.last.get(&s.id) != Some(&key) && !w.session_quiet(s) {
                let space = s
                    .terminal
                    .as_ref()
                    .and_then(|t| w.terminals.iter().find(|x| &x.id == t))
                    .and_then(|t| w.spaces.iter().find(|p| p.id == t.space))
                    .map(|p| p.name.clone());
                fired.push(Payload {
                    event: e,
                    session: s.id.clone(),
                    agent: s.runtime.name().to_owned(),
                    title: short(&w.session_title(s), 120),
                    cwd: s.cwd.clone(),
                    space,
                    detail: short(
                        detail
                            .lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .unwrap_or(""),
                        500,
                    ),
                    at,
                    // Which terminal it ran in, as `mooring list` names it.
                    terminal: s.terminal.clone(),
                    exit_code: None,
                    duration_ms: None,
                });
            }
            seen.insert(s.id.clone(), key);
        }
        self.last = seen;
        self.seeded = true;
        fired
    }
}

/// A command finished in a terminal: the hooks that listen for it and its duration.
pub fn command(p: Payload) {
    let took = p.duration_ms.unwrap_or(0);
    for h in hook::all()
        .iter()
        .filter(|h| h.on.contains(&p.event) && took >= h.min_ms)
    {
        spawn(h, p.clone());
    }
}

/// Queues a hook for its plugin's turn. A hook with no plugin (tests) has a line of its own.
fn spawn(h: &Hook, p: Payload) {
    let plugin = h.file.parent().unwrap_or(Path::new("")).to_owned();
    let dropped = SCHEDULER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .submit(&plugin, (h.clone(), p));
    if let Some((old, event)) = dropped {
        eprintln!(
            "mooring: hook {} ({}) dropped: {MAX_QUEUED} events already waiting",
            old.id,
            event.event.name()
        );
        let why = format!(
            "事件太多（已有 {MAX_QUEUED} 条在等），丢了这条 {}",
            event.event.name()
        );
        mooring_core::plugin_context::record(
            &plugin,
            &format!("hooks/{}", old.id),
            &Err(why),
            0,
            false,
        );
    }
    pump();
}

/// Starts whatever may start now.
fn pump() {
    loop {
        let next = SCHEDULER.lock().unwrap_or_else(|e| e.into_inner()).next();
        let Some((plugin, (h, p))) = next else {
            return;
        };
        std::thread::spawn(move || {
            let started = Instant::now();
            let result = run(&h, &p);
            if let Some(dir) = h.file.parent() {
                mooring_core::plugin_context::record(
                    dir,
                    &format!("hooks/{}", h.id),
                    &result,
                    started.elapsed().as_millis() as u64,
                    false,
                );
            }
            if let Err(e) = result {
                eprintln!("mooring: hook {} ({}) failed: {e}", h.id, p.event.name());
            }
            SCHEDULER
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .finished(&plugin);
            pump();
        });
    }
}

/// What runs for a hook: its command with the plugin's and the event's variables, or curl
/// posting to its webhook. The event goes on stdin either way.
pub fn command_for<'a>(h: &Hook, p: &Payload) -> Result<Run<'a>, String> {
    let body = serde_json::to_vec(p).map_err(|e| e.to_string())?;
    let timeout = Duration::from_millis(h.timeout_ms);
    let mut run = match (&h.command, &h.webhook) {
        (Some(c), _) => {
            let mut run = Run::shell(c, timeout, Leftovers::Keep);
            if let Some(dir) = h.file.parent() {
                run.env = mooring_core::plugin_context::env(dir);
            }
            run.env.extend(p.env());
            run
        }
        (None, Some(url)) => {
            // curl ships with macOS; its own limit backs up ours.
            let secs = h.timeout_ms.div_ceil(1000).to_string();
            let mut args: Vec<std::ffi::OsString> = Vec::new();
            for a in [
                "-sS",
                "-f",
                "-m",
                &secs,
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                "@-",
                url,
            ] {
                args.push(a.into());
            }
            Run {
                program: "/usr/bin/curl".into(),
                args,
                ..Run::shell("", timeout, Leftovers::Keep)
            }
        }
        (None, None) => return Err("没有 command 也没有 webhook".into()),
    };
    run.stdin = Some(body);
    Ok(run)
}

/// Runs one hook to completion or its timeout, the event on stdin.
pub fn run(h: &Hook, p: &Payload) -> Result<(), String> {
    let done = mooring_core::plugin_command::run(command_for(h, p)?).map_err(|e| match e {
        Failed::Start(why) => format!("启动不了：{why}"),
        Failed::TimedOut => format!("超过 {} ms，已停止", h.timeout_ms),
    })?;
    if done.status.success() {
        Ok(())
    } else {
        Err(format!("{} {}", done.how(), done.said()))
    }
}

#[cfg(test)]
mod tests {
    /// Plugin events name a conversation as the window does, and say where it runs.
    #[test]
    fn events_read_title_and_terminal_from_their_owners() {
        let w = mooring_core::sample::every_state();
        let fired = Hooks::current(&w);
        assert!(!fired.is_empty());
        for p in fired {
            let s = w.agent_sessions.iter().find(|s| s.id == p.session).unwrap();
            assert_eq!(p.title, short(&w.session_title(s), 120), "{}", p.session);
            assert_eq!(p.terminal, s.terminal, "{}", p.session);
        }
    }
    use super::*;

    /// A notifying plugin keeps 先不管's promise: nothing while aside, no burst on the way
    /// back, and the next new fact is told.
    #[test]
    fn a_space_put_aside_fires_no_hooks_and_no_burst_on_return() {
        use mooring_core::content::Turn;
        use mooring_core::model::Edit;
        let mut w = mooring_core::sample::every_state();
        w.spaces.push(mooring_core::model::Space {
            id: "other".into(),
            name: "另一个".into(),
            cwd: "/other".into(),
        });
        let quiet = |w: &mut Workspace, quiet| {
            w.edit(Edit::SetSpaceQuiet {
                id: "space".into(),
                quiet,
            })
            .unwrap();
        };
        let mut hooks = Hooks::default();
        quiet(&mut w, true);
        hooks.changes(&w);
        let id = "review";
        let content = w.content.get_mut(id).unwrap();
        content.turn = Turn::Waiting;
        content.waiting = "继续吗？".into();
        assert!(hooks.changes(&w).is_empty(), "nothing while aside");
        quiet(&mut w, false);
        assert!(hooks.changes(&w).is_empty(), "no burst on the way back");
        w.content.get_mut(id).unwrap().waiting_episode += 1;
        let fired = hooks.changes(&w);
        assert_eq!(fired.len(), 1, "{fired:?}");
        assert_eq!(fired[0].session, id);
    }

    /// A plugin with a burst of slow hooks: the other plugin's event runs straight away
    /// though eight of the first are ahead of it.
    #[test]
    fn a_plugin_with_a_burst_does_not_hold_up_another() {
        let (a, b) = (Path::new("/p/a"), Path::new("/p/b"));
        let mut s = Scheduler::new();
        for i in 0..8 {
            assert!(s.submit(a, i).is_none());
        }
        assert!(s.submit(b, 100).is_none());
        let first = s.next().unwrap();
        let second = s.next().unwrap();
        assert_eq!(first, (a.to_owned(), 0));
        assert_eq!(second, (b.to_owned(), 100), "the other plugin's turn");
        assert_eq!(s.next().unwrap(), (a.to_owned(), 1));
        assert_eq!(s.next(), None, "a has its two running, b has nothing");
        s.finished(a);
        assert_eq!(s.next().unwrap(), (a.to_owned(), 2), "in order");
    }

    #[test]
    fn all_slots_are_shared_and_a_full_line_drops_its_oldest() {
        let mut s = Scheduler::new();
        let names: Vec<PathBuf> = (0..6).map(|i| PathBuf::from(format!("/p/{i}"))).collect();
        for n in &names {
            s.submit(n, 0);
            s.submit(n, 1);
        }
        let mut started = 0;
        while s.next().is_some() {
            started += 1;
        }
        assert_eq!(started, MAX_IN_FLIGHT);
        s.finished(&names[0]);
        assert!(s.next().is_some(), "a slot freed is taken again");

        let mut s = Scheduler::new();
        let p = Path::new("/p/full");
        for i in 0..MAX_QUEUED {
            assert_eq!(s.submit(p, i), None);
        }
        assert_eq!(s.submit(p, MAX_QUEUED), Some(0), "the oldest goes");
    }

    /// Through the real runner: plugin a's hooks all sleep, plugin b's still fires at once,
    /// and a's overflow shows in its runs.
    #[test]
    fn a_slow_plugins_hooks_leave_room_for_others_and_overflow_is_reported() {
        let root = std::env::temp_dir().join(format!("kl-fair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugin = |id: &str, command: String| {
            let dir = root.join("plugins").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("plugin.json"),
                format!(r#"{{"api":1,"id":"{id}","name":"x","provides":["hooks"]}}"#),
            )
            .unwrap();
            std::fs::write(dir.join("hooks.json"), "{}").unwrap();
            Hook {
                id: "h".into(),
                file: dir.join("hooks.json"),
                ..hook(&command, 2_000)
            }
        };
        let mark = root.join("b-ran");
        let slow = plugin("a", "sleep 1".into());
        let quick = plugin("b", format!("touch {}", mark.display()));
        for _ in 0..(MAX_PER_PLUGIN + MAX_QUEUED + 3) {
            spawn(&slow, payload());
        }
        let started = Instant::now();
        spawn(&quick, payload());
        while !mark.exists() && started.elapsed() < Duration::from_millis(900) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(mark.exists(), "b waited on a");
        let plugins = root.join("plugins");
        let runs = mooring_core::plugin_context::runs(&plugins, "a");
        assert_eq!(
            runs.iter()
                .filter(|r| r.detail.contains("事件太多"))
                .count(),
            3,
            "{runs:?}"
        );
        // Let a's queue drain enough not to outlive the directory it logs to.
        std::thread::sleep(Duration::from_millis(100));
    }

    fn hook(command: &str, timeout_ms: u64) -> Hook {
        // These limits are about the hook, not a second spent in this machine's profile.
        std::env::set_var("MOORING_NO_LOGIN_PATH", "1");
        Hook {
            id: "t".into(),
            on: vec![Event::Waiting],
            command: Some(command.into()),
            webhook: None,
            timeout_ms,
            min_ms: 0,
            file: PathBuf::new(),
            skipped: vec![],
        }
    }
    fn payload() -> Payload {
        Payload {
            event: Event::Waiting,
            session: "s1".into(),
            agent: "Codex".into(),
            title: "修 bug; rm -rf /".into(),
            cwd: "/tmp".into(),
            space: Some("lib".into()),
            detail: "是否允许推送？".into(),
            at: 1,
            terminal: None,
            exit_code: None,
            duration_ms: None,
        }
    }

    #[test]
    fn a_command_hook_gets_the_event_on_stdin_and_in_its_environment() {
        let out = std::env::temp_dir().join(format!("kl-hook-out-{}", std::process::id()));
        let cmd = format!(
            "cat > {0}; printf '\\n%s|%s' \"$MOORING_EVENT\" \"$MOORING_TITLE\" >> {0}",
            out.display()
        );
        run(&hook(&cmd, 5_000), &payload()).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        let (json, env) = text.split_once('\n').unwrap();
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(v["event"], "waiting");
        assert_eq!(v["space"], "lib");
        // The title stays data: it reaches the hook verbatim and runs nothing.
        assert_eq!(env, "waiting|修 bug; rm -rf /");
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn each_state_reaches_hooks_once_and_a_restart_replays_nothing() {
        use mooring_core::content::Content;
        use mooring_core::model::{AgentSession, Outcome, ProcessState, Terminal};
        let mut w = Workspace::default();
        // The agent is still in its terminal, so a question in it waits for you.
        w.terminals.push(Terminal {
            id: "pty".into(),
            space: "lib".into(),
            argv: vec![],
            cwd: "/project".into(),
            pid: None,
            state: ProcessState::Running,
            exit_code: None,
            error: None,
        });
        w.agent_sessions.push(AgentSession {
            id: "s".into(),
            runtime: mooring_core::model::Agent::Codex,
            cli_session_id: "cli".into(),
            terminal: Some("pty".into()),
            current: true,
            task: None,
            outcome: Outcome::Open,
            updated: now(),
            cwd: "/project".into(),
            transcript: "log".into(),
            binding: None,
            name: String::new(),
        });
        let content = |turn, key: &str, waiting: &str, episode| Content {
            interactive: true,
            title: "修复登录".into(),
            turn,
            turn_key: key.into(),
            turn_started: now(),
            waiting: waiting.into(),
            waiting_episode: episode,
            last_reply: "改好了\n细节".into(),
            updated: now(),
            ..Default::default()
        };
        let mut hooks = Hooks::default();
        w.content
            .insert("s".into(), content(Turn::Waiting, "t1", "推送吗？", 1));
        assert!(hooks.changes(&w).is_empty(), "the first look only records");
        assert!(hooks.changes(&w).is_empty());
        // A refined question in the same dialog is not a new event; a new dialog is.
        w.content.insert(
            "s".into(),
            content(Turn::Waiting, "t1", "推送到 main 吗？", 1),
        );
        assert!(hooks.changes(&w).is_empty());
        w.content
            .insert("s".into(), content(Turn::Waiting, "t1", "再确认？", 2));
        let fired = hooks.changes(&w);
        assert_eq!(fired.len(), 1);
        assert_eq!(
            (fired[0].event, fired[0].detail.as_str()),
            (Event::Waiting, "再确认？")
        );
        w.content
            .insert("s".into(), content(Turn::Running, "t2", "", 2));
        assert_eq!(hooks.changes(&w)[0].event, Event::Started);
        w.content
            .insert("s".into(), content(Turn::Completed, "t2", "", 2));
        let fired = hooks.changes(&w);
        assert_eq!(
            (fired[0].event, fired[0].detail.as_str()),
            (Event::Done, "改好了")
        );
        assert!(hooks.changes(&w).is_empty());
    }

    #[test]
    fn a_slow_or_failing_hook_is_stopped_and_reported() {
        let started = Instant::now();
        let err = run(&hook("sleep 5", 200), &payload()).unwrap_err();
        assert!(err.contains("200 ms"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
        let err = run(&hook("echo nope >&2; exit 3", 2_000), &payload()).unwrap_err();
        assert!(err.contains("nope"), "{err}");
    }

    /// A failing hook that left a background child holding stderr returns at once;
    /// it would otherwise hold one of the 8 in-flight slots for as long as the child runs.
    #[test]
    fn a_failed_hook_with_a_background_child_returns_promptly() {
        let start = Instant::now();
        let err = run(&hook("echo bad >&2; sleep 3 & exit 1", 2_000), &payload()).unwrap_err();
        assert!(
            start.elapsed() < Duration::from_millis(1500),
            "{:?}",
            start.elapsed()
        );
        assert!(err.contains("bad") || err.contains("exit"), "{err}");
    }

    /// A timed-out hook's children are stopped with it.
    #[test]
    fn a_timed_out_hook_stops_its_children() {
        let f = std::env::temp_dir().join(format!("kl-hook-child-{}", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let _ = run(
            &hook(&format!("sleep 30 & echo $! > {}; wait", f.display()), 300),
            &payload(),
        );
        let pid: i32 = std::fs::read_to_string(&f).unwrap().trim().parse().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if alive {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        let _ = std::fs::remove_file(&f);
        assert!(!alive, "the hook's child {pid} outlived its timeout");
    }

    #[test]
    fn the_shipped_notifier_stays_quiet_except_on_errors_when_set_so() {
        // The real notify-macos plugin, with osascript swapped for a file the test reads.
        let root = std::env::temp_dir().join(format!("kl-notify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("plugins/notify-macos");
        std::fs::create_dir_all(&dir).unwrap();
        let shipped =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/notify-macos");
        std::fs::copy(shipped.join("plugin.json"), dir.join("plugin.json")).unwrap();
        let out = root.join("said");
        let hooks = std::fs::read_to_string(shipped.join("hooks.json"))
            .unwrap()
            .replace(
                "osascript",
                &format!(
                    "echo \\\"$MOORING_PLUGIN_ID $MOORING_EVENT\\\" >> {} #",
                    out.display()
                ),
            );
        std::fs::write(dir.join("hooks.json"), hooks).unwrap();
        let (loaded, report) = mooring_core::hook::load(vec![dir.join("hooks.json")]);
        assert!(report[0].result.is_ok(), "{report:?}");
        let h = &loaded[0];
        let fire = |event| {
            let mut p = payload();
            p.event = event;
            run(h, &p).unwrap();
        };
        fire(Event::Waiting);
        mooring_core::plugin_context::set(
            &root.join("plugins"),
            "notify-macos",
            "only_errors",
            serde_json::Value::Bool(true),
        )
        .unwrap();
        fire(Event::Waiting);
        fire(Event::Error);
        let said = std::fs::read_to_string(&out).unwrap();
        assert_eq!(said, "notify-macos waiting\nnotify-macos error\n");
        let _ = std::fs::remove_dir_all(&root);
    }
}
