//! Fixture sessions and tasks the visual pages draw.
use super::*;

impl Workbench {
    /// Sessions in space s0 shaped like sidebar-density.html's: two need you (one asks,
    /// one failed), two run, four are done, three stopped and a few went quiet, over five
    /// folders. Kept in memory only, in a tab of their own, and the revision is pushed
    /// past the host's so its snapshots leave them be.
    pub(super) fn visual_dense_fixture(&mut self) {
        use mooring_core::content::{Content, Evidence, Quote, Source, Turn};
        if self.workspace.terminals.iter().any(|t| t.id == "dense-t0") {
            return;
        }
        // A fresh runtime has no session to copy: start from a bare Claude one.
        let template = self
            .workspace
            .agent_sessions
            .first()
            .cloned()
            .unwrap_or_else(|| AgentSession {
                id: String::new(),
                runtime: Agent::Claude,
                cli_session_id: String::new(),
                terminal: None,
                current: false,
                task: None,
                outcome: Outcome::Open,
                updated: 0,
                cwd: String::new(),
                transcript: String::new(),
                name: String::new(),
                binding: None,
            });
        let Some(shell) = self.workspace.terminals.first().cloned() else {
            return;
        };
        // Only these sessions in the column: the fixture's own step aside.
        for c in self.workspace.content.values_mut() {
            c.hidden_reason = "visual".into();
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let dir = |d: &str| match d {
            "knowledge-base" => format!("{home}/Documents/knowledge-base"),
            "grok-build" => format!("{home}/Desktop/code/AI/grok-build"),
            d => format!("{home}/Desktop/code/AI/tools/{d}"),
        };
        let now = now();
        // (title, folder, turn, live, seconds ago, what it says)
        let rows: Vec<(&str, &str, Turn, bool, u64, &str)> = vec![
            (
                "这个终止 agent 中断取消",
                "mooring",
                Turn::Error,
                false,
                240,
                "managed_session 测试超时：timed out waiting for Settled after 30s",
            ),
            (
                "这个目录的 grok 的聊天记录",
                "grok-build",
                Turn::Waiting,
                true,
                720,
                "找不到 grok 的会话目录，要我去 ~/.grok 下面再找一遍吗？",
            ),
            (
                "这个诊断的 ui 感觉很丑啊",
                "mooring",
                Turn::Running,
                true,
                2467,
                "S=/private/tmp/claude-502/-Users-lifcc-Desktop; cd \"$S\" && cargo build -p mooring-app 2>&1 | tail -20",
            ),
            (
                "标签命名依旧有问题",
                "mooring",
                Turn::Running,
                true,
                192,
                "文件修改：next/crates/mooring-app/src/workbench/tabs.rs",
            ),
            (
                "左侧的会话 有什么好的命名",
                "mooring",
                Turn::Completed,
                true,
                7200,
                "标签用第一句话命名",
            ),
            (
                "这个 mooring 的 gpu 占用",
                "mooring",
                Turn::Completed,
                true,
                10800,
                "只重画变化的区域",
            ),
            (
                "为什么有的会话 在 mooring 里",
                "cli",
                Turn::Completed,
                true,
                18000,
                "外部会话标为只读",
            ),
            (
                "cli 是否支持 workflow 呢",
                "cli",
                Turn::Completed,
                true,
                86400,
                "说明了 workflow 命令",
            ),
            (
                "我准备 重启一下 mac mooring",
                "mooring",
                Turn::Interrupted,
                false,
                7200,
                "等你确认是否重启",
            ),
            (
                "这个库 有什么可以优化的",
                "bench",
                Turn::Interrupted,
                false,
                21600,
                "列了 12 条，没开始改",
            ),
            (
                "看这里 选中一个内容后",
                "bench",
                Turn::Interrupted,
                false,
                86400,
                "复现了选区跳动",
            ),
            (
                "整理知识库目录",
                "knowledge-base",
                Turn::Completed,
                false,
                40 * 86400,
                "整理好了",
            ),
            (
                "知识库去重",
                "knowledge-base",
                Turn::Completed,
                false,
                41 * 86400,
                "去掉了 30 条",
            ),
            (
                "bench 跑分脚本",
                "bench",
                Turn::Completed,
                false,
                50 * 86400,
                "加了脚本",
            ),
        ];
        let mut layout: Option<mooring_core::layout::Layout> = None;
        for (i, (title, folder, turn, live, ago, says)) in rows.into_iter().enumerate() {
            let (sid, tid) = (format!("dense-s{i}"), format!("dense-t{i}"));
            let at = now - ago;
            self.workspace.terminals.push(Terminal {
                id: tid.clone(),
                space: "s0".into(),
                cwd: dir(folder),
                ..shell.clone()
            });
            self.workspace.agent_sessions.push(AgentSession {
                id: sid.clone(),
                cli_session_id: sid.clone(),
                terminal: Some(tid.clone()),
                current: live,
                task: None,
                // One you marked complete: the column draws its ✓.
                outcome: if i == 6 {
                    Outcome::Completed
                } else {
                    Outcome::Open
                },
                updated: at,
                cwd: dir(folder),
                name: title.into(),
                ..template.clone()
            });
            if !live {
                self.resuming.resumed.insert(sid.clone(), tid.clone());
            }
            // A finished turn not looked at yet: its green dot is filled.
            if i == 4 {
                self.attention.unseen.insert(sid.clone());
            }
            let record = |kind: &str, summary: &str| Evidence {
                kind: kind.into(),
                summary: summary.into(),
                exit_code: None,
                at: now - 20,
                source: Source::default(),
            };
            self.workspace.content.insert(
                sid,
                Content {
                    interactive: true,
                    first_ask_known: true,
                    title: title.into(),
                    asks: vec![Quote {
                        text: title.into(),
                        at,
                        source: Source::default(),
                    }],
                    turn,
                    turn_started: at,
                    updated: if turn == Turn::Running { now } else { at },
                    waiting: if turn == Turn::Waiting {
                        says.into()
                    } else {
                        String::new()
                    },
                    error: if turn == Turn::Error {
                        says.into()
                    } else {
                        String::new()
                    },
                    completed_claim: turn == Turn::Completed,
                    last_reply: if matches!(turn, Turn::Completed | Turn::Interrupted) {
                        says.into()
                    } else {
                        String::new()
                    },
                    evidence: match turn {
                        Turn::Running if says.starts_with("文件修改") => {
                            vec![record("file", says)]
                        }
                        Turn::Running => vec![record("command", says)],
                        _ => vec![],
                    },
                    ..Default::default()
                },
            );
            let leaf = mooring_core::layout::Layout::Leaf(tid);
            layout = Some(match layout {
                None => leaf,
                Some(first) => mooring_core::layout::Layout::Split {
                    axis: mooring_core::layout::Axis::Down,
                    ratio: 5000,
                    first: Box::new(first),
                    second: Box::new(leaf),
                },
            });
        }
        // Two closed since: their terminals are gone, so they count under 已关闭.
        for i in 0..2 {
            let sid = format!("dense-closed-{i}");
            self.workspace.agent_sessions.push(AgentSession {
                id: sid.clone(),
                cli_session_id: sid.clone(),
                terminal: Some(format!("dense-gone-{i}")),
                current: false,
                task: None,
                outcome: Outcome::Open,
                updated: now - 3600,
                // No terminal left to place it: the space is found by its folder.
                cwd: self
                    .workspace
                    .spaces
                    .iter()
                    .find(|s| s.id == "s0")
                    .map(|s| s.cwd.clone())
                    .unwrap_or_default(),
                name: format!("已关闭的会话 {i}"),
                ..template.clone()
            });
            self.workspace.content.insert(
                sid,
                Content {
                    interactive: true,
                    turn: Turn::Completed,
                    turn_started: now - 3600,
                    updated: now - 3600,
                    ..Default::default()
                },
            );
        }
        self.workspace.tabs.push(Tab {
            id: "dense".into(),
            space: "s0".into(),
            default_task: None,
            title: "左栏样例".into(),
            layout,
        });
        self.workspace.revision = u64::MAX / 2;
    }
    /// Attaches the fixture sessions' own records to task0's criteria through a real
    /// edit: a passing test and a file change on the first, a failed test on the third;
    /// with `full`, passing records on all three.
    pub(super) fn visual_attach_evidence(&mut self, full: bool, cx: &mut Context<Self>) {
        let Some(mut task) = self
            .workspace
            .tasks
            .iter()
            .find(|t| t.id == "task0")
            .cloned()
        else {
            return;
        };
        let records = super::task_detail::attachable(&self.workspace, "task0", None);
        let find = |needle: &str, ok: bool| {
            records
                .iter()
                .find(|e| e.record.summary.contains(needle) && e.record.proves() == ok)
                .cloned()
        };
        let mut sets: Vec<Vec<Option<CriterionEvidence>>> = vec![
            vec![find("tab_title：", true), find("tabs.rs", true)],
            vec![],
            vec![find("-p mooring-app：", false)],
        ];
        if full {
            sets[1] = vec![
                find("tab_title_survives_exit", true),
                find("model.rs", true),
            ];
            sets[2].push(find("213 passed", true));
        }
        for (c, set) in task.acceptance.iter_mut().zip(sets) {
            c.evidence = set.into_iter().flatten().collect();
        }
        self.edit(Edit::UpdateTask(task), cx);
    }
}
impl Workbench {
    /// Binds a session to each of the running terminals `rw`, `re`, `rq`, `rr`: one waiting
    /// on an approval, one stopped on an error, one asking a question with a short title,
    /// one waiting on a text no reasons rule knows. Kept in memory only.
    /// Two panes holding the same two-turn conversation: `pc` as Claude Code draws it
    /// (`❯`), `px` as Codex (`›`, and Codex pins its own ask).
    pub fn visual_pinned_ask_fixture(&mut self, asks: &[&str], cx: &mut Context<Self>) {
        use mooring_core::content::{Content, Quote, Source, Turn};
        use mooring_core::model::{Agent, AgentSession, Outcome};
        let now = now();
        for (tid, runtime) in [("pc", Agent::Claude), ("px", Agent::Codex)] {
            let sid = format!("pinned-{tid}");
            self.workspace.agent_sessions.push(AgentSession {
                id: sid.clone(),
                runtime,
                cli_session_id: sid.clone(),
                terminal: Some(tid.into()),
                current: true,
                task: None,
                outcome: Outcome::Open,
                updated: now - 60,
                cwd: "/tmp".into(),
                transcript: String::new(),
                name: "补齐遗漏并提交变更".into(),
                binding: None,
            });
            self.workspace.content.insert(
                sid,
                Content {
                    interactive: true,
                    first_ask_known: true,
                    title: "补齐遗漏并提交变更".into(),
                    asks: asks
                        .iter()
                        .map(|a| Quote {
                            text: (*a).into(),
                            at: now - 600,
                            source: Source::default(),
                        })
                        .collect(),
                    turn: Turn::Running,
                    turn_started: now - 120,
                    updated: now - 5,
                    ..Default::default()
                },
            );
        }
        self.workspace.revision = u64::MAX / 2;
        self.mark_pane_agents(cx);
        cx.notify();
    }
    pub fn visual_pinned_ask(
        &self,
        id: &str,
        cx: &gpui::App,
    ) -> (Vec<Option<usize>>, Option<(String, bool)>, Option<String>) {
        self.layout.panes[id].read(cx).visual_pinned_ask()
    }
    pub fn visual_scroll_row(&self, id: &str, row: usize, cx: &mut Context<Self>) {
        self.layout.panes[id].update(cx, |p, cx| p.visual_scroll_row(row, cx));
    }
    pub fn visual_back_to_ask(&self, id: &str, cx: &mut Context<Self>) {
        self.layout.panes[id].update(cx, |p, cx| p.back_to_latest_ask(cx));
    }
    pub fn visual_reason_fixture(&mut self, cx: &mut Context<Self>) {
        use mooring_core::content::{Content, Quote, Source, Turn};
        use mooring_core::model::{Agent, AgentSession, Outcome};
        let now = now();
        let rows = [
            (
                "rw",
                "查一下 grok 的会话目录",
                Turn::Waiting,
                "Claude requested permissions to use WebSearch, but you haven't granted it yet.",
            ),
            (
                "re",
                "修好登录页",
                Turn::Error,
                "Failed to authenticate: OAuth session expired and could not be refreshed",
            ),
            (
                "rq",
                "整合为 next 做的 UI",
                Turn::Waiting,
                "【合并授权】要不要直接合进 main？（合并 / 先开 PR）",
            ),
            (
                "rr",
                "这个目录的 grok 的聊天记录",
                Turn::Waiting,
                "找不到 grok 的会话目录，要我去 ~/.grok 下面再找一遍吗？",
            ),
        ];
        for (tid, title, turn, says) in rows {
            let sid = format!("reason-{tid}");
            self.workspace.agent_sessions.push(AgentSession {
                id: sid.clone(),
                runtime: Agent::Claude,
                cli_session_id: sid.clone(),
                terminal: Some(tid.into()),
                current: true,
                task: None,
                outcome: Outcome::Open,
                updated: now - 60,
                cwd: "/tmp".into(),
                transcript: String::new(),
                name: title.into(),
                binding: None,
            });
            self.workspace.content.insert(
                sid,
                Content {
                    interactive: true,
                    first_ask_known: true,
                    title: title.into(),
                    asks: vec![Quote {
                        text: format!("{title}，顺便把测试补上"),
                        at: now - 600,
                        source: Source::default(),
                    }],
                    turn,
                    turn_started: now - 600,
                    updated: now - 60,
                    waiting: if turn == Turn::Waiting {
                        says.into()
                    } else {
                        String::new()
                    },
                    error: if turn == Turn::Error {
                        says.into()
                    } else {
                        String::new()
                    },
                    ..Default::default()
                },
            );
        }
        self.workspace.revision = u64::MAX / 2;
        cx.notify();
    }
}
