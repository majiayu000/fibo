//! Which space a conversation is listed under, for every surface: the window's sidebar,
//! board and filters, and the cards a host sends to phones and other machines.
use super::*;

impl Workspace {
    /// The space a session is listed under: its todo's; else its terminal's, or the space
    /// of the project it works in when that one sits inside the terminal's (an Agent opened
    /// in ~/code that works in a project with its own space); else, for one Mooring did not
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
