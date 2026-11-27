//! Native control requests and the hook facts they carry.
use super::*;
use crate::model::Edit;

/// The few hook payload facts the status rules read. Prompt and reply text stay out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookFacts {
    /// From a Stop or SubagentStop payload that carried the task registry.
    pub background: Option<crate::content::Background>,
    /// Exact workflow task registry when a hook supplied it; absence is unknown.
    #[serde(default)]
    pub workflow_tasks: Option<Vec<String>>,
    pub notification: Option<String>,
    /// The event came from inside a subagent (`agent_id` present), not the parent turn.
    pub subagent: bool,
    /// The running background shells a Stop or SubagentStop listed. Their commands only
    /// ride to the host, which matches them to processes; the stored card keeps the
    /// descriptions alone, since a command line may hold a secret.
    #[serde(default)]
    pub shells: Vec<HookShell>,
    /// A question tool's question and option labels, from a PermissionRequest's
    /// `tool_input`: Claude Code writes the call to its log only once it is answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<(String, Vec<String>)>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookShell {
    pub id: String,
    pub description: String,
    pub command: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Hello {
        major: u32,
    },
    /// From a newer host: hand over every running terminal, then exit.
    Handoff,
    Workspace,
    ClientLeft {
        at: u64,
    },
    ClaimNotification {
        session: String,
        key: String,
    },
    RefreshAgents,
    /// Plugins were installed, removed or switched: read them again. The host swaps its
    /// rules and hooks; agents stay as registered at its start.
    ReloadPlugins,
    RecognitionStatus,
    WatchRecognition,
    /// A fifteen-second measurement stream, cancelled by disconnecting this socket.
    Diagnose,
    /// Scan only repositories reached from the local host's workspace.
    ScanWorktrees,
    /// Act only if this terminal is still alive and outside every layout.
    RecoverTerminal {
        id: String,
        place: bool,
    },
    ReclaimWorktree {
        path: String,
        head: String,
        entire: bool,
    },
    Hook {
        runtime: crate::model::Agent,
        pid: u32,
        cli_session_id: String,
        transcript: String,
        ended: bool,
        event: Option<String>,
        #[serde(default)]
        facts: HookFacts,
        /// The terminal the hook's environment names (`TERMINAL_ENV`); absent outside one.
        #[serde(default)]
        terminal: Option<String>,
        /// The working directory the hook reports; stands in until the transcript exists.
        #[serde(default)]
        cwd: String,
    },
    /// A change to the workspace. With `request` it takes effect once however often it
    /// is sent: a resend gets the first outcome back (the workspace if it was applied,
    /// the same error if it was refused), across disconnects and host restarts.
    Edit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<RequestId>,
        edit: Edit,
    },
    Launch {
        id: String,
        space: String,
        argv: Vec<String>,
        cwd: String,
        cols: u16,
        rows: u16,
        /// Hosts before it start every terminal dark.
        #[serde(default)]
        scheme: crate::terminal::Scheme,
        /// A user's theme; hosts before it read `scheme`, the nearest built-in.
        #[serde(default)]
        colors: Option<crate::terminal::TermColors>,
        /// The Agent `argv` starts and with what: kept as evidence of the conversation it
        /// runs (`Workspace::agent_starts`). Hosts before it ignore it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<crate::model::AgentStart>,
    },
    Attach {
        id: String,
        takeover: bool,
        /// Only looks: never becomes the writer and never sizes the terminal, however
        /// many writers come and go (`fibo remote`'s watch). Hosts before it ignore it.
        #[serde(default)]
        watch: bool,
    },
    Input {
        bytes: Vec<u8>,
        /// Mouse reports: dropped while the program is behind on reading, so a full
        /// input queue never cuts one in half. Hosts before it write them all.
        #[serde(default)]
        lossy: bool,
    },
    Resize {
        cols: u16,
        rows: u16,
        /// One cell in device pixels, for size reports and the pty's pixel size; 0 if unknown.
        #[serde(default)]
        cell_width: u32,
        #[serde(default)]
        cell_height: u32,
    },
    /// The window switched between light and dark: repaint this terminal's defaults.
    Scheme(crate::terminal::Scheme),
    /// The same for a user's theme. Sent only to a host of this build: one before it
    /// would drop the connection on a request it cannot read.
    Colors(crate::terminal::TermColors),
    /// Cmd+K: drop scrollback and the rows above the cursor (no-op in the alternate screen).
    ClearHistory,
    /// The request, its workspace reply as a `WorkspaceDelta` against the stamp given.
    Brief(Option<u64>, Box<Request>),
    /// As `Brief`, and of the rest of the workspace only the fields that changed go.
    BriefAll(Option<u64>, Box<Request>),
    TerminalCwd {
        id: String,
    },
    StopAgent {
        id: String,
    },
    Stop {
        id: String,
    },
    /// Reads the next stretch of a conversation's log that its first read skipped, on a thread
    /// and a connection of its own at low priority, so a long log holds nothing else up.
    Backfill {
        session: String,
    },
    /// A phone's chat view: one conversation's user and Agent messages from the index,
    /// oldest first. `after` gives those newer than a `seq`; otherwise the newest `limit`,
    /// or those older than `before`.
    Chat {
        session: String,
        #[serde(default)]
        after: Option<i64>,
        #[serde(default)]
        before: Option<i64>,
        limit: usize,
        /// Each Agent message also carries its [`ChatTools`].
        #[serde(default)]
        tools: bool,
    },
    /// A phone opening a stretch of work: the tool calls logged before the Agent message
    /// `seq` and after the message before it (no `seq`: after the last message), oldest
    /// first, from past the `after` cursor. Rows only: their raw lines are the caller's to
    /// read, without the host's lock.
    ToolCalls {
        session: String,
        #[serde(default)]
        seq: Option<i64>,
        #[serde(default)]
        after: Option<i64>,
        limit: usize,
        #[serde(default)]
        last: Option<usize>,
        /// With no `seq`: the stretch that ended where the turn was last stopped, not the one
        /// after it (empty until it answers). A host before this reads it as absent.
        #[serde(default)]
        stopped: bool,
    },
    /// One tool call by its id, and where its raw lines are.
    ToolCall {
        session: String,
        call: String,
    },
    /// The files this session's current turn changed, read from its log when asked.
    TurnChanges {
        session: String,
    },
    /// What the turn did to one of them, read from the places `TurnChanges` named.
    TurnFile {
        session: String,
        log: u64,
        path: String,
        lines: Vec<u64>,
    },
    /// Scripts and other Agents: the last `rows` rows of a terminal as plain text.
    ReadScreen {
        id: String,
        rows: usize,
    },
    /// Which conversation an unbound Agent's screen shows, tied to it when beyond doubt:
    /// [`Reply::ScreenAssociated`].
    AssociateScreen {
        id: String,
    },
    /// What the Agent in a terminal offers as your next prompt: [`Reply::InputSuggestion`].
    InputSuggestion {
        id: String,
    },
    /// `fibo run`, just before it types its task in: `start`'s first message is the one the
    /// terminal's Agent gets now, kept as `Workspace::agent_starts` the way a `Launch`'s start
    /// is. Not at launch: a run that never hands its task over has started nothing. Ignored
    /// once the terminal runs a conversation; hosts before it refuse it.
    HandingOver {
        id: String,
        start: crate::model::AgentStart,
    },
    /// Scripts and other Agents: text placed in a terminal as a paste, then Enter when
    /// `submit`. Goes in beside whoever is typing there.
    SendText {
        /// Restricts a chat message to this current conversation; None permits shell input.
        #[serde(default)]
        session: Option<String>,
        id: String,
        text: String,
        submit: bool,
    },
    /// A guarded chat message using the Agent's queue or steer key.
    SendTurn(TurnInput),
    /// A picture another computer's window was given for a terminal here (pasted or
    /// dropped), PNG, JPEG, GIF or WebP as base64: kept on this machine as a phone's is.
    /// Replies `Text`, its path here, for the window to paste. Hosts before it refuse it.
    SaveImage {
        data: String,
    },
    /// `fibo run`: each time the Agent in `child` ends a turn (done, a question, an
    /// error), its last reply goes into `parent`'s Agent input, wrapped as
    /// `<fibo from=…>`, once that Agent is idle. Ends when either terminal exits.
    ReportTo {
        child: String,
        parent: String,
    },
    /// The workbench's 允许一次 / 拒绝 on an approval the screen shows. The host reads
    /// the screen again and answers only while that same approval is still there.
    AnswerApproval {
        id: String,
        question: String,
        allow: bool,
    },
    /// As `AnswerApproval` allowing, but takes the menu's 「don't ask again」 option
    /// (this session, this command); refused when the menu shows none.
    AnswerAlways {
        id: String,
        question: String,
    },
    /// Answers an Agent's 「trust this folder?」, read off the screen before and after the
    /// keys go out; refused when that prompt about `folder` is not what it shows.
    AnswerTrust {
        id: String,
        folder: String,
        trust: bool,
    },
    /// Interrupts the turn of the conversation in the terminal, with the key its CLI takes
    /// for that (`Adapter::interrupt_key`), only while it is working: what its turn was at
    /// that moment decides, not what a client saw earlier. The window's 中断这一轮 and a
    /// phone's 停下 are both this and [`Request::AwaitInterrupt`].
    Interrupt {
        id: String,
    },
    /// After [`Request::Interrupt`]: replies [`Reply::Interrupted`] once the state says the
    /// turn ended, with the Agent still there, and closes a command the turn left running.
    /// Refused if it does not end within seconds or the Agent left. A key sent is not a turn
    /// stopped. Outside `Once`: its one lock would hold every other answer up meanwhile.
    AwaitInterrupt {
        id: String,
    },
    Modes(crate::modes::ModeRequest),
    /// Typing, keys or an approval answer (`SendText`, `SendKeys`, `SendInput`,
    /// `AnswerApproval`, `AnswerAlways`, `AnswerTrust`, `Interrupt`) run
    /// at most once under `request`: a resend after a lost reply gets the first outcome
    /// back (a phone or another computer that reconnected). Edits name themselves.
    Once {
        request: RequestId,
        inner: Box<Request>,
    },
    /// Named keys (`enter`, `esc`, `up`, `ctrl-c`, a single character…) typed into a
    /// terminal in order, beside whoever is typing there; arrows follow the program's
    /// cursor-key mode. For clients with no keyboard of their own, such as a phone.
    SendKeys {
        id: String,
        keys: Vec<String>,
    },
    /// Bytes as a terminal emulator encodes keys and pastes, typed into a terminal beside
    /// whoever types there: another desktop's pane on this machine (`fibo remote`).
    SendInput {
        id: String,
        bytes: Vec<u8>,
    },
    /// Scripts, other Agents and other clients: the sessions' states now, then every
    /// change as it happens, until this socket closes (`fibo events`).
    Subscribe,
    /// `fibo split` asks before it starts anything: `Size`, the new pane's cells,
    /// when the pane, at the size it is shown now, has room for both halves of that split.
    SplitRoom {
        target: String,
        axis: crate::layout::Axis,
        share: u16,
    },
    /// Turns phones on a relay on or off, or moves them to another relay. The host keeps
    /// `fibo remote --relay` running while it is on.
    RemoteConfigure {
        enabled: bool,
        relay: String,
        /// The relay's registration key: None keeps the one saved, empty clears it.
        key: Option<String>,
        /// More relays the machine stays on beside `relay`: None keeps the saved list.
        /// A relay listed without a key keeps the key saved for it.
        #[serde(default)]
        also: Option<Vec<RemoteRelay>>,
    },
    /// Pairs one more phone; replies `Text`, its pairing link.
    RemotePair {
        name: String,
        /// view, answer or full.
        role: String,
    },
    RemoteDevice {
        device: String,
        change: RemoteChange,
    },
    /// From `fibo remote --relay`: who is on now; the host passes it to the app.
    RemoteReport(RemoteLive),
}
