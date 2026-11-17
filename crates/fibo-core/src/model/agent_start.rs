//! What the host started a terminal's Agent with, kept as evidence of which conversation
//! the terminal runs until it runs one.
use super::*;

/// How the host was asked to start a terminal's Agent (a phone's 新任务, 接着做 or a todo's
/// 开始, the window's resume): the CLI, the first message its command line hands over, or
/// the conversation it picks back up, in that CLI's own id. A CLI that names no conversation
/// of its own (OpenCode, a shared-daemon Codex) says nothing about which one this terminal
/// runs; the host does, since it started it (`fibo-host` `recognition::launched`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStart {
    pub agent: Agent,
    /// The first message, exactly as the command line hands it to the CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// The conversation it resumes, in the CLI's own id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    /// When the host started it, seconds since the epoch. The host sets it, whatever the
    /// caller sent: only a conversation begun after it can be the one started.
    #[serde(default)]
    pub at: u64,
}

impl AgentStart {
    pub fn new(agent: Agent, prompt: Option<String>) -> Self {
        Self {
            agent,
            prompt,
            resume: None,
            at: 0,
        }
    }
    pub fn resuming(agent: Agent, cli_id: &str) -> Self {
        Self {
            resume: Some(cli_id.to_owned()),
            ..Self::new(agent, None)
        }
    }
}

impl Workspace {
    /// Lets go of the starts that have done their part: their terminal runs a conversation
    /// now, whatever tied it, or has ended. What its Agent was started with says which
    /// conversation came first in it, never which one runs there later.
    pub fn settle_starts(&mut self) {
        if self.agent_starts.is_empty() {
            return;
        }
        let Self {
            agent_starts,
            terminals,
            agent_sessions,
            ..
        } = self;
        agent_starts.retain(|terminal, _| {
            let live = terminals.iter().any(|t| {
                &t.id == terminal
                    && matches!(t.state, ProcessState::Starting | ProcessState::Running)
            });
            live && !agent_sessions
                .iter()
                .any(|s| s.current && s.terminal.as_ref() == Some(terminal))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal(id: &str, state: ProcessState) -> Terminal {
        Terminal {
            id: id.into(),
            space: "s".into(),
            argv: vec!["sh".into()],
            cwd: "/w".into(),
            pid: Some(1),
            state,
            exit_code: None,
            error: None,
        }
    }

    #[test]
    fn a_start_lasts_until_its_terminal_runs_a_conversation_or_ends() {
        let mut w = Workspace::default();
        for (id, state) in [
            ("waiting", ProcessState::Running),
            ("starting", ProcessState::Starting),
            ("bound", ProcessState::Running),
            ("ended", ProcessState::Exited),
        ] {
            w.terminals.push(terminal(id, state));
            w.agent_starts
                .insert(id.into(), AgentStart::new(Agent::Codex, Some("x".into())));
        }
        w.agent_starts
            .insert("gone".into(), AgentStart::resuming(Agent::Claude, "c1"));
        w.agent_sessions.push(AgentSession {
            id: "a".into(),
            runtime: Agent::Codex,
            cli_session_id: "t".into(),
            terminal: Some("bound".into()),
            current: true,
            task: None,
            outcome: Outcome::Open,
            updated: 1,
            cwd: "/w".into(),
            transcript: "/l.jsonl".into(),
            name: String::new(),
            binding: Some(Binding::Hook),
        });
        w.settle_starts();
        let mut left: Vec<_> = w.agent_starts.keys().cloned().collect();
        left.sort();
        assert_eq!(left, ["starting", "waiting"]);
    }

    /// A workspace an older host wrote has no starts; one this host writes reads back.
    #[test]
    fn starts_are_optional_in_what_is_stored_and_sent() {
        let old: Workspace = serde_json::from_value(serde_json::json!({
            "revision": 1, "spaces": [], "tasks": [], "tabs": [], "terminals": [],
            "agent_sessions": [], "content": {}
        }))
        .unwrap();
        assert!(old.agent_starts.is_empty());
        let start = AgentStart {
            at: 7,
            ..AgentStart::new(Agent::Codex, Some("两行\n任务".into()))
        };
        let back: AgentStart =
            serde_json::from_str(&serde_json::to_string(&start).unwrap()).unwrap();
        assert_eq!(back, start);
        let resumed: AgentStart =
            serde_json::from_str(r#"{"agent":"opencode","resume":"ses_1"}"#).unwrap();
        assert_eq!((resumed.prompt, resumed.at), (None, 0));
    }
}
