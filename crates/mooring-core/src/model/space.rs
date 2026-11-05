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
}
