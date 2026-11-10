//! `visual_page`: each named page set up from fixture data.
use super::*;

impl Workbench {
    pub fn visual_page(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.tasks.form = None;
        self.tasks.quick_task = None;
        self.tasks.picking = None;
        self.nav.task_detail = None;
        self.tasks.attaching = None;
        self.tasks.expanded.clear();
        self.overview.cleanup.clear();
        self.overview.here = None;
        self.overview.here_more = None;
        self.search.hide();
        self.nav.overview = matches!(
            name,
            "Overview"
                | "OverviewEnd"
                | "Closed"
                | "BoardDrawer"
                | "BoardAccept"
                | "BoardRefuse"
                | "BoardDrag"
                | "BoardRefuseDrag"
        );
        self.overview.drawer = None;
        self.overview.toast = None;
        self.overview.closed_only = name == "Closed";
        self.nav.zoomed = None;
        self.nav.space = "s0".into();
        self.nav.tab = Some(
            match name {
                "TodoStart" | "UnboundAgent" | "UnboundAgentMenu" | "UnboundAgentConfirm" => {
                    "start"
                }
                // No tab of the fixture's: the empty tab's 在这里开始 and its Agent links.
                "EmptyTab" => "visual-empty-tab",
                _ => "feedback",
            }
            .into(),
        );
        self.overview.show_running = false;
        self.statusbar = None;
        // `MainSpace` narrows 会话优先 to the sample's 工作空间, as a click on its chip does.
        self.sidebar.space_filter = (name == "MainSpace").then(|| "s0".to_owned());
        self.sync_panes(cx);
        match name {
            // The status bar's 工作中 popover beside the sidebar: one session, one color.
            "StatusSessions" => self.statusbar = Some(super::statusbar::StatusPop::Sessions),
            // 额度's popover, with MOORING_FIXTURE_QUOTA_SPARE a week about to go unused.
            "StatusQuota" => self.statusbar = Some(super::statusbar::StatusPop::Quota),
            // Main is the window as it opens (docs/ui-v3/v3-4.png); 看进度's card has its own.
            "Main" | "MainSpace" | "UnboundAgent" | "EmptyTab" => {}
            // 标记完成 on it, in its menu; asked for real, the fixture's screen names no
            // conversation, so the host's answer opens the close-only confirmation.
            "UnboundAgentMenu" => {
                self.layout.menu = Some(PaneMenu {
                    terminal: "pstart".into(),
                    position: gpui::point(px(T::WINDOW_WIDTH / 2.), px(T::TOP * 3.)),
                    page: MenuPage::Main,
                    path: None,
                })
            }
            "UnboundAgentConfirm" => {
                self.layout.menu = None;
                self.complete_unbound("pstart".into(), cx)
            }
            "HereClickOut" => {
                self.overview.here = self.workspace.current_agent("p0").map(|s| s.id.clone())
            }
            // The sidebar with the space's todos unfolded above its sessions.
            "SidebarTodos" => self.tasks.todos_open = true,
            // The board: a card's drawer, an accepted drop with 撤销, a refused one.
            "BoardDrawer" => {
                use super::board::Column;
                self.overview.drawer = self
                    .column_sessions(Column::Needs, now())
                    .first()
                    .map(|s| s.id.clone());
            }
            "BoardAccept" | "BoardRefuse" => {
                use super::board::Column;
                let from = if name == "BoardAccept" {
                    Column::Review
                } else {
                    Column::Needs
                };
                let card = self.column_sessions(from, now()).first().map(|s| {
                    super::board_drawer::DraggedCard {
                        id: s.id.clone(),
                        title: self.session_title(s),
                        from,
                    }
                });
                if let Some(card) = card {
                    self.drop_card(&card, Column::Done, cx);
                }
            }
            // The card open on the focused waiting pane; the harness then types a key.
            "HereTyped" => {
                self.overview.here = self.workspace.current_agent("p0").map(|s| s.id.clone());
                self.nav.focused = Some("p0".into());
                if let Some(pane) = self.layout.panes.get("p0") {
                    window.focus(&pane.focus_handle(cx), cx);
                }
            }
            // The close dialog over a waiting Agent, and over one whose turn is under way.
            // The top bar's entry for an older host, and its restart dialog.
            "RestartHost" => {
                crate::lifecycle::visual_stale_host();
                self.layout.confirm_close = Some(Closing::Host);
            }
            "CloseIdle" | "CloseWorking" => {
                if name == "CloseWorking"
                    && let Some(s) = self.workspace.current_agent("p0").map(|s| s.id.clone())
                    && let Some(c) = self.workspace.content.get_mut(&s)
                {
                    c.turn = mooring_core::content::Turn::Running;
                }
                self.layout.confirm_close = Some(Closing::Pane("p0".into()));
            }
            // An Agent reply in Markdown, as the pane header shows it.
            "Markdown" => {
                for c in self.workspace.content.values_mut() {
                    if c.turn == mooring_core::content::Turn::Completed {
                        c.last_reply =
                            "**建议都接收**，见 `a.rs`\n\n| 投稿 | 建议处理 |\n|---|---|\n| 3 | 接收 |"
                                .into();
                    }
                }
            }
            "SelectHint" => {
                if let Some(pane) = self.layout.panes.get("p0") {
                    pane.update(cx, |p, cx| p.visual_select_hint(cx));
                }
            }
            "HereMore" => {
                self.overview.here = self.workspace.current_agent("p0").map(|s| s.id.clone());
                self.overview.here_more = self.overview.here.clone();
            }
            // The filled form, the line being written last; the login shell's answer is
            // fixed so the run does not depend on what this machine has installed.
            "TodoNew" | "TodoNewBare" | "TodoNewChecks" => {
                let all: HashSet<String> = Agent::all()
                    .into_iter()
                    .map(|a| a.executable().to_owned())
                    .collect();
                self.tasks.found = Some(all.clone());
                self.show_form(FormKind::Task, window, cx);
                let mut task = self
                    .workspace
                    .tasks
                    .iter()
                    .find(|t| t.id == "task0")
                    .cloned()
                    .unwrap();
                match name {
                    "TodoNewBare" => task.acceptance.clear(),
                    // Every check at once: no title after a save, a repeated line, a space
                    // whose folder is gone and an Agent the shell cannot find.
                    "TodoNewChecks" => {
                        task.title.clear();
                        let repeat = task.acceptance[1].description.clone();
                        task.acceptance.push(AcceptanceCriterion {
                            id: "repeat".into(),
                            description: repeat,
                            evidence: vec![],
                        });
                        if let Some(gone) = self
                            .workspace
                            .spaces
                            .iter()
                            .find(|s| !Path::new(&s.cwd).is_dir())
                        {
                            task.space = gone.id.clone();
                        }
                        if let Some(missing) = Agent::all().into_iter().last() {
                            task.agent = missing;
                            self.tasks.found = Some(
                                all.into_iter()
                                    .filter(|e| e != missing.executable())
                                    .collect(),
                            );
                        }
                    }
                    _ => (),
                }
                self.fill_task_form(&task, window, cx);
                if let Some(form) = &mut self.tasks.form {
                    form.tried = name == "TodoNewChecks";
                    if let Some(last) = form.criteria.last() {
                        window.focus(&last.field.focus_handle(cx), cx);
                    }
                }
            }
            "TodoDetail" | "TodoDetailAttach" | "TodoDetailFull" => {
                self.visual_attach_evidence(name == "TodoDetailFull", cx);
                self.nav.task_detail = Some("task0".into());
                if name == "TodoDetailAttach" {
                    self.tasks.attaching = self
                        .workspace
                        .tasks
                        .iter()
                        .find(|t| t.id == "task0")
                        .and_then(|t| t.acceptance.get(1))
                        .map(|c| c.id.clone());
                }
            }
            // A todo settled without criteria or sessions.
            "TodoDetailDone" => {
                self.edit(
                    Edit::SetTaskOutcome {
                        seen: None,
                        id: "task2".into(),
                        outcome: Outcome::Completed,
                    },
                    cx,
                );
                self.nav.task_detail = Some("task2".into());
            }
            "TodoStart" => {
                let text = self
                    .workspace
                    .tasks
                    .iter()
                    .find(|t| t.id == "task1")
                    .unwrap()
                    .prompt();
                let command = self
                    .workspace
                    .tasks
                    .iter()
                    .find(|t| t.id == "task1")
                    .unwrap()
                    .agent
                    .command();
                self.layout.panes["pstart"]
                    .update(cx, |pane, cx| pane.start_agent(&command, Some(text), cx));
            }
            "Cleanup" => {
                self.overview.cleanup = self
                    .workspace
                    .agent_sessions
                    .iter()
                    .filter(|s| status_of(&self.workspace, s) == Status::Stale)
                    .map(|s| s.id.clone())
                    .collect();
                self.overview.cleanup_index = 0;
                assert!(!self.overview.cleanup.is_empty());
            }
            "QuickTask" => {
                self.show_quick_task(window, cx);
                if let Some((field, _)) = &self.tasks.quick_task {
                    field.update(cx, |f, cx| {
                        f.set_text("给离线模式补一组断网回归测试");
                        cx.notify();
                    });
                }
            }
            "ScreenWait" => {
                self.overview.here = self.workspace.current_agent("p1").map(|s| s.id.clone())
            }
            "StartPick" => self.tasks.picking = Some(("task2".into(), false)),
            // The logo's menu, then the menu after choosing the mascot from it.
            "IconMenu" => {
                self.layout.icon_menu = Some(gpui::point(px(T::TRAFFIC_LIGHT_INSET), px(T::TOP)))
            }
            "IconMascot" => {
                layout::choose_icon(AppIcon::Mascot, cx);
                self.layout.icon_menu = Some(gpui::point(px(T::TRAFFIC_LIGHT_INSET), px(T::TOP)));
            }
            // The left column's presets, set in memory so the evidence run writes no choice.
            "SidebarDirectory" | "SidebarRail" | "SidebarWasm" | "SidebarWasmOpen"
            | "SidebarWasmClosed" | "SidebarViewMenu" => {
                self.sidebar.preset = match name {
                    "SidebarDirectory" => "directory",
                    // The example WebAssembly plugin, installed into the scratch config.
                    "SidebarWasm" | "SidebarWasmOpen" | "SidebarWasmClosed" => "wasm-status",
                    _ => "recent",
                }
                .into();
                self.sidebar.collapsed = name == "SidebarRail";
                self.sidebar.toggled.clear();
                // The ≑ beside 会话 with its menu open, as a click on it leaves it.
                self.sidebar.view_menu = (name == "SidebarViewMenu")
                    .then(|| gpui::point(px(T::SIDEBAR - 24.), px(T::TOP + 170.)));
                if name == "SidebarWasmOpen" || name == "SidebarWasmClosed" {
                    for section in ["rest", "closed"] {
                        self.sidebar
                            .toggled
                            .insert(format!("wasm:wasm-status:{section}"), true);
                    }
                }
                // Closes the tab's last pane, a finished Agent, so the plugin has a closed
                // session to place. Run it last: the fixture keeps the pane closed.
                if name == "SidebarWasmClosed"
                    && let Some(last) = self.workspace.tab_terminals("feedback").pop()
                {
                    self.close_now(Closing::Pane(last), cx);
                }
            }
            // The two looks, set in memory so the evidence run writes no choice.
            "StyleStandard" | "StyleTerminal" => {
                use crate::ui::style::Style;
                cx.set_global(if name == "StyleTerminal" {
                    Style::Terminal
                } else {
                    Style::Standard
                });
                self.nav.focused = Some("p0".into());
            }
            "PaneMenu" => {
                self.nav.focused = Some("p0".into());
                self.layout.menu = Some(PaneMenu {
                    terminal: "p0".into(),
                    position: gpui::point(px(T::WINDOW_WIDTH / 2.), px(T::TOP * 3.)),
                    page: MenuPage::Main,
                    path: None,
                });
            }
            // Runs the first action a real pane offers, as a click in its menu would.
            "ActionRun" => {
                let target = self.workspace.tabs.iter().find_map(|tab| {
                    let id = self.workspace.tab_terminals(&tab.id).first()?.clone();
                    Some((tab.space.clone(), tab.id.clone(), id))
                });
                if let Some((space, tab, terminal)) = target {
                    self.nav.space = space;
                    self.nav.tab = Some(tab);
                    self.nav.focused = Some(terminal.clone());
                    self.sync_panes(cx);
                    if let Some(action) = self.actions_for(&terminal).first().copied() {
                        self.run_action(&terminal, action, cx);
                    }
                }
            }
            // The palette empty, narrowed to 出错, with a query, and with one that finds
            // nothing but whose halves do.
            "SearchEmpty" | "SearchGroup" | "Search" | "SearchNone" => {
                self.nav.focused = Some("p0".into());
                self.open_search(window, cx);
                if name == "SearchGroup" {
                    self.search.pick_group(Some(Status::Error));
                }
                if name == "Search" || name == "SearchNone" {
                    let query = match name {
                        "SearchNone" => "反馈报销".into(),
                        _ => {
                            std::env::var("MOORING_SEARCH_QUERY").unwrap_or_else(|_| "反馈".into())
                        }
                    };
                    self.search.field().update(cx, |f, cx| {
                        f.set_text(&query);
                        cx.notify();
                    });
                }
            }
            "Closed" => self.visual_close_two_sessions(),
            // The overview's foot: the folded groups below the cards.
            "OverviewEnd" => self.overview.scroll.scroll_to_bottom(),
            "Overview" | "Menubar" | "BoardDrag" | "BoardRefuseDrag" => (),
            // `Settings-{General|Look|Terminal|Keys|Notifications|Phones|Plugins|About}`: the
            // settings window open on that page (MOORING_LANG=en for its English).
            settings if settings.starts_with("Settings-") => {
                use super::settings::Page;
                let page = match &settings["Settings-".len()..] {
                    "Look" => Page::Look,
                    "Terminal" => Page::Terminal,
                    "Keys" => Page::Keys,
                    "Notifications" => Page::Notifications,
                    "Phones" => Page::Phones,
                    "Plugins" => Page::Plugins,
                    "About" => Page::About,
                    _ => Page::General,
                };
                self.show_settings_page(page, window, cx);
            }
            // `SidebarUnlinked-{all|s0|s1}-{today}{older}-{none|codex|claude}-{recent|directory}`:
            // the terminals `MOORING_FIXTURE_UNLINKED` starts, under each fold (`o` open, `f`
            // folded) and selection; `s1` holds none of them.
            unlinked if unlinked.starts_with("SidebarUnlinked-") => {
                let parts: Vec<&str> = unlinked.split('-').collect();
                let (scope, folds, selected) = (parts[1], parts[2].as_bytes(), parts[3]);
                self.sidebar.preset = parts[4].into();
                self.sidebar.collapsed = false;
                self.sidebar.space_filter = (scope != "all").then(|| scope.to_owned());
                self.sidebar.toggled.clear();
                for (key, fold) in [("recent:today", folds[0]), ("recent:older", folds[1])] {
                    self.sidebar.toggled.insert(key.into(), fold == b'o');
                }
                self.nav.focused = Some(if selected == "none" {
                    "p0".to_owned()
                } else {
                    self.nav.tab = Some("unlinked".into());
                    format!("punlinked-{selected}")
                });
                self.sync_panes(cx);
            }
            // `Dense-{directory|wasm|first}-{standard|terminal}-{light|dark}[-open|-fold|-menu|-bg|-peek|-pop|-sub|-many|-manyall]`:
            // the denser presets over sessions shaped like sidebar-density.html's; `-bg`
            // stops the second running turn while a sub-agent of it still runs; `-pop` opens
            // the status bar's 工作中 popover beside the column.
            dense if dense.starts_with("Dense-") => {
                use crate::ui::{appearance::Mode, style::Style};
                let parts: Vec<&str> = dense.split('-').collect();
                self.visual_dense_fixture();
                self.sidebar.preset = match parts[1] {
                    "wasm" => "wasm-status",
                    // 会话优先, the built-in default.
                    "first" => "recent",
                    other => other,
                }
                .into();
                self.sidebar.collapsed = false;
                self.sidebar.toggled.clear();
                self.visual_spread_spaces(parts.get(4).copied());
                self.visual_bare_space(parts.get(4).copied());
                self.visual_unknown_state(parts.get(4).copied());
                self.visual_crowded(parts.get(4).copied());
                if parts.get(4) == Some(&"open") {
                    for key in ["status:rest", "wasm:wasm-status:rest"] {
                        self.sidebar.toggled.insert(key.into(), true);
                    }
                }
                // `-fold`: 会话优先 with 要你处理 and 今天 folded to their headings.
                if parts.get(4) == Some(&"fold") {
                    for key in ["recent:need", "recent:today"] {
                        self.sidebar.toggled.insert(key.into(), false);
                    }
                }
                cx.set_global(if parts[2] == "terminal" {
                    Style::Terminal
                } else {
                    Style::Standard
                });
                let _ = crate::ui::appearance::set_mode(
                    if parts[3] == "dark" {
                        Mode::Dark
                    } else {
                        Mode::Light
                    },
                    cx,
                );
                // `-done`: the ✓ row unfolded, as a click on it leaves it.
                self.layout.done_open =
                    (parts.get(4) == Some(&"done")).then(|| "dense-s6".to_owned());
                // `-shell`: the same turn ended with only a server left in a background
                // shell: 等你验收, with a note of what is left running.
                let shell = parts.get(4) == Some(&"shell");
                if (parts.get(4) == Some(&"bg") || shell)
                    && let Some(c) = self
                        .workspace
                        .content
                        .values_mut()
                        .find(|c| c.title == "标签命名依旧有问题")
                {
                    c.turn = mooring_core::content::Turn::Completed;
                    c.completed_claim = false;
                    c.background = Some(mooring_core::content::Background {
                        agents: u32::from(!shell),
                        shells: u32::from(shell),
                        ..Default::default()
                    });
                    c.shells = if shell {
                        vec![mooring_core::content::ShellTask {
                            id: "server".into(),
                            port: Some(5569),
                            ..Default::default()
                        }]
                    } else {
                        vec![]
                    };
                }
                if let Some(v @ ("sub" | "many" | "manyall" | "marks")) = parts.get(4).copied() {
                    self.sample_subagents(v);
                }
                self.nav.focused = Some(
                    match parts.get(4) {
                        Some(&"bare") => "dense-bare",
                        Some(&"crowded") => "crowd-t0",
                        _ => "dense-t2",
                    }
                    .into(),
                );
                // `-peek`: the first session floats over the focused one.
                self.peek = (parts.get(4) == Some(&"peek")).then(|| {
                    let pane = cx.new(|cx| {
                        let mut pane = Pane::new(self.store.socket(), "dense-t0", false, cx);
                        pane.peek = true;
                        pane
                    });
                    super::peek::Peek {
                        terminal: "dense-t0".into(),
                        session: "dense-s0".into(),
                        pane,
                        answering: false,
                        note: None,
                        confirm_end: false,
                        remote: None,
                    }
                });
                self.layout.space_menu = (parts.get(4) == Some(&"menu"))
                    .then(|| ("s0".into(), gpui::point(px(T::S20), px(T::TOP * 2.))));
                self.layout.space_switch = true;
                if parts.get(4) == Some(&"pop") {
                    self.statusbar = Some(super::statusbar::StatusPop::Sessions);
                }
            }
            _ => panic!("unknown visual page"),
        }
        cx.notify();
        window.refresh();
    }
}
