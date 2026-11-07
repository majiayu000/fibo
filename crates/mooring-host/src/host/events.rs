//! `mooring events`: the sessions' states, then each change, to whoever subscribed.
//! The state changes are the plugin hooks' own (`plugin_hooks::Hooks`), tracked apart so a
//! subscriber coming or going never moves what the hooks have seen. With nobody
//! subscribed nothing here runs.
use super::*;
use mooring_core::hook::Notice;
use mooring_platform::local;
use std::collections::HashSet;
use std::sync::Condvar;

/// What subscribers were last told, to send only what changed.
#[derive(Default)]
pub(super) struct Tracker {
    states: crate::plugin_hooks::Hooks,
    /// Session -> the approval question its screen shows.
    approvals: HashMap<String, String>,
    running: HashSet<String>,
    /// Running terminal -> the conversation current in it, to say when it changes.
    sessions: HashMap<String, String>,
}
/// The conversation each running terminal shows now.
fn sessions(w: &Workspace) -> HashMap<String, String> {
    w.terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .filter_map(|t| Some((t.id.clone(), w.current_agent(&t.id)?.id.clone())))
        .collect()
}
fn approvals(w: &Workspace) -> HashMap<String, String> {
    w.agent_sessions
        .iter()
        .filter(|s| s.current)
        .filter_map(|s| {
            let c = w.content.get(&s.id)?;
            (!c.approval.is_empty()).then(|| (s.id.clone(), c.approval.clone()))
        })
        .collect()
}
fn running(w: &Workspace) -> HashSet<String> {
    w.terminals
        .iter()
        .filter(|t| t.state == ProcessState::Running)
        .map(|t| t.id.clone())
        .collect()
}
fn session_notice(event: &str, w: &Workspace, id: &str, detail: &str, at: u64) -> Notice {
    let mut n = Notice::named(event, at);
    n.session = id.to_owned();
    n.detail = detail.to_owned();
    if let Some(s) = w.agent_sessions.iter().find(|s| s.id == id) {
        n.agent = s.runtime.name().to_owned();
        n.cwd = s.cwd.clone();
        n.terminal = s.terminal.clone();
        n.title = mooring_core::content::short(&w.session_title(s), 120);
        n.space = space_of(w, s.terminal.as_deref());
    }
    n
}
fn space_of(w: &Workspace, terminal: Option<&str>) -> Option<String> {
    let t = w
        .terminals
        .iter()
        .find(|t| Some(t.id.as_str()) == terminal)?;
    w.spaces
        .iter()
        .find(|p| p.id == t.space)
        .map(|p| p.name.clone())
}
fn terminal_notice(event: &str, w: &Workspace, id: &str, at: u64) -> Notice {
    let mut n = Notice::named(event, at);
    n.terminal = Some(id.to_owned());
    if let Some(t) = w.terminals.iter().find(|t| t.id == id) {
        n.cwd = t.cwd.clone();
    }
    n.space = space_of(w, Some(id));
    n
}
/// Only running terminals ask: one that ended took its prompt with it.
fn map_retain(map: &mut HashMap<String, TrustPrompt>, w: &Workspace) {
    map.retain(|id, _| {
        (w.terminals.iter()).any(|t| t.id == *id && t.state == ProcessState::Running)
    });
}
impl Tracker {
    /// Starts from `w`: the first subscriber gets it as its snapshot, not as changes.
    pub(super) fn seed(&mut self, w: &Workspace) {
        self.states = Default::default();
        self.states.changes(w);
        self.approvals = approvals(w);
        self.running = running(w);
        self.sessions = sessions(w);
    }
    /// The state now: every running terminal, every session in a state a hook would
    /// name, and every approval on screen.
    pub(super) fn snapshot(w: &Workspace) -> Vec<Notice> {
        let at = mooring_core::content::now();
        let mut out: Vec<Notice> = {
            let mut ids: Vec<_> = running(w).into_iter().collect();
            ids.sort();
            ids.iter()
                .map(|id| terminal_notice("terminal_started", w, id, at))
                .collect()
        };
        out.extend(
            crate::plugin_hooks::Hooks::current(w)
                .into_iter()
                .map(Notice::from),
        );
        let mut asks: Vec<_> = approvals(w).into_iter().collect();
        asks.sort();
        out.extend(
            asks.iter()
                .map(|(id, q)| session_notice("approval", w, id, q, at)),
        );
        out
    }
    pub(super) fn changes(&mut self, w: &Workspace) -> Vec<Notice> {
        let at = mooring_core::content::now();
        let mut out = vec![];
        let now_running = running(w);
        let mut started: Vec<_> = now_running.difference(&self.running).cloned().collect();
        started.sort();
        out.extend(
            started
                .iter()
                .map(|id| terminal_notice("terminal_started", w, id, at)),
        );
        out.extend(self.states.changes(w).into_iter().map(Notice::from));
        let now_asking = approvals(w);
        let mut gone: Vec<_> = self
            .approvals
            .iter()
            .filter(|(id, q)| now_asking.get(*id) != Some(*q))
            .map(|(id, q)| (id.clone(), q.clone()))
            .collect();
        gone.sort();
        out.extend(
            gone.iter()
                .map(|(id, q)| session_notice("approval_gone", w, id, q, at)),
        );
        let mut shown: Vec<_> = now_asking
            .iter()
            .filter(|(id, q)| self.approvals.get(*id) != Some(*q))
            .collect();
        shown.sort();
        out.extend(
            shown
                .iter()
                .map(|(id, q)| session_notice("approval", w, id, q, at)),
        );
        // A terminal now on another conversation (`/new`, `/clear`, an Agent started in a
        // shell): a phone showing it follows (plan stage 4); `detail` is the one before.
        let now_sessions = sessions(w);
        let mut switched: Vec<_> = now_sessions
            .iter()
            .filter(|(t, s)| self.sessions.get(*t) != Some(*s))
            .map(|(t, s)| (s.clone(), self.sessions.get(t).cloned().unwrap_or_default()))
            .collect();
        switched.sort();
        out.extend(
            switched
                .iter()
                .map(|(s, before)| session_notice("session_switched", w, s, before, at)),
        );
        self.sessions = now_sessions;
        let mut ended: Vec<_> = self.running.difference(&now_running).cloned().collect();
        ended.sort();
        out.extend(
            ended
                .iter()
                .map(|id| terminal_notice("terminal_exited", w, id, at)),
        );
        self.approvals = now_asking;
        self.running = now_running;
        out
    }
}

/// Notices one `mooring events` connection may have waiting before more are dropped and
/// it is told `lagged`. Counted in notices, not frames: one commit carries one notice or
/// hundreds, so how many frames a burst makes depends on how fast the host is, and a frame
/// limit called a reading client lagged on a slow machine and missed a stalled one on a
/// fast machine. 4096 holds a burst well beyond what a person's terminals produce between
/// two reads (thousands of commands finishing), in about a megabyte at the few hundred
/// bytes a notice takes, far below a frame's limit when written as one.
const EVENT_BACKLOG: usize = 4096;

#[derive(Default)]
struct Backlog {
    notices: Vec<Notice>,
    /// Notices were dropped; the writer says `lagged` after what it writes next.
    lagged: bool,
    /// The connection is gone; the writer stops.
    closed: bool,
}
type Queue = Arc<(Mutex<Backlog>, Condvar)>;
pub(super) struct EventSubscriber {
    queue: Queue,
}
impl EventSubscriber {
    /// Queues `notices`, or drops them all and marks the subscriber lagged if they would
    /// take its backlog past `EVENT_BACKLOG`; never waits for the subscriber.
    fn offer(&self, notices: &[Notice]) -> io::Result<()> {
        let (backlog, wake) = &*self.queue;
        let mut b = lock(backlog)?;
        if b.notices.len() + notices.len() > EVENT_BACKLOG {
            b.lagged = true;
        } else {
            b.notices.extend_from_slice(notices);
        }
        wake.notify_one();
        Ok(())
    }
}
impl Drop for EventSubscriber {
    fn drop(&mut self) {
        let (backlog, wake) = &*self.queue;
        if let Ok(mut b) = lock(backlog) {
            b.closed = true;
        }
        wake.notify_one();
    }
}

impl Host {
    /// What the screen says about trusting the folder; subscribers hear when it changes.
    pub(super) fn set_terminal_trust(
        &mut self,
        terminal: &str,
        prompt: Option<TrustPrompt>,
    ) -> io::Result<()> {
        let map = &mut self.recognition.terminal_trust;
        if map.get(terminal) == prompt.as_ref() {
            return Ok(());
        }
        let event = if prompt.is_some() {
            "trust"
        } else {
            "trust_gone"
        };
        match prompt {
            Some(p) => map.insert(terminal.to_owned(), p),
            None => map.remove(terminal),
        };
        let w = self.workspace.clone();
        map_retain(&mut self.recognition.terminal_trust, &w);
        self.publish_status()?;
        let at = mooring_core::content::now();
        self.send_events(vec![terminal_notice(event, &w, terminal, at)]);
        Ok(())
    }
    /// Queues `notices` for every subscriber; one that has fallen behind is marked and
    /// told `lagged` once it catches up, never waited for.
    pub(super) fn send_events(&self, notices: Vec<Notice>) {
        if notices.is_empty() {
            return;
        }
        for (connection, s) in &self.event_subscribers {
            if let Err(e) = s.offer(&notices) {
                eprintln!("events subscriber {connection}: {e}");
            }
        }
    }
}

/// Writes what is waiting, all of it as one frame, each time the queue wakes it.
fn write_events(mut writer: local::Stream, first: Vec<u8>, queue: &Queue) -> io::Result<()> {
    write_frame(&mut writer, CONTROL, &first)?;
    let (backlog, wake) = &**queue;
    loop {
        let (notices, lagged) = {
            let mut b = lock(backlog)?;
            while b.notices.is_empty() && !b.lagged && !b.closed {
                b = wake
                    .wait(b)
                    .map_err(|_| io::Error::other("events queue poisoned"))?;
            }
            if b.closed {
                return Ok(());
            }
            (
                std::mem::take(&mut b.notices),
                std::mem::take(&mut b.lagged),
            )
        };
        if !notices.is_empty() {
            let bytes = serde_json::to_vec(&Reply::Events(notices)).map_err(io::Error::other)?;
            write_frame(&mut writer, CONTROL, &bytes)?;
        }
        // Dropped events are not replayed; the client reads the snapshot again.
        if lagged {
            let notice = Notice::named("lagged", mooring_core::content::now());
            let bytes =
                serde_json::to_vec(&Reply::Events(vec![notice])).map_err(io::Error::other)?;
            write_frame(&mut writer, CONTROL, &bytes)?;
        }
    }
}

pub(super) fn subscribe(
    mut stream: local::Stream,
    shared: Shared,
    connection: u64,
) -> io::Result<()> {
    // A subscriber that stops reading is marked lagged, not cut by the connection's write
    // timeout in the middle of a frame; one that goes away fails the write at once.
    stream.set_write_timeout(None)?;
    let queue = Queue::default();
    let first = {
        let mut host = lock(&shared)?;
        let w = host.snapshot();
        if host.event_subscribers.is_empty() {
            host.events.seed(&w);
        }
        let first =
            serde_json::to_vec(&Reply::Events(Tracker::snapshot(&w))).map_err(io::Error::other)?;
        host.event_subscribers.insert(
            connection,
            EventSubscriber {
                queue: Arc::clone(&queue),
            },
        );
        first
    };
    let writer = stream.try_clone()?;
    let writer_guard = stream.try_clone()?;
    thread::spawn(move || {
        if let Err(e) = write_events(writer, first, &queue) {
            eprintln!("events stream ended: {e}");
        }
        let _ = writer_guard.shutdown(std::net::Shutdown::Both);
    });
    // A blocked read observes the client leaving without a heartbeat or timer.
    let result = read_frame(&mut stream);
    let mut host = lock(&shared)?;
    host.event_subscribers.remove(&connection);
    drop(host);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(s: &EventSubscriber) -> (usize, bool) {
        let b = lock(&s.queue.0).unwrap();
        (b.notices.len(), b.lagged)
    }
    /// A slow machine commits a burst one notice at a time; 3000 commits are still 3000
    /// notices, under the backlog, so a reader that is reading is not called lagged.
    #[test]
    fn the_backlog_counts_notices_not_commits() {
        let s = EventSubscriber {
            queue: Queue::default(),
        };
        let one = [Notice::named("command_done", 0)];
        for _ in 0..3000 {
            s.offer(&one).unwrap();
        }
        let (queued, lagged) = waiting(&s);
        assert_eq!((queued, lagged), (3000, false));
        s.offer(&vec![one[0].clone(); EVENT_BACKLOG - queued])
            .unwrap();
        assert_eq!(waiting(&s), (EVENT_BACKLOG, false));
        s.offer(&one).unwrap();
        assert_eq!(
            waiting(&s),
            (EVENT_BACKLOG, true),
            "one past the backlog is dropped"
        );
    }
}
