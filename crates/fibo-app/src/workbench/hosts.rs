//! Other machines in this window (docs/decisions/2026-11-14-one-window-many-hosts.md, step
//! 2): a machine's own host, worked by a workbench of its own, the same code as this one's,
//! drawn beside this window's own sidebar while one of its spaces or sessions is picked
//! (docs/decisions/2026-11-15-one-ui-many-hosts.md, step 1): the sidebar is the window's and
//! stays as it is, only the main area is that machine's. One of this machine's spaces or
//! sessions in the sidebar, or 「← 本机」, comes back; each keeps its tabs, splits and focus.
use super::*;

#[derive(Default)]
pub(super) struct Hosts {
    /// A workbench for each machine opened here, by its host in machines.json.
    views: Vec<(String, Entity<Workbench>)>,
    /// The one drawn in this window's place; None draws this workbench.
    showing: Option<String>,
    /// Being reached: its forward and first workspace come in the background.
    pub(super) opening: Option<String>,
    /// This workbench is another machine's, drawn in a window of this one.
    pub(super) embedded: bool,
    /// Drawn embedded: how wide the window's own sidebar is beside it, None while folded.
    pub(super) beside: Option<f32>,
    /// Where to go the next time it draws: asked for before it was shown.
    go: Option<Go>,
}

/// Where another machine's workbench opens at.
#[derive(Clone)]
pub(super) enum Go {
    Space(String),
    Terminal(String),
}

/// What a machine's workbench drawn in this window asks of it.
pub(super) enum HostEvent {
    /// 「← 本机」.
    Home,
    /// Its sidebar toggle or ⌘\: the window's own sidebar folds or comes back.
    ToggleSidebar,
}
impl gpui::EventEmitter<HostEvent> for Workbench {}

impl Workbench {
    /// The machine's workbench drawn in this one's place, if one is picked.
    pub(super) fn hosted_view(&self) -> Option<Entity<Workbench>> {
        let host = self.hosts.showing.as_ref()?;
        (self.hosts.views.iter())
            .find(|(h, _)| h == host)
            .map(|(_, v)| v.clone())
    }
    /// `host`'s own workbench in this window's place, at `go`; reached the first time.
    pub(super) fn show_host(
        &mut self,
        host: String,
        go: Option<Go>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, view)) = self.hosts.views.iter().find(|(h, _)| *h == host) {
            view.update(cx, |w, cx| {
                w.hosts.go = go;
                w.nav.pending_focus = true;
                cx.notify();
            });
            self.hosts.showing = Some(host);
            cx.notify();
            return;
        }
        if self.hosts.opening.is_some() {
            return;
        }
        self.hosts.opening = Some(host.clone());
        cx.notify();
        let reach = cx.background_spawn({
            let host = host.clone();
            async move {
                let root = crate::attach::root(&host)?;
                crate::attach::start(&host, &root)?;
                let workspace = crate::load_workspace(&root)?;
                std::io::Result::Ok((root, workspace))
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let reached = reach.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.hosts.opening = None;
                match reached {
                    Ok((root, workspace)) => {
                        let socket = root.join("workbench.sock");
                        let view = cx.new(|cx| Workbench::new(socket, workspace, window, cx));
                        view.update(cx, |w, _| {
                            w.hosts.embedded = true;
                            w.hosts.go = go;
                        });
                        cx.subscribe_in(&view, window, |this, _, event, window, cx| match event {
                            HostEvent::Home => this.show_home(window, cx),
                            HostEvent::ToggleSidebar => this.toggle_sidebar(cx),
                        })
                        .detach();
                        this.hosts.views.push((host.clone(), view));
                        this.hosts.showing = Some(host);
                    }
                    Err(e) => this.error = Some(t!("没能打开 {host}：{e}", host = &host, e)),
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// This window's sidebar beside another machine's main area: the sidebar stays as it
    /// is, only the main area, its title row and status bar are that machine's.
    pub(super) fn hosted_frame(
        &mut self,
        view: Entity<Workbench>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.before_render(window, cx);
        let sidebar = self.cached_sidebar(window, cx);
        let beside = sidebar.is_some().then(|| self.sidebar_width());
        view.update(cx, |w, cx| {
            if w.hosts.beside != beside {
                w.hosts.beside = beside;
                cx.notify();
            }
        });
        let row = div()
            .flex_1()
            .min_h_0()
            .flex()
            .children(sidebar)
            .child(div().flex_1().min_w_0().h_full().child(view));
        let root = Self::window_root(cx)
            .relative()
            .child(row)
            .children(self.turning_view(cx));
        self.window_layers(root, window, cx)
    }
    /// The machine drawn here and the space its main area shows, or is about to show.
    pub(super) fn shown_space(&self, cx: &gpui::App) -> Option<(String, String)> {
        let host = self.hosts.showing.clone()?;
        let view = self.hosted_view()?;
        let w = view.read(cx);
        let space = match &w.hosts.go {
            Some(Go::Space(id)) => Some(id.clone()),
            Some(Go::Terminal(id)) => w.workspace.terminal_tab(id).map(|t| t.space.clone()),
            None => None,
        };
        Some((host, space.unwrap_or_else(|| w.nav.space.clone())))
    }
    /// Whether the sidebar is folded away; drawn embedded, whether the window's own is.
    pub(super) fn sidebar_folded(&self) -> bool {
        match self.hosts.embedded {
            true => self.hosts.beside.is_none(),
            false => self.sidebar.collapsed,
        }
    }
    /// One of this machine's spaces or sessions picked: its main area again.
    pub(super) fn leave_host(&mut self) {
        if self.hosts.showing.take().is_some() {
            self.nav.pending_focus = true;
        }
    }
    /// 「← 本机」: this window's own workbench again.
    pub(super) fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hosts.showing = None;
        self.nav.pending_focus = true;
        window.refresh();
        cx.notify();
    }
    /// Each frame: goes where it was asked to while it was not drawn.
    pub(super) fn settle_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.hosts.go.take() {
            Some(Go::Terminal(id)) => {
                self.focus_terminal(&id, window, cx);
            }
            Some(Go::Space(id)) => self.pick_space(&id, cx),
            None => {}
        }
    }
    /// The status bar's way back, on another machine's workbench drawn in this window.
    pub(super) fn home_button(&self, cx: &Context<Self>) -> Option<gpui::Stateful<gpui::Div>> {
        if !self.hosts.embedded {
            return None;
        }
        let home = t!("← 本机");
        Some(
            div()
                .id("host-home")
                .role(gpui::accesskit::Role::Button)
                .aria_label(t!("回到本机"))
                .flex_shrink_0()
                .px(px(T::S6))
                .rounded(px(T::CONTROL_RADIUS))
                .cursor_pointer()
                .text_color(T::color(T::link()))
                .hover(|s| s.bg(T::color(T::button_hover())))
                .child(home)
                .on_click(cx.listener(|_, _, _, cx| cx.emit(HostEvent::Home))),
        )
    }
    /// A machine's session from this window's sidebar or board: in its own workbench here,
    /// or floated up for a look when peek is turned on in settings. False leaves it to peek.
    pub(super) fn open_on_host(
        &mut self,
        index: usize,
        go: Go,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if crate::ui::peek::enabled(cx) || self.host.attached() {
            return false;
        }
        let Some(host) = self.machine_host(index) else {
            return false;
        };
        self.show_host(host, Some(go), window, cx);
        true
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Which machine's workbench is drawn here, and whether one is still being reached.
    pub fn visual_hosts(&self) -> (Option<String>, Option<String>) {
        (self.hosts.showing.clone(), self.hosts.opening.clone())
    }
    pub fn visual_hosted(&self) -> Option<Entity<Workbench>> {
        self.hosted_view()
    }
    pub fn visual_show_host(
        &mut self,
        host: &str,
        space: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let go = space.map(|s| Go::Space(s.to_owned()));
        self.show_host(host.to_owned(), go, window, cx);
    }
    pub fn visual_go_home(&mut self, cx: &mut Context<Self>) {
        cx.emit(HostEvent::Home);
    }
    pub fn visual_shown_space(&self, cx: &gpui::App) -> Option<(String, String)> {
        self.shown_space(cx)
    }
    /// Drawn embedded: how wide the window's own sidebar beside it was, None when it was
    /// not drawn beside one.
    pub fn visual_beside(&self) -> Option<f32> {
        self.hosts.beside
    }
}
