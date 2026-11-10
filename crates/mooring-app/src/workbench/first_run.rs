//! The first run: what someone who just installed Mooring sees where an empty tab would be,
//! until a session has run in one of its terminals or they close it. It says which Agent
//! CLIs the login shell finds, starts one in the shown space, and offers the hooks with the
//! file they change and how to take them out. Never shown again once closed or once a
//! session ran here; an install that already has hooks never sees it.
use super::*;
use crate::agent_hooks::{Hook, State};
use std::io;

/// `<config>/first-run-done`: written once the first run is over, so it stays over.
fn marker() -> Option<PathBuf> {
    crate::ui::status_colors::config_dir().map(|dir| dir.join("first-run-done"))
}
/// A session has run in a Mooring terminal: this install is in use.
pub(super) fn ran_here(w: &Workspace) -> bool {
    w.agent_sessions.iter().any(AgentSession::managed)
}
/// Whether to show it for `w`: not over, and nothing ran here. A marker that cannot be
/// checked counts as written: a returning user must never be greeted as a stranger.
pub(super) fn wanted(w: &Workspace, done: Option<io::Result<bool>>) -> bool {
    matches!(done, Some(Ok(false))) && !ran_here(w)
}
/// Some CLI already carries hooks of ours: someone installed them, so not a first run.
pub(super) fn hooks_installed(hooks: &[Hook]) -> bool {
    hooks
        .iter()
        .any(|h| matches!(h.state, State::Current | State::Outdated))
}

#[derive(Default)]
pub(super) struct FirstRun {
    pub(super) on: bool,
    /// The hooks as `mooring hooks status` read them; None while reading.
    hooks: Option<Result<Vec<Hook>, String>>,
    installing: bool,
}
impl FirstRun {
    pub(super) fn load(w: &Workspace) -> Self {
        let done = marker().map(|p| p.try_exists());
        Self {
            on: wanted(w, done),
            ..Default::default()
        }
    }
}
/// Install commands for the CLIs a stranger most likely wants, shown when not found.
fn install_hint(agent: Agent) -> Option<&'static str> {
    match agent.executable() {
        "claude" => Some("npm install -g @anthropic-ai/claude-code"),
        "codex" => Some("npm install -g @openai/codex"),
        _ => None,
    }
}

/// What the welcome page offers to start.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Offer {
    /// The login shell is being asked: nothing is listed until it answers, so the page
    /// never shows every CLI there is and then takes most of them away.
    pub(super) looking: bool,
    /// Started with one click, `first` at the front when it is among them.
    pub(super) start: Vec<Agent>,
    /// Not found, with an install command to show.
    pub(super) missing: Vec<Agent>,
}
/// Found by the login shell: those. It could not tell (no zsh or bash, or it hung): the two
/// a stranger most likely wants, offered anyway. CLIs without an install command are only
/// ever listed when found.
pub(super) fn offered(
    all: Vec<Agent>,
    found: Option<&HashSet<String>>,
    probing: bool,
    first: Agent,
) -> Offer {
    let mut offer = Offer::default();
    match found {
        Some(found) => {
            for agent in all {
                if found.contains(agent.executable()) {
                    offer.start.push(agent);
                } else if install_hint(agent).is_some() {
                    offer.missing.push(agent);
                }
            }
        }
        None if probing => offer.looking = true,
        None => {
            offer.start = all
                .into_iter()
                .filter(|a| install_hint(*a).is_some())
                .collect()
        }
    }
    if let Some(at) = offer.start.iter().position(|a| *a == first) {
        offer.start[..=at].rotate_right(1);
    }
    offer
}

impl Workbench {
    /// Asks the login shell for the Agent CLIs and `mooring hooks status` for the hooks.
    pub(super) fn start_first_run(&mut self, cx: &mut Context<Self>) {
        if !self.first_run.on {
            return;
        }
        self.probe_agents(cx);
        self.first_run_hooks(false, cx);
    }
    /// Reads the hooks again, installing the missing or outdated ones first when `install`
    /// (only ever from the button). Found installed on the first read, the run is over:
    /// this install was set up before, and gets the shell it always opened with.
    fn first_run_hooks(&mut self, install: bool, cx: &mut Context<Self>) {
        let pending: Vec<String> = match &self.first_run.hooks {
            Some(Ok(hooks)) if install => hooks
                .iter()
                .filter(|h| matches!(h.state, State::Missing | State::Outdated))
                .map(|h| h.agent.clone())
                .collect(),
            _ => vec![],
        };
        self.first_run.installing = install;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    for agent in &pending {
                        crate::agent_hooks::install(agent)?;
                    }
                    crate::agent_hooks::check()
                })
                .await
                .map_err(|e| e.to_string());
            let _ = this.update(cx, |this, cx| {
                let old = !install && result.as_deref().is_ok_and(hooks_installed);
                this.first_run.hooks = Some(result);
                this.first_run.installing = false;
                if old && this.first_run.on {
                    this.finish_first_run(cx);
                    if this.workspace.terminals.is_empty() && !this.nav.overview {
                        this.launch(false, None, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// Over for good. A marker that cannot be written only ends it for this run.
    pub(super) fn finish_first_run(&mut self, cx: &mut Context<Self>) {
        if !self.first_run.on {
            return;
        }
        self.first_run.on = false;
        if let Some(path) = marker() {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, "done\n"));
            if let Err(e) = written {
                eprintln!("first run marker {}: {e}", path.display());
            }
        }
        cx.notify();
    }
    /// After each snapshot: a session that ran here ends the first run.
    pub(super) fn first_run_follow(&mut self, cx: &mut Context<Self>) {
        if self.first_run.on && ran_here(&self.workspace) {
            self.finish_first_run(cx);
        }
    }
    pub(super) fn first_run_view(&self, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        let space = self
            .workspace
            .spaces
            .iter()
            .find(|s| s.id == self.nav.space);
        let place = space.map_or_else(|| "~".to_owned(), space_dir_label);
        let agents = self.first_run_agents(cx);
        let link = |id: &'static str, text: &'static str| {
            button(id, text)
                .border_0()
                .px(px(T::S4))
                .text_color(T::color(T::link()))
        };
        div()
            .id("first-run")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(T::S20))
            .child(
                div()
                    .w_full()
                    .max_w(px(T::HERE_WIDTH * 1.6))
                    .flex()
                    .flex_col()
                    .gap(px(T::S12))
                    .child(
                        div()
                            .text_size(px(T::FORM_TITLE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(T::color(T::primary()))
                            .child(t!("欢迎使用 Fibo")),
                    )
                    .child(label({
                        use mooring_core::status::Status;
                        t!("每个 Agent 都跑在这里的真终端里。在这里启动 Claude Code 或 Codex，左边就会列出这个会话和它现在的状态：{running}、{waiting}、{review}。", running = Status::Running.label(), waiting = Status::Waiting.label(), review = Status::Review.label())
                    }))
                    .child(section(
                        t!("启动一个 Agent"),
                        t!("在 {place} 打开；换文件夹用左边空间旁的 ＋", place),
                    ))
                    .child(agents)
                    .children(self.first_run_hooks_view(cx))
                    .child(
                        div()
                            .pt(px(T::S8))
                            .flex()
                            .items_center()
                            .gap(px(T::S4))
                            .text_size(px(T::SMALL))
                            .child(link("first-run-shell", t!("打开空终端")).on_click(
                                cx.listener(|this, _, _, cx| this.launch(false, None, cx)),
                            ))
                            .child(label("·"))
                            .child(
                                link("first-run-close", t!("不再显示")).on_click(
                                    cx.listener(|this, _, _, cx| this.finish_first_run(cx)),
                                ),
                            ),
                    ),
            )
    }
    /// The Agents to start, laid out as an empty tab lays them out: one solid button for
    /// the first, the others as quiet links that wrap, never a column of buttons.
    fn first_run_agents(&self, cx: &Context<Self>) -> gpui::Div {
        let offer = offered(
            Agent::all(),
            self.tasks.found.as_ref(),
            self.tasks.probing,
            self.first_agent(),
        );
        let mark = |agent: Agent, ink| super::board::agent_mark_in(agent, ink, T::SMALL);
        let id = |agent: Agent| format!("first-run-{}", agent.executable());
        let mut column = div().flex().flex_col().items_start().gap(px(T::S6));
        if offer.looking {
            column = column.child(label(t!("正在查找…")));
        }
        let mut start = offer.start.into_iter();
        if let Some(first) = start.next() {
            column = column.child(
                super::overview::solid(super::render::marked_button(
                    id(first),
                    t!("启动 {name}", name = first.name()),
                    Some(mark(first, T::color(T::on_accent()))),
                ))
                .px(px(T::S16))
                .on_click(cx.listener(move |this, _, _, cx| this.run_agent_here(first, cx))),
            );
        }
        let others: Vec<_> = start
            .map(|agent| {
                super::render::marked_button(
                    id(agent),
                    agent.name().to_owned(),
                    Some(mark(agent, T::color(T::link()))),
                )
                .px(px(T::S4))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(move |this, _, _, cx| this.run_agent_here(agent, cx)))
            })
            .collect();
        if !others.is_empty() {
            column = column.child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(T::S12))
                    .gap_y(px(T::S4))
                    .children(others),
            );
        }
        for agent in offer.missing {
            column = column.child(label(t!(
                "没在登录 shell 的 PATH 里找到 {executable}。安装：{hint}",
                executable = agent.executable(),
                hint = install_hint(agent).unwrap_or_default()
            )));
        }
        column
    }
    /// The hooks: which file, what they do, how to take them out, and one button.
    fn first_run_hooks_view(&self, cx: &Context<Self>) -> Option<gpui::Div> {
        let hooks = match &self.first_run.hooks {
            None => return Some(section("Hook", t!("正在检查…").to_owned())),
            Some(Err(e)) => return Some(section("Hook", t!("没能检查：{e}", e))),
            Some(Ok(hooks)) if hooks.is_empty() => return None,
            Some(Ok(hooks)) => hooks,
        };
        let files: Vec<String> = hooks.iter().map(|h| home_relative(&h.settings)).collect();
        let names: Vec<&str> = hooks.iter().map(|h| h.name.as_str()).collect();
        let all_current = hooks.iter().all(|h| h.state == State::Current);
        let unreadable = hooks.iter().find(|h| h.state == State::Unreadable);
        let mut body = div().flex().flex_col().gap(px(T::S6)).child(label(t!("推荐装上：{agents} 的状态和审批就不靠猜画面。会在 {files} 里加几条 Fibo 的条目，只在 Fibo 的终端里起作用，在别处什么都不做。撤销：设置 › 关于，或在 Fibo 终端里运行 fibo hooks uninstall。", agents = names.join(t!("、")), files = files.join(t!("、")))));
        let control = if all_current {
            label(t!("已安装")).into_any_element()
        } else if let Some(h) = unreadable {
            label(t!(
                "读不了 {file}：{why}",
                file = files[0],
                why = h.differences.join(t!("；"))
            ))
            .text_color(Tone::Danger.ink())
            .into_any_element()
        } else {
            let text = if self.first_run.installing {
                t!("正在安装…")
            } else {
                t!("安装 Hook")
            };
            button("first-run-hooks", text)
                .px(px(T::S16))
                .bg(T::color(T::button()))
                .text_color(T::color(T::primary()))
                .when(self.first_run.installing, |b| b.opacity(0.5))
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.first_run.installing {
                        this.first_run_hooks(true, cx);
                        cx.notify();
                    }
                }))
                .into_any_element()
        };
        body = body.child(div().flex().child(control));
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(T::S6))
                .child(section("Hook", String::new()))
                .child(body),
        )
    }
}
#[cfg(feature = "visual-test")]
impl Workbench {
    pub fn visual_first_run(&self) -> bool {
        self.first_run.on
    }
    /// The 「安装 Hook」 button's click.
    pub fn visual_first_run_install(&mut self, cx: &mut Context<Self>) {
        self.first_run_hooks(true, cx);
    }
    /// The 「启动 X」 button's click for the CLI named `executable`.
    pub fn visual_first_run_start(&mut self, executable: &str, cx: &mut Context<Self>) {
        if let Some(agent) = Agent::from_executable(executable) {
            self.run_agent_here(agent, cx);
        }
    }
}
/// A section's title with a note after it.
fn section(title: &'static str, note: String) -> gpui::Div {
    div()
        .pt(px(T::S8))
        .flex()
        .items_baseline()
        .gap(px(T::S8))
        .child(
            div()
                .text_size(px(T::BODY_SIZE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(T::color(T::primary()))
                .child(title),
        )
        .when(!note.is_empty(), |d| d.child(label(note)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(state: State) -> Hook {
        Hook {
            agent: "claude".into(),
            name: "Claude Code".into(),
            settings: "/h/.claude/settings.json".into(),
            state,
            differences: vec![],
        }
    }

    #[test]
    fn only_a_fresh_install_is_greeted() {
        let fresh = Workspace::default();
        assert!(wanted(&fresh, Some(Ok(false))));
        // Closed before, or the marker cannot be checked: a returning user is never greeted.
        assert!(!wanted(&fresh, Some(Ok(true))));
        assert!(!wanted(&fresh, Some(Err(io::Error::other("denied")))));
        assert!(!wanted(&fresh, None));
        // A session that ran in a Mooring terminal: in use.
        let mut used = super::super::tests::fixture();
        assert!(used.agent_sessions.iter().any(AgentSession::managed));
        assert!(!wanted(&used, Some(Ok(false))));
        // Conversations from outside Mooring only (sessions the host read from the CLIs'
        // own files): still a stranger to Mooring.
        for s in &mut used.agent_sessions {
            s.terminal = None;
        }
        assert!(wanted(&used, Some(Ok(false))));
    }

    fn names(agents: &[Agent]) -> Vec<&'static str> {
        agents.iter().map(|a| a.executable()).collect()
    }
    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn only_what_the_shell_finds_is_offered_and_nothing_while_it_is_asked() {
        let all = Agent::all();
        let first = Agent::from_executable("codex").unwrap();
        // Being asked: one 「正在查找…」, not every CLI there is.
        let asking = offered(all.clone(), None, true, first);
        assert!(asking.looking);
        assert!(asking.start.is_empty() && asking.missing.is_empty());
        // Answered: the found ones, the one used last in front; Claude Code is missing and
        // has an install command, the other missing ones are not mentioned.
        let found = set(&["codex", "gemini"]);
        let answered = offered(all.clone(), Some(&found), false, first);
        assert!(!answered.looking);
        assert_eq!(names(&answered.start)[0], "codex");
        assert_eq!(answered.start.len(), 2);
        assert_eq!(names(&answered.missing), ["claude"]);
        // It could not tell: the two with install commands, offered anyway.
        let unknown = offered(all, None, false, first);
        assert!(!unknown.looking && unknown.missing.is_empty());
        assert_eq!(names(&unknown.start), ["codex", "claude"]);
    }

    #[test]
    fn an_answer_ends_the_asking_so_the_page_is_never_left_looking() {
        let asked = vec!["claude".to_owned(), "codex".to_owned()];
        // No answer (no zsh or bash, or it hung): the asking is still over.
        let mut tasks = super::super::tasks::TaskUi {
            probing: true,
            ..Default::default()
        };
        assert!(!tasks.probe_answered(&asked, &asked, None));
        assert!(!tasks.probing && tasks.found.is_none());
        // Plugins were read again meanwhile and declare another CLI: this answer is for
        // the old list, so it is dropped and the shell is asked again.
        tasks.probing = true;
        let now = vec![
            "claude".to_owned(),
            "codex".to_owned(),
            "opencode".to_owned(),
        ];
        assert!(tasks.probe_answered(&asked, &now, Some(set(&["claude"]))));
        assert!(!tasks.probing && tasks.found.is_none());
        tasks.probing = true;
        assert!(!tasks.probe_answered(&now, &now, Some(set(&["claude"]))));
        assert_eq!(tasks.found, Some(set(&["claude"])));
    }

    #[test]
    fn hooks_already_there_mean_an_older_install() {
        assert!(!hooks_installed(&[hook(State::Missing)]));
        assert!(!hooks_installed(&[hook(State::Unreadable)]));
        assert!(hooks_installed(&[hook(State::Current)]));
        assert!(hooks_installed(&[
            hook(State::Missing),
            hook(State::Outdated)
        ]));
    }
}
