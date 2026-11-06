//! Fixture-driven layouts and pages for native visual evidence.
use super::*;
mod crowded;
mod fixture;
mod page;
mod spaces;
#[path = "subagent_sample_tests.rs"]
mod subagent_samples;
mod todo;

/// The space menu's state, the rename field's text and focus, each space's name and cwd.
pub type VisualSpaceState = (bool, Option<(String, bool)>, Vec<(String, String)>);
impl Workbench {
    /// ⌘B, as the sidebar button does.
    pub fn visual_toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.toggle_sidebar(cx);
    }
    /// The host's socket, whether its connection strip shows, and 「重新连接」 itself.
    pub fn visual_socket(&self) -> PathBuf {
        self.store.socket().to_owned()
    }
    /// The pane's session, whether 标记完成 is offered on it, and whether the pane is shown.
    pub fn visual_completable(&self, terminal: &str) -> (Option<String>, bool, bool) {
        let session = self.pane_session(terminal);
        let review = session.is_some_and(|s| {
            s.outcome == Outcome::Open && status_of(&self.workspace, s) == Status::Review
        });
        (
            session.map(|s| s.id.clone()),
            review,
            self.layout.panes.contains_key(terminal),
        )
    }
    /// The 标记完成 button's click, and 「留着这个画面」 mid-run.
    pub fn visual_complete(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let session = self.pane_session(terminal).expect("no session").id.clone();
        self.complete_pane(terminal.to_owned(), session, cx);
    }
    /// The session card over the pane, as clicking its status opens it.
    pub fn visual_open_here(&mut self, terminal: &str, cx: &mut Context<Self>) {
        self.overview.here = self.pane_session(terminal).map(|s| s.id.clone());
        cx.notify();
    }
    /// 标记完成 with a given animation, held at `pin` of the way through when given.
    pub fn visual_complete_with(
        &mut self,
        terminal: &str,
        effect: mooring_core::effect::Complete,
        pin: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let session = self.pane_session(terminal).expect("no session").id.clone();
        self.complete_pane_with(terminal.to_owned(), session, effect, cx);
        self.pin_completing(terminal, pin);
    }
    /// Where a run is held for screenshots: `MOORING_COMPLETE_TS` as `from,to,step`. The
    /// window takes `MOORING_COMPLETE_WINDOW` (`width,height`) first, to try a small pane.
    pub fn visual_complete_times(&self, window: &mut Window) -> Vec<f32> {
        if let Ok(asked) = std::env::var("MOORING_COMPLETE_WINDOW")
            && let Some((w, h)) = asked.split_once(',')
        {
            let parse = |x: &str| px(x.parse().expect("window size"));
            window.resize(gpui::size(parse(w), parse(h)));
        }
        let asked: Vec<f32> = std::env::var("MOORING_COMPLETE_TS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|x| x.parse().ok())
            .collect();
        match asked[..] {
            [from, to, step] => (0..)
                .map(|i| from + step * i as f32)
                .take_while(|t| *t <= to + 1e-4)
                .collect(),
            _ => vec![0.1, 0.32, 0.45, 0.6, 0.8, 0.9, 0.95],
        }
    }
    pub fn visual_specks(&self) -> usize {
        super::complete::SPECKS.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// The overlay's paints since the last call: frame gaps and paint times, in ms.
    pub fn visual_take_paints(&self) -> (Vec<f32>, Vec<f32>) {
        let most = std::mem::take(&mut *super::complete::PRIMITIVES.lock().unwrap());
        eprintln!(
            "PRIMITIVES busiest frame: paths={} quads={}",
            most.0, most.1
        );
        let log = std::mem::take(&mut *super::complete::PAINTS.lock().unwrap());
        let gaps = log
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).as_secs_f32() * 1e3)
            .collect();
        let paints = log.iter().map(|(_, d)| d.as_secs_f32() * 1e3).collect();
        (gaps, paints)
    }
    pub fn visual_pin_completing(
        &mut self,
        terminal: &str,
        t: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        self.pin_completing(terminal, t);
        cx.notify();
    }
    pub fn visual_keep_completed(&mut self, terminal: &str, cx: &mut Context<Self>) {
        self.layout.completing.remove(terminal);
        cx.notify();
    }
    /// A notification for `id` clicked before the session loaded, from a tab page: the
    /// session is taken out of what this window holds, as before its first refresh.
    /// `then_overview` is the user opening the overview before it loads.
    pub fn visual_route_before_load(
        &mut self,
        id: &str,
        then_overview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.nav.overview = false;
        self.overview.here = None;
        self.workspace.agent_sessions.retain(|s| s.id != id);
        self.navigate_to(id, window, cx);
        let said = self.error.clone();
        if then_overview {
            self.show_overview(cx);
        }
        said
    }
    pub fn visual_session_ids(&self) -> Vec<String> {
        self.workspace
            .agent_sessions
            .iter()
            .map(|s| s.id.clone())
            .collect()
    }
    /// The overview's open state, the session it centers on, and the error line.
    pub fn visual_route_state(&self) -> (bool, Option<String>, Option<String>) {
        (
            self.nav.overview,
            self.overview.here.clone(),
            self.error.clone(),
        )
    }
    pub fn visual_cut_off(&self) -> bool {
        self.store.cut_off()
    }
    pub fn visual_reconnect(&mut self, cx: &mut Context<Self>) {
        self.reconnect(cx);
    }
    pub fn visual_layout(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.overview = false;
        self.nav.zoomed = None;
        self.layout.menu = None;
        if name == "eight" {
            let mut leaves = self
                .workspace
                .terminals
                .iter()
                .map(|t| Layout::Leaf(t.id.clone()))
                .collect::<Vec<_>>();
            while leaves.len() > 1 {
                leaves = leaves
                    .chunks(2)
                    .map(|pair| {
                        if pair.len() == 1 {
                            pair[0].clone()
                        } else {
                            Layout::Split {
                                axis: if pair[0].ids().len() == 1 {
                                    Axis::Down
                                } else {
                                    Axis::Right
                                },
                                ratio: 5000,
                                first: Box::new(pair[0].clone()),
                                second: Box::new(pair[1].clone()),
                            }
                        }
                    })
                    .collect();
            }
            // A new tab takes every pane; the old ones close as they empty.
            if let Some(mut tab) = self
                .workspace
                .tabs
                .iter()
                .find(|t| t.id == "eight")
                .cloned()
            {
                tab.id = "eight-all".into();
                tab.layout = leaves.pop();
                if let Ok(Reply::Workspace(w)) = rpc(
                    self.store.socket(),
                    &Request::Edit {
                        request: None,
                        edit: Edit::CreateTab { tab },
                    },
                ) {
                    self.workspace = w;
                }
            }
        }
        self.nav.tab = Some(
            match name {
                "menu" => "nested",
                "eight" => "eight-all",
                name => name,
            }
            .into(),
        );
        self.sync_panes(cx);
        if let Some(id) = self
            .nav
            .tab
            .as_ref()
            .and_then(|id| self.workspace.tab_terminals(id).first().cloned())
            && let Some(pane) = self.layout.panes.get(&id)
        {
            window.focus(&pane.focus_handle(cx), cx);
        }
        cx.notify();
        window.refresh();
    }
    /// The middle of the first card in a board column, by the column's id.
    pub fn visual_card_center(&self, column: &str) -> Option<gpui::Point<gpui::Pixels>> {
        use super::board::Column;
        let c = [
            Column::Todo,
            Column::Running,
            Column::Needs,
            Column::Review,
            Column::Done,
        ]
        .into_iter()
        .find(|c| c.key() == column)?;
        let bounds = self
            .overview
            .columns
            .borrow()
            .get(&c)?
            .state
            .bounds_for_item(0)?;
        Some(bounds.center())
    }
    /// How far apart neighbouring board columns are drawn.
    pub fn visual_column_step(&self) -> gpui::Pixels {
        self.overview
            .column_width
            .unwrap_or(px(T::BOARD_COLUMN_MIN))
            + px(T::S14)
    }
    pub fn visual_bounds(&self) -> gpui::Bounds<gpui::Pixels> {
        *self
            .layout
            .split_bounds
            .get(&Vec::new())
            .expect("root splitter bounds")
    }
    /// The middle of the pane menu's item `title`, past the rules between groups.
    pub fn visual_menu_point(&self, title: &str) -> gpui::Point<gpui::Pixels> {
        let menu = self
            .layout
            .menu
            .as_ref()
            .expect("right mouse opened pane menu");
        let zoomed = self.nav.zoomed.as_deref() == Some(&menu.terminal);
        let mut y = T::S6;
        for row in super::pane_menu::menu_rows(zoomed, true) {
            match row {
                Some(t) if t == title => {
                    return menu.position + gpui::point(px(T::S20), px(y + T::ROW / 2.));
                }
                Some(_) => y += T::ROW,
                None => y += super::layout::MENU_RULE,
            }
        }
        panic!("no {title} in the pane menu");
    }
    /// Opens the session menu on `id` at `at`, as a right click on its row does.
    pub fn visual_open_session_menu(
        &mut self,
        id: &str,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.layout.session_menu = Some((id.to_owned(), at));
        cx.notify();
    }
    /// Where the open session menu lists `title`.
    pub fn visual_session_menu_point(&self, title: &str) -> gpui::Point<gpui::Pixels> {
        let (id, position) = self
            .layout
            .session_menu
            .as_ref()
            .expect("right mouse opened session menu");
        let index = self
            .session_menu_entries(id)
            .iter()
            .position(|(name, _)| *name == title)
            .unwrap_or_else(|| panic!("no {title} in the session menu"));
        *position + gpui::point(px(T::S20), px(T::S6 + T::ROW * (index as f32 + 0.5)))
    }
    pub fn visual_tab_menu_point(&self, index: usize) -> gpui::Point<gpui::Pixels> {
        let (_, position) = self
            .layout
            .tab_menu
            .as_ref()
            .expect("right mouse opened tab menu");
        *position + gpui::point(px(T::S20), px(T::S6 + T::ROW * (index as f32 + 0.5)))
    }
    /// Whether the tab menu is open, and the rename field's text and focus if any.
    pub fn visual_tab_state(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> (bool, Option<(String, bool)>) {
        (
            self.layout.tab_menu.is_some(),
            self.layout.renaming_tab.as_ref().map(|(_, field, _)| {
                (
                    field.read(cx).text.clone(),
                    field.focus_handle(cx).is_focused(window),
                )
            }),
        )
    }
    /// Types into the open rename field as an input method would, caret at the end.
    pub fn visual_rename_type(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some((_, field, _)) = &self.layout.renaming_tab {
            field.update(cx, |f, cx| {
                f.set_text(text);
                cx.notify();
            });
        }
    }
    /// The name the first tab of this space shows.
    pub fn visual_first_tab_name(&self, cx: &gpui::App) -> String {
        self.workspace
            .tabs
            .iter()
            .find(|t| t.space == self.nav.space)
            .map(|t| self.tab_name(t, cx))
            .unwrap_or_default()
    }
    pub fn visual_new_tab(&mut self, cx: &mut Context<Self>) {
        self.launch(false, None, cx);
    }
    /// Opens a conversation whose resume cannot take (unknown CLI id, no live terminal),
    /// as clicking a 出错了 row does; returns the tab count and the current tab after.
    pub fn visual_open_failed_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (usize, Option<String>) {
        let session = AgentSession {
            id: "visual-failed".into(),
            runtime: Agent::Codex,
            cli_session_id: "visual-missing".into(),
            terminal: None,
            current: false,
            task: None,
            outcome: Outcome::Open,
            updated: 1,
            cwd: std::env::temp_dir().display().to_string(),
            transcript: String::new(),
            binding: None,
            name: String::new(),
        };
        self.open_session(&session, window, cx);
        (self.workspace.tabs.len(), self.nav.tab.clone())
    }
    /// The recorded directory of the shown tab's first terminal.
    pub fn visual_shown_cwd(&self) -> String {
        let tab = self.nav.tab.as_ref().expect("a shown tab");
        let id = self.workspace.tab_terminals(tab)[0].clone();
        let terminal = self
            .workspace
            .terminals
            .iter()
            .find(|t| t.id == id)
            .unwrap();
        terminal.cwd.clone()
    }
    /// The settings panel when open: (recording, a taken chord waiting, panel focused).
    pub fn visual_settings(&self, window: &Window, cx: &gpui::App) -> Option<(bool, bool, bool)> {
        let view = self.settings.as_ref()?.read(cx);
        Some((
            view.recording.is_some(),
            view.taken.is_some(),
            view.focus.is_focused(window),
        ))
    }
    /// Starts recording a chord for the action named as in keys.json, as its key button does.
    pub fn visual_settings_record(&mut self, name: &str, cx: &mut Context<Self>) {
        let action = crate::keymap::Keymap::actions()
            .find(|a| a.name() == name)
            .unwrap();
        let view = self.settings.clone().expect("settings open");
        view.update(cx, |v, cx| v.record(action, cx));
    }
    /// 取消 under a taken chord, or 全部恢复默认.
    pub fn visual_settings_press(&mut self, button: &str, cx: &mut Context<Self>) {
        let view = self.settings.clone().expect("settings open");
        view.update(cx, |v, cx| match button {
            "取消" => {
                v.taken = None;
                cx.notify();
            }
            "全部恢复默认" => v.reset(None, cx),
            _ => panic!("no button {button}"),
        });
    }
    /// Whether a terminal pane holds input focus.
    /// Where a pane's grid is drawn.
    pub fn visual_terminal_bounds(
        &self,
        terminal: &str,
        cx: &gpui::App,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.layout.panes.get(terminal)?.read(cx).visual_bounds()
    }
    /// Shows a tab other than the current one, as clicking it does.
    pub fn visual_show_other_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let other = self
            .workspace
            .tabs
            .iter()
            .find(|t| Some(&t.id) != self.nav.tab.as_ref());
        let id = other.expect("a second tab").id.clone();
        self.show_tab(&id, window, cx);
    }
    /// The shown tab's first terminal.
    pub fn visual_shown_terminal(&self) -> String {
        let tab = self.nav.tab.as_ref().expect("a shown tab");
        self.workspace.tab_terminals(tab)[0].clone()
    }
    pub fn visual_pane_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.layout
            .panes
            .values()
            .any(|p| p.focus_handle(cx).is_focused(window))
    }
    /// The shown tab's panes in layout order and the one whose handle really holds
    /// input focus (not just the one the workbench remembers).
    pub fn visual_pane_focus(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> (Vec<String>, Option<String>) {
        let order = self
            .nav
            .tab
            .as_ref()
            .map(|t| self.workspace.tab_terminals(t))
            .unwrap_or_default();
        let focused = order
            .iter()
            .find(|id| {
                self.layout
                    .panes
                    .get(*id)
                    .is_some_and(|p| p.focus_handle(cx).is_focused(window))
            })
            .cloned();
        assert_eq!(
            focused.is_some(),
            focused == self.nav.focused,
            "the remembered pane is the focused one"
        );
        (order, focused)
    }
    /// Tabs out of sight left and right of the strip, and a point on the right marker.
    pub fn visual_tab_overflow(&self) -> (usize, usize, gpui::Point<gpui::Pixels>) {
        let count = self.visual_tab_row().0.len();
        let (left, right) = super::render::hidden_tabs(&self.layout.tab_scroll, count);
        let view = self.layout.tab_scroll.bounds();
        (
            left.len(),
            right.len(),
            gpui::point(view.right() + px(T::S20), view.center().y),
        )
    }
    /// Scrolls the tab strip back to its start.
    pub fn visual_tab_scroll_home(&mut self, cx: &mut Context<Self>) {
        let y = self.layout.tab_scroll.offset().y;
        self.layout.tab_scroll.set_offset(gpui::point(px(0.), y));
        cx.notify();
    }
    /// The current space's tabs in strip order, the shown one, and its pane count.
    pub fn visual_tab_row(&self) -> (Vec<String>, Option<String>, usize) {
        let row = self
            .workspace
            .tabs
            .iter()
            .filter(|t| t.space == self.nav.space)
            .map(|t| t.id.clone())
            .collect();
        let panes = self
            .nav
            .tab
            .as_ref()
            .map_or(0, |t| self.workspace.tab_terminals(t).len());
        (row, self.nav.tab.clone(), panes)
    }
    pub fn visual_tabs(&self) -> (usize, Option<String>) {
        (self.workspace.tabs.len(), self.nav.tab.clone())
    }
    pub fn visual_check_panes(&self, count: usize) {
        assert_eq!(self.layout.panes.len(), count);
    }
    pub fn visual_check_saved_ratio(&self) {
        let Reply::Workspace(w) = rpc(self.store.socket(), &Request::Workspace).unwrap() else {
            panic!("workspace")
        };
        let local = self
            .workspace
            .tabs
            .iter()
            .find(|t| t.id == "nested")
            .unwrap();
        assert_eq!(w.tabs.iter().find(|t| t.id == "nested").unwrap(), local);
        assert!(
            matches!(local.layout,Some(Layout::Split{ratio,..}) if (6600..6800).contains(&ratio))
        );
    }
    pub fn visual_confirming_close(&self) -> bool {
        self.layout.confirm_close.is_some()
    }
    /// The dialog's 取消.
    pub fn visual_cancel_close(&mut self, cx: &mut Context<Self>) {
        self.layout.confirm_close = None;
        cx.notify();
    }
    pub fn visual_split_down(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.nav.focused.clone().unwrap();
        self.menu_action(&id, "向下分屏", window, cx);
    }
    pub fn visual_close_focused(&mut self, cx: &mut Context<Self>) {
        let id = self.nav.focused.clone().unwrap();
        self.close_pane(&id, cx);
    }
    /// Closes the shown tab, as its × does; the neighbour that takes over is returned.
    pub fn visual_close_tab(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let id = self.nav.tab.clone().unwrap();
        self.close_tab(&id, cx);
        let tab = self.nav.tab.clone()?;
        for id in attachable_ids(&self.workspace, self.workspace.tab_terminals(&tab)) {
            assert!(
                self.layout.panes.contains_key(&id),
                "the neighbour's panes attach as soon as it takes over"
            );
        }
        Some(tab)
    }
    pub fn visual_assert_new_shell(&self, count: usize, window: &Window, cx: &gpui::App) {
        self.visual_check_panes(count);
        let id = self.nav.focused.as_ref().unwrap();
        assert!(
            self.layout.panes[id].focus_handle(cx).is_focused(window),
            "new split must receive input focus"
        );
        let terminal = self
            .workspace
            .terminals
            .iter()
            .find(|t| &t.id == id)
            .unwrap();
        assert_eq!(terminal.argv, vec!["/bin/sh", "-l"]);
        assert_eq!(
            std::fs::canonicalize(&terminal.cwd).unwrap(),
            std::fs::canonicalize(&self.workspace.terminals[0].cwd).unwrap()
        );
        assert!(self.workspace.agent_sessions.is_empty());
    }
    /// Of the two shells this run opened, `closed` were closed and must have stopped.
    pub fn visual_check_closed_processes(&self, closed: usize) {
        let extras: Vec<_> = self
            .workspace
            .terminals
            .iter()
            .filter(|t| !t.id.starts_with('p'))
            .collect();
        assert_eq!(extras.len(), 2);
        let stopped = extras
            .iter()
            .filter(|t| {
                t.state != ProcessState::Running
                    && unsafe { libc::kill(t.pid.unwrap() as i32, 0) } != 0
            })
            .count();
        assert_eq!(stopped, closed, "closing a pane stops its terminal");
    }
    pub fn visual_marked_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        if let Some(pane) = self
            .nav
            .tab
            .as_ref()
            .and_then(|id| self.workspace.tab_terminals(id).first().cloned())
            .and_then(|id| self.layout.panes.get(&id))
        {
            return Some(pane.update(cx, |pane, cx| pane.verify_marked_overlay(window, cx)));
        }
        None
    }
    pub fn visual_view(&mut self, overview: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.overview = overview;
        self.sync_panes(cx);
        if !overview
            && let Some(pane) = self
                .nav
                .tab
                .as_ref()
                .and_then(|id| self.workspace.tab_terminals(id).first().cloned())
                .and_then(|id| self.layout.panes.get(&id))
        {
            window.focus(&pane.focus_handle(cx), cx);
        }
        cx.notify();
        window.refresh();
    }
    /// The first pane of the shown tab.
    pub fn visual_decorations(&self, cx: &Context<Self>) -> (Vec<String>, Vec<String>) {
        let tab = self.nav.tab.as_ref().expect("a shown tab");
        let id = self.workspace.tab_terminals(tab)[0].clone();
        let lines = self.pane_decorations(&id, cx);
        (lines.title.into_iter().map(|(l, _)| l).collect(), lines.tip)
    }
    pub fn visual_shown_pane(&self) -> Entity<Pane> {
        let tab = self.nav.tab.as_ref().expect("a shown tab");
        let id = self
            .workspace
            .tab_terminals(tab)
            .first()
            .cloned()
            .expect("a pane");
        self.layout.panes[&id].clone()
    }
    /// Whether the shown tab still has its pane and no completion run is left on it.
    pub fn visual_shown_pane_alive(&self) -> bool {
        self.nav.tab.as_ref().is_some_and(|tab| {
            self.workspace.tab_terminals(tab).first().is_some_and(|id| {
                self.layout.panes.contains_key(id) && self.completing(id).is_none()
            })
        })
    }
    pub fn visual_pane_menu_open(&self) -> bool {
        self.layout.menu.is_some()
    }
    pub fn visual_here_open(&self) -> bool {
        self.overview.here.is_some()
    }
    pub fn visual_restart_host(&mut self, cx: &mut Context<Self>) {
        self.close_now(Closing::Host, cx);
    }
    /// The terminal in focus and the one floating over it, for evidence scripts.
    pub fn visual_nav(&self) -> (Option<String>, Option<String>) {
        let peek = self.peek.as_ref().map(|p| p.terminal.clone());
        (self.nav.focused.clone(), peek)
    }
    /// The overview toggle's centre, as last drawn.
    pub fn visual_toggle_point(&self) -> gpui::Point<gpui::Pixels> {
        self.overview.toggle_at.expect("toggle drawn").center()
    }
    /// Overview shown, the tab in view, the space filter, and who holds the keyboard:
    /// the overview itself or a pane of the shown tab.
    pub fn visual_overview_state(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> (bool, Option<String>, Option<String>, &'static str) {
        let focus = if self.overview.focus.is_focused(window) {
            "overview"
        } else if self
            .layout
            .panes
            .values()
            .any(|p| p.focus_handle(cx).contains_focused(window, cx))
        {
            "pane"
        } else {
            "none"
        };
        (
            self.nav.overview,
            self.nav.tab.clone(),
            self.overview.filter_space.clone(),
            focus,
        )
    }
    /// Tab titles in strip order for the shown space, and the spaces left.
    pub fn visual_tab_titles(&self) -> Vec<String> {
        self.workspace
            .tabs
            .iter()
            .filter(|t| t.space == self.nav.space)
            .map(|t| t.title.clone())
            .collect()
    }
    pub fn visual_page_tab(&mut self, title: &str, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self
            .workspace
            .tabs
            .iter()
            .find(|t| visual_tab_is(&self.workspace, t, title))
            .expect("tab")
            .id
            .clone();
        self.nav.tab = Some(tab);
        self.nav.focused = None;
        self.sync_panes(cx);
    }
    pub fn visual_move_pane(&mut self, title: &str, cx: &mut Context<Self>) {
        let pane = self.nav.focused.clone().expect("a focused pane");
        let to = self
            .workspace
            .tabs
            .iter()
            .find(|t| visual_tab_is(&self.workspace, t, title))
            .map(|t| t.id.clone());
        self.move_pane(&pane, to.as_deref(), cx);
    }
    pub fn visual_tab_layout(&self, title: &str) -> usize {
        self.workspace
            .tabs
            .iter()
            .find(|t| visual_tab_is(&self.workspace, t, title))
            .map_or(0, |t| self.workspace.tab_terminals(&t.id).len())
    }
    pub fn visual_open_space_menu(&mut self, id: &str, cx: &mut Context<Self>) {
        self.layout.space_menu =
            Some((id.into(), gpui::point(px(T::S20), px(T::TOP + 4. * T::ROW))));
        self.layout.space_removing = false;
        cx.notify();
    }
    /// Names this space's tabs in strip order, for scripts that address them by name.
    pub fn visual_name_tabs(&mut self, names: &[&str], cx: &mut Context<Self>) {
        let tabs: Vec<_> = self
            .workspace
            .tabs
            .iter()
            .filter(|t| t.space == self.nav.space)
            .cloned()
            .collect();
        for (tab, name) in tabs.into_iter().zip(names) {
            let title = (*name).into();
            self.edit(Edit::RenameTab { id: tab.id, title }, cx);
        }
    }
    /// Shows space `id` as a click on it in the sidebar does.
    pub fn visual_pick_space(&mut self, id: &str, cx: &mut Context<Self>) {
        self.pick_space(id, cx);
    }
    /// The executable of the Agent an empty tab offers first.
    pub fn visual_first_agent(&self) -> String {
        self.first_agent().executable().to_owned()
    }
    /// The empty tab's 「运行 X」 button, pressed.
    pub fn visual_run_agent(&mut self, executable: &str, cx: &mut Context<Self>) {
        let agent = Agent::from_executable(executable).expect("a known Agent");
        self.run_agent_here(agent, cx);
    }
    /// The shown space and tab, and the shown tab's first pane's screen text.
    pub fn visual_shown(&self, cx: &gpui::App) -> (String, Option<String>, String) {
        let text = self
            .nav
            .tab
            .as_ref()
            .and_then(|t| self.workspace.tab_terminals(t).first().cloned())
            .and_then(|id| self.layout.panes.get(&id))
            .map(|p| p.read(cx).visual_text())
            .unwrap_or_default();
        (self.nav.space.clone(), self.nav.tab.clone(), text)
    }
}

/// A tab the visual runs name: by what it shows, its own pinned name, or its terminal
/// (a tab holding one conversation hands its name to that conversation).
pub(super) fn visual_tab_is(w: &Workspace, tab: &Tab, title: &str) -> bool {
    tab.title == title
        || w.tab_title(tab, &std::collections::HashMap::new(), None) == title
        || w.terminal_tab(title).is_some_and(|t| t.id == tab.id)
}
