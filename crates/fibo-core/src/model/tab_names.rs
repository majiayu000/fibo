//! What a tab is called. The window names every tab each second and on every change, and
//! naming one used to walk every session several times over for each earlier tab of its
//! space (15 tabs and 2000 sessions: about 8 ms a second). The sessions are now walked
//! once per naming, into each terminal's current ones; the rules are unchanged.
use super::*;
use rustc_hash::FxHashMap;

/// Each terminal's current sessions, in workspace order.
struct Current<'a>(FxHashMap<&'a str, Vec<(usize, &'a AgentSession)>>);

impl<'a> Current<'a> {
    fn of(w: &'a Workspace) -> Self {
        let mut by: FxHashMap<&str, Vec<(usize, &AgentSession)>> = FxHashMap::default();
        for (i, s) in w.agent_sessions.iter().enumerate() {
            if s.current
                && let Some(terminal) = s.terminal.as_deref()
            {
                by.entry(terminal).or_default().push((i, s));
            }
        }
        Self(by)
    }
    /// The current sessions on any of `ids`, in workspace order, as a scan of them finds them.
    fn on(&self, ids: &[String]) -> Vec<&'a AgentSession> {
        let mut found: Vec<(usize, &AgentSession)> = ids
            .iter()
            .filter_map(|id| self.0.get(id.as_str()))
            .flatten()
            .copied()
            .collect();
        found.sort_unstable_by_key(|(i, _)| *i);
        found.dedup_by_key(|(i, _)| *i);
        found.into_iter().map(|(_, s)| s).collect()
    }
    /// `Workspace::current_agent`.
    fn first(&self, terminal: &str) -> Option<&'a AgentSession> {
        self.0.get(terminal)?.first().map(|(_, s)| *s)
    }
}

impl Workspace {
    /// Like Warp: the name you gave it, then what runs in it (todo, conversation,
    /// the title the program set), then the program itself, numbered among same-named
    /// tabs. The space's directory is not a tab name; every tab in one space shares it.
    /// `titles` holds every terminal's program title, so a tab keeps its name while
    /// another tab is in front; `focused` breaks ties between a tab's own panes.
    pub fn tab_title(
        &self,
        tab: &Tab,
        titles: &std::collections::HashMap<String, String>,
        focused: Option<&str>,
    ) -> String {
        self.tab_title_in(&Current::of(self), tab, titles, focused)
    }
    /// `tab_title` of every tab, in order, walking the sessions once.
    pub fn tab_titles(
        &self,
        titles: &std::collections::HashMap<String, String>,
        focused: Option<&str>,
    ) -> Vec<String> {
        let current = Current::of(self);
        self.tabs
            .iter()
            .map(|tab| self.tab_title_in(&current, tab, titles, focused))
            .collect()
    }
    fn tab_title_in(
        &self,
        current: &Current,
        tab: &Tab,
        titles: &std::collections::HashMap<String, String>,
        focused: Option<&str>,
    ) -> String {
        if let Some(title) = self.tab_given_title(current, tab, titles, focused) {
            return title;
        }
        let name = self.tab_program(current, tab);
        let same = self
            .tabs
            .iter()
            .filter(|t| t.space == tab.space)
            .take_while(|t| t.id != tab.id)
            .filter(|t| {
                self.tab_given_title(current, t, titles, None).is_none()
                    && self.tab_program(current, t) == name
            })
            .count();
        if same == 0 {
            name
        } else {
            format!("{name} {}", same + 1)
        }
    }
    fn tab_given_title(
        &self,
        current: &Current,
        tab: &Tab,
        titles: &std::collections::HashMap<String, String>,
        focused: Option<&str>,
    ) -> Option<String> {
        let ids = self.tab_terminals(&tab.id);
        // A tab holding one conversation is called what the conversation is called,
        // everywhere the same: `session_name` is the one owner of that name.
        let sessions = current.on(&ids);
        if let [only] = sessions.as_slice()
            && let Some(name) = self.session_name(only)
        {
            return Some(name);
        }
        if !tab.title.trim().is_empty() {
            return Some(tab.title.clone());
        }
        if sessions.is_empty()
            && let Some(task) = tab
                .default_task
                .as_ref()
                .and_then(|id| self.tasks.iter().find(|t| &t.id == id))
        {
            return Some(task.title.clone());
        }
        if let Some(first) = sessions.first().and_then(|s| s.task.as_ref())
            && sessions.iter().all(|s| s.task.as_ref() == Some(first))
            && let Some(task) = self.tasks.iter().find(|t| &t.id == first)
        {
            return Some(task.title.clone());
        }
        if let Some(title) = sessions
            .iter()
            .filter_map(|s| Some((self.session_updated(s), self.session_name(s)?)))
            .max_by_key(|(updated, _)| *updated)
            .map(|(_, t)| t)
        {
            return Some(title);
        }
        let mut ids = ids;
        if let Some(i) = ids.iter().position(|id| Some(id.as_str()) == focused) {
            ids.swap(0, i);
        }
        ids.iter()
            .filter_map(|id| titles.get(id))
            .find(|t| !t.trim().is_empty())
            .cloned()
    }
    /// What runs in the tab's first pane: its agent, else its program ("zsh").
    fn tab_program(&self, current: &Current, tab: &Tab) -> String {
        let ids = self.tab_terminals(&tab.id);
        let Some(id) = ids.first() else {
            return t!("终端").into();
        };
        if let Some(session) = current.first(id) {
            return session.runtime.name().into();
        }
        self.terminals
            .iter()
            .find(|t| &t.id == id)
            .and_then(|t| t.argv.first())
            .and_then(|program| program.rsplit(['/', '\\']).next())
            .map(|program| program.trim_start_matches('-'))
            // `powershell.exe` is named `powershell`, as `/bin/zsh` is `zsh`.
            .map(|program| {
                let len = program.len();
                match program.get(len.saturating_sub(4)..) {
                    Some(ext) if len > 4 && ext.eq_ignore_ascii_case(".exe") => &program[..len - 4],
                    _ => program,
                }
            })
            .filter(|program| !program.is_empty())
            .unwrap_or(t!("终端"))
            .into()
    }
}
