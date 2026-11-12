//! Spaces added, renamed and broken for the space evidence.
use super::*;

impl Workbench {
    /// Adds a space with a fixed id beside the current one.
    pub fn visual_add_space(&mut self, id: &str, cwd: &str, cx: &mut Context<Self>) {
        self.edit(
            Edit::AddSpace(Space {
                id: id.into(),
                name: id.into(),
                cwd: cwd.into(),
            }),
            cx,
        );
    }
    /// The sidebar's 「＋ 空间」, pressed.
    pub fn visual_new_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_space(window, cx);
    }
    /// Whether the pane menu shows 「为什么是这个状态」.
    pub fn visual_menu_why(&self) -> bool {
        self.layout
            .menu
            .as_ref()
            .is_some_and(|m| matches!(m.page, MenuPage::Why))
    }
    /// The empty tab's 「换目录」, pressed for the current space.
    pub fn visual_change_dir(&mut self, cx: &mut Context<Self>) {
        self.layout.choosing_dir = Some(self.nav.space.clone());
        cx.notify();
    }
    pub fn visual_spaces(&self) -> Vec<String> {
        self.workspace.spaces.iter().map(|s| s.id.clone()).collect()
    }
    /// Whether the space menu is open, the rename field's text and focus if any, and
    /// each space's name and directory.
    pub fn visual_space_state(&self, window: &Window, cx: &gpui::App) -> VisualSpaceState {
        (
            self.layout.space_menu.is_some(),
            self.layout.renaming_space.as_ref().map(|(_, field, _)| {
                (
                    field.read(cx).text.clone(),
                    field.focus_handle(cx).is_focused(window),
                )
            }),
            self.workspace
                .spaces
                .iter()
                .map(|s| (s.name.clone(), s.cwd.clone()))
                .collect(),
        )
    }
    pub fn visual_space_menu_point(&self, index: usize) -> gpui::Point<gpui::Pixels> {
        let (_, position) = self
            .layout
            .space_menu
            .as_ref()
            .expect("right mouse opened space menu");
        *position + gpui::point(px(T::S20), px(T::S6 + T::ROW * (index as f32 + 0.5)))
    }
    pub fn visual_space_rename_type(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some((_, field, _)) = &self.layout.renaming_space {
            field.update(cx, |f, cx| {
                f.set_text(text);
                cx.notify();
            });
        }
    }
    /// Whether the pane the keyboard should be on holds the keyboard.
    pub fn visual_pane_has_keyboard(&self, window: &Window, cx: &gpui::App) -> bool {
        (self.nav.focused.as_ref())
            .and_then(|id| self.layout.panes.get(id))
            .is_some_and(|pane| pane.focus_handle(cx).is_focused(window))
    }
    pub fn visual_rename_tab(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.rename_tab(id, window, cx);
    }
    /// Opens the name field of a session made up for this window, on the first terminal.
    pub fn visual_rename_probe_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = self.workspace.terminals.first().map(|t| t.id.clone());
        self.workspace.agent_sessions.push(AgentSession {
            id: "rename-probe".into(),
            runtime: Agent::Claude,
            cli_session_id: "rename-probe".into(),
            terminal,
            current: true,
            task: None,
            outcome: Outcome::Open,
            updated: now(),
            cwd: String::new(),
            transcript: String::new(),
            name: "探针会话".into(),
            binding: None,
        });
        // With no content of its own the row stays out of the list.
        self.workspace.content.insert(
            "rename-probe".into(),
            mooring_core::content::Content {
                title: "探针会话".into(),
                updated: now(),
                interactive: true,
                ..Default::default()
            },
        );
        self.facts.forget();
        self.rename_session("rename-probe", window, cx);
    }
    /// The session name field's text, and whether it holds the keyboard.
    pub fn visual_session_field(&self, window: &Window, cx: &gpui::App) -> Option<(String, bool)> {
        self.layout.renaming_session.as_ref().map(|(_, field, _)| {
            (
                field.read(cx).text.clone(),
                field.focus_handle(cx).is_focused(window),
            )
        })
    }
    pub fn visual_rename_first_space(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.workspace.spaces.first().map(|s| s.id.clone()) {
            self.rename_space(&id, window, cx);
        }
    }
    /// Points the first space at a folder that is gone, as a deleted worktree does.
    pub fn visual_lose_space_dir(&mut self, cx: &mut Context<Self>) {
        if let Some(space) = self.workspace.spaces.first() {
            let cwd = "/nonexistent/mooring-space-evidence".into();
            self.edit(
                Edit::UpdateSpace(Space {
                    cwd,
                    ..space.clone()
                }),
                cx,
            );
        }
    }
}
