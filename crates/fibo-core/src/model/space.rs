//! Which space a conversation is listed under, for every surface: the window's sidebar,
//! board and filters, and the cards a host sends to phones and other machines.
use super::*;

impl Workspace {
    /// The space a session is listed under: its todo's; else its terminal's, or the space
    /// of the project it works in when that one sits inside the terminal's (an Agent opened
    /// in ~/code that works in a project with its own space); else, for one Fibo did not
    /// open, the deepest space holding its folder.
    pub fn session_space(&self, s: &AgentSession) -> Option<&str> {
        s.task
            .as_ref()
            .and_then(|id| self.tasks.iter().find(|t| &t.id == id))
            .map(|t| t.space.as_str())
            .or_else(|| {
                let space = s
                    .terminal
                    .as_ref()
                    .and_then(|id| self.terminals.iter().find(|t| &t.id == id))
                    .map(|t| t.space.as_str())?;
                Some(narrower_space(&self.spaces, space, &s.cwd).map_or(space, |n| n.id.as_str()))
            })
            .or_else(|| {
                self.spaces
                    .iter()
                    .filter(|p| std::path::Path::new(&s.cwd).starts_with(&p.cwd))
                    .max_by_key(|p| p.cwd.len())
                    .map(|p| p.id.as_str())
            })
    }
    /// Where a new terminal in a space starts when no pane or conversation says: the
    /// space's own folder; else where its terminals last were; else "" (home).
    pub fn space_start_dir(&self, id: &str) -> &str {
        let pinned = (self.spaces.iter().find(|s| s.id == id)).map_or("", |s| s.cwd.as_str());
        if !pinned.is_empty() {
            return pinned;
        }
        self.space_last_cwd.get(id).map_or("", String::as_str)
    }
    /// Folders worked in lately, each once, newest first: where conversations ran, then
    /// spaces' folders and where their terminals last were. Whether one still exists is
    /// the caller's to check.
    pub fn recent_dirs(&self) -> Vec<&str> {
        let mut ran: Vec<(u64, &str)> = (self.agent_sessions.iter())
            .map(|s| (self.session_updated(s), s.cwd.as_str()))
            .collect();
        ran.sort_by_key(|r| std::cmp::Reverse(r.0));
        let spaces = (self.spaces.iter().map(|s| s.cwd.as_str()))
            .chain(self.space_last_cwd.values().map(String::as_str));
        let mut seen = std::collections::HashSet::new();
        (ran.into_iter().map(|(_, dir)| dir).chain(spaces))
            .map(|d| d.trim_end_matches('/'))
            .filter(|d| !d.is_empty() && seen.insert(*d))
            .collect()
    }
    /// Folders a space could start in, newest first: `recent_dirs` that still exist, home
    /// and `/` left out (a space with nothing better starts in home anyway).
    pub fn offered_dirs(&self, home: Option<&str>, limit: usize) -> Vec<String> {
        let home = home.map(|h| h.trim_end_matches('/'));
        (self.recent_dirs().into_iter())
            .filter(|d| Some(*d) != home && !d.is_empty())
            .filter(|d| std::path::Path::new(d).is_dir())
            .take(limit)
            .map(str::to_owned)
            .collect()
    }
    /// Where a new terminal in a space starts when no pane says: its own folder; else
    /// where its own terminals last were; else the folder you worked in last anywhere, so
    /// a new space does not start in home; "" is home.
    pub fn space_start(&self, id: &str, home: Option<&str>) -> String {
        match self.space_start_dir(id) {
            "" => self.offered_dirs(home, 1).pop().unwrap_or_default(),
            own => own.to_owned(),
        }
    }
    /// Whether you put this space aside (先不管).
    pub fn space_quiet(&self, id: &str) -> bool {
        self.quiet_spaces.contains(id)
    }
    /// The space whose 先不管 covers a session: the tab it runs in, which is what is in
    /// front of you, whatever todo or folder it is listed under; without a tab, where it
    /// is listed.
    pub fn session_home(&self, s: &AgentSession) -> Option<&str> {
        (s.terminal.as_ref())
            .and_then(|t| self.terminal_tab(t))
            .map(|tab| tab.space.as_str())
            .or_else(|| self.session_space(s))
    }
    /// Whether a space you put aside covers the session. Every surface that counts or
    /// shows sessions asks this, through `status::needs_you`, `status_group::column` or
    /// the window's `session_hidden`.
    pub fn session_quiet(&self, s: &AgentSession) -> bool {
        !self.quiet_spaces.is_empty()
            && self
                .session_home(s)
                .is_some_and(|space| self.space_quiet(space))
    }
    /// The space a terminal is open in: its tab's, else the one it was started in.
    pub fn terminal_home(&self, id: &str) -> Option<&str> {
        (self.terminal_tab(id).map(|tab| tab.space.as_str()))
            .or_else(|| (self.terminals.iter().find(|t| t.id == id)).map(|t| t.space.as_str()))
    }
    /// The same for a terminal, with or without a conversation.
    pub fn terminal_quiet(&self, id: &str) -> bool {
        !self.quiet_spaces.is_empty()
            && self
                .terminal_home(id)
                .is_some_and(|space| self.space_quiet(space))
    }
    /// How many spaces are not put aside.
    pub fn attended_spaces(&self) -> usize {
        (self.spaces.iter())
            .filter(|s| !self.space_quiet(&s.id))
            .count()
    }
}
