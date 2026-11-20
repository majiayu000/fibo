//! Other machines in this window (docs/decisions/2026-11-14-one-window-many-hosts.md, step
//! 2): a machine's own host, worked by a workbench of its own, the same code as this one's,
//! drawn beside this window's own sidebar while one of its spaces or sessions is picked
//! (docs/decisions/2026-11-15-one-ui-many-hosts.md, step 1): the sidebar is the window's and
//! stays as it is, only the main area is that machine's. One of this machine's spaces or
//! sessions in the sidebar, or 「← 本机」, comes back; each keeps its tabs, splits and focus.
use super::*;
use crate::machines::Link;

#[derive(Default)]
pub(super) struct Hosts {
    /// A workbench for each machine opened here, by its host in machines.json.
    views: Vec<(String, Entity<Workbench>)>,
    pub(super) outcomes: Vec<(RequestId, String)>,
    pub(super) closing: Option<(RequestId, String)>,
    /// The one drawn in this window's place; None draws this workbench.
    showing: Option<String>,
    /// Being reached: their forwards and first workspaces come in the background.
    pub(super) opening: Vec<String>,
    /// The machine last picked before it was reached, and where to go there: drawn once
    /// it is.
    wanted: Option<(String, Option<Go>)>,
    /// This workbench is another machine's, drawn in a window of this one.
    pub(super) embedded: bool,
    /// Embedded and the one drawn now. Kept undrawn it only follows its machine's
    /// workspace for the sidebar: it opens no terminal there and takes no input (`undrawn`).
    shown: bool,
    /// Drawn embedded: how wide the window's own sidebar is beside it, None while folded.
    pub(super) beside: Option<f32>,
    /// Drawn embedded: how many need you on every machine, as the window counts them.
    pub(super) window_wants: Option<(usize, fibo_core::status::Status)>,
    /// Where to go the next time it draws: asked for before it was shown.
    go: Option<Go>,
    /// The machines list changed: reach those new to it on the next frame.
    pub(super) reach_again: bool,
}

/// How long a machine that could not be reached waits before it is tried again.
const RETRY: std::time::Duration = std::time::Duration::from_secs(15);
/// How often a machine's round trip is timed.
const PING_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

fn socket_of(root: &std::path::Path) -> std::path::PathBuf {
    root.join("workbench.sock")
}

/// One request's round trip on a link already open: what typing there waits for. Opening
/// the link through its forward costs more, so it is left out.
fn round_trip(socket: &std::path::Path) -> std::io::Result<std::time::Duration> {
    use fibo_core::protocol::{CONTROL, MAJOR, Reply, receive, send};
    let mut stream = fibo_platform::local::Stream::connect(socket)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    send(&mut stream, CONTROL, &Request::Hello { major: MAJOR })?;
    receive::<Reply>(&mut stream)?;
    let at = std::time::Instant::now();
    // A terminal no one has: answered at once, whatever the host's version.
    let probe = Request::TerminalCwd { id: String::new() };
    send(&mut stream, CONTROL, &probe)?;
    receive::<Reply>(&mut stream)?;
    Ok(at.elapsed())
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
    Receipt {
        id: Option<RequestId>,
        error: Option<String>,
    },
    /// Its sidebar toggle or ⌘\: the window's own sidebar folds or comes back.
    ToggleSidebar,
    /// 下一个要你的 (⌘J or its chip): the next on any machine, as the window finds it.
    NextWaiting,
    /// 鸟瞰: the window's board, every machine's sessions on it.
    Overview,
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
            self.draw_host(host, cx);
            cx.notify();
            return;
        }
        self.hosts.wanted = Some((host.clone(), go));
        self.reach_host(host, window, cx);
        cx.notify();
    }
    /// Each machine in machines.json reached when the window opens, and kept: its main area
    /// is there at the first click, and its workspace is this window's to read meanwhile.
    pub(super) fn reach_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Another machine's own view has none beside it.
        if self.host.attached() {
            return;
        }
        let hosts: Vec<String> = (self.machines.iter())
            .map(|m| m.view.lock().unwrap().spec.clone())
            .filter(|spec| !spec.quiet)
            .map(|spec| spec.host)
            .collect();
        for host in hosts {
            self.reach_host(host, window, cx);
        }
    }
    pub(super) fn reload_machines(&mut self, cx: &mut Context<Self>) {
        let fresh = Self::start_machines(&self.host);
        let mut old = std::mem::take(&mut self.machines);
        self.machines = fresh
            .into_iter()
            .map(|remote| {
                let spec = remote.view.lock().unwrap().spec.clone();
                match old
                    .iter()
                    .position(|m| m.view.lock().unwrap().spec.host == spec.host)
                {
                    Some(i) => {
                        let kept = old.remove(i);
                        kept.view.lock().unwrap().spec = spec;
                        kept
                    }
                    None => remote,
                }
            })
            .collect();
        self.close_peek(cx); // Its machine index may have changed.
        for removed in old {
            let host = removed.view.lock().unwrap().spec.host.clone();
            if self.hosts.showing.as_ref() == Some(&host) {
                self.leave_host(cx);
            }
            self.hosts.views.retain(|(h, _)| h != &host);
            self.hosts.opening.retain(|h| h != &host);
            if self.hosts.wanted.as_ref().is_some_and(|(h, _)| h == &host) {
                self.hosts.wanted = None;
            }
            crate::attach::forget(&host);
        }
        self.hosts.reach_again = true;
    }
    /// Reaches `host` in the background once; drawn when it was the one last picked. One
    /// that cannot be reached shows why in the sidebar and is tried again (`RETRY`); only a
    /// pick of it says so on the error line.
    fn reach_host(&mut self, host: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.machine_of(&host).is_none() {
            return;
        }
        let known = self.hosts.views.iter().any(|(h, _)| *h == host);
        if known || self.hosts.opening.contains(&host) {
            return;
        }
        self.hosts.opening.push(host.clone());
        let reach = cx.background_spawn({
            let host = host.clone();
            async move {
                let root = crate::attach::root(&host)?;
                crate::attach::start(&host, &root)?;
                let workspace = crate::load_workspace(&root).map_err(|e| {
                    std::io::Error::other(t!(
                        "无法读取 {host} 的工作区：{e}。请确认那台电脑已打开 Fibo，再重试",
                        host,
                        e
                    ))
                })?;
                std::io::Result::Ok((root, workspace))
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let reached = reach.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.hosts.opening.retain(|h| *h != host);
                if this.machine_of(&host).is_none() {
                    crate::attach::forget(&host);
                    return;
                }
                let wanted = (this.hosts.wanted.as_ref())
                    .is_some_and(|(h, _)| *h == host)
                    .then(|| this.hosts.wanted.take())
                    .flatten();
                match reached {
                    Ok((root, workspace)) => {
                        let socket = socket_of(&root);
                        let view = cx.new(|cx| Workbench::embedded(socket, workspace, window, cx));
                        cx.subscribe_in(&view, window, |this, _, event, window, cx| match event {
                            HostEvent::Receipt { id, error } => {
                                this.remote_receipt(id.as_ref(), error.as_deref(), cx)
                            }
                            HostEvent::Home => this.show_home(window, cx),
                            HostEvent::ToggleSidebar => this.toggle_sidebar(cx),
                            HostEvent::NextWaiting => this.next_waiting(window, cx),
                            HostEvent::Overview => {
                                this.leave_host(cx);
                                this.show_overview(cx);
                                cx.notify();
                            }
                        })
                        .detach();
                        // Its rows in this sidebar, from its own workspace from now on.
                        let feed = host.clone();
                        cx.observe(&view, move |this, view, cx| {
                            this.feed_machine(&feed, &view, cx)
                        })
                        .detach();
                        this.name_machine(&host, &root);
                        this.feed_machine(&host, &view, cx);
                        this.time_machine(host.clone(), socket_of(&root), cx);
                        this.hosts.views.push((host.clone(), view.clone()));
                        if let Some((_, go)) = wanted {
                            view.update(cx, |w, _| w.hosts.go = go);
                            this.draw_host(host, cx);
                        }
                    }
                    Err(e) => {
                        this.set_link(&host, Link::Offline(e.to_string()));
                        if wanted.is_some() {
                            this.error = Some(t!("没能打开 {host}：{e}", host = &host, e));
                        }
                        let again = host.clone();
                        cx.spawn_in(window, async move |this, cx| {
                            cx.background_executor().timer(RETRY).await;
                            let _ = this.update_in(cx, |this, window, cx| {
                                this.reach_host(again, window, cx)
                            });
                        })
                        .detach();
                    }
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
        self.forget_tab_marks();
        let sidebar = self.cached_sidebar(window, cx);
        let beside = sidebar.is_some().then(|| self.sidebar_width());
        let wants = Some(self.wants());
        view.update(cx, |w, cx| {
            if w.hosts.beside != beside || w.hosts.window_wants != wants {
                w.hosts.beside = beside;
                w.hosts.window_wants = wants;
                cx.notify();
            }
        });
        // A peek floats over the main area here too, as over this machine's.
        let peek = (self.peek.as_ref()).map(|peek| self.peek_view(peek, cx));
        let main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .child(view)
            .children(peek);
        let row = div()
            .flex_1()
            .min_h_0()
            .flex()
            .children(sidebar)
            .child(main);
        let root = Self::window_root(cx)
            .relative()
            .child(row)
            .children(self.turning_view(cx));
        self.window_layers(root, window, cx)
    }
    /// `host`'s spaces and cards in this sidebar, made from its own workspace by the rules
    /// its host lists them with for phones (`remote_cards`), and whether its link holds;
    /// redrawn only when something changed.
    fn feed_machine(&mut self, host: &str, view: &Entity<Workbench>, cx: &mut Context<Self>) {
        let Some(machine) = self.machine_of(host) else {
            return;
        };
        let (spaces, cards, link, pins) = {
            let w = view.read(cx);
            let now = fibo_core::content::now();
            let (spaces, _) = fibo_core::remote_cards::spaces(&w.workspace);
            let cards = fibo_core::remote_cards::cards(&w.workspace, w.store.recognition(), now);
            let link = match w.store.cut_off_reason() {
                Some(why) if w.store.cut_off() => Link::Offline(why.to_owned()),
                _ => Link::Online,
            };
            (spaces, cards, link, w.workspace.pinned_terminals.clone())
        };
        let mut v = machine.view.lock().unwrap();
        let changed =
            v.link != link || v.spaces != spaces || v.terminals != cards || v.pins != pins;
        v.link = link;
        v.spaces = spaces;
        v.terminals = cards;
        v.pins = pins;
        drop(v);
        if changed {
            cx.notify();
        }
    }
    fn machine_of(&self, host: &str) -> Option<&crate::machines::Remote> {
        (self.machines.iter()).find(|m| m.view.lock().unwrap().spec.host == host)
    }
    fn set_link(&mut self, host: &str, link: Link) {
        if let Some(machine) = self.machine_of(host) {
            machine.view.lock().unwrap().link = link;
        }
    }
    /// Its name and id as its host gives them to phones, read when it was reached.
    fn name_machine(&mut self, host: &str, root: &std::path::Path) {
        let Some((id, name)) = crate::attach::Host::of(root).machine() else {
            return;
        };
        let Some(machine) = self.machine_of(host) else {
            return;
        };
        let agent_names = (fibo_core::agent::Agent::all().into_iter())
            .map(|a| (a.key().to_lowercase(), a.name().to_owned()))
            .collect();
        machine.view.lock().unwrap().machine = Some(fibo_remote::protocol::Machine {
            id: if id.is_empty() { host.to_owned() } else { id },
            name: if name.is_empty() {
                host.to_owned()
            } else {
                name
            },
            agent_names,
        });
    }
    /// The round trip shown beside its name, timed every `PING_EVERY` on its own link while
    /// this window lasts; gone while that link is down.
    fn time_machine(&mut self, host: String, socket: std::path::PathBuf, cx: &mut Context<Self>) {
        let owner = self.machine_of(&host).unwrap().view.clone();
        cx.spawn(async move |this, cx| {
            loop {
                if !this
                    .update(cx, |this, _| {
                        this.machine_of(&host)
                            .is_some_and(|m| std::sync::Arc::ptr_eq(&m.view, &owner))
                    })
                    .unwrap_or(false)
                {
                    crate::machines::set_latency(&host, None);
                    return;
                }
                let socket = socket.clone();
                let rtt = cx
                    .background_spawn(async move { round_trip(&socket).ok() })
                    .await;
                let moved = crate::machines::set_latency(&host, rtt);
                if this
                    .update(cx, |_, cx| {
                        if moved {
                            cx.notify()
                        }
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(PING_EVERY).await;
            }
        })
        .detach();
    }
    /// The workbench of machine `index` in machines.json, once reached; drawn or not.
    pub(super) fn machine_view(&self, index: usize) -> Option<Entity<Workbench>> {
        let host = self
            .machines
            .get(index)?
            .view
            .lock()
            .unwrap()
            .spec
            .host
            .clone();
        (self.hosts.views.iter())
            .find(|(h, _)| *h == host)
            .map(|(_, v)| v.clone())
    }
    /// The terminal focused in the machine drawn here, as 下一个要你的 names it.
    pub(super) fn hosted_focus(&self, cx: &gpui::App) -> Option<String> {
        let host = self.hosts.showing.as_ref()?;
        let terminal = self.hosted_view()?.read(cx).nav.focused.clone()?;
        let machine = (self.machines.iter()).find(|m| m.view.lock().unwrap().spec.host == *host)?;
        Some(super::machines::remote_key(
            &machine.view.lock().unwrap(),
            &terminal,
        ))
    }
    /// Whether the sidebar is folded away; drawn embedded, whether the window's own is.
    pub(super) fn sidebar_folded(&self) -> bool {
        match self.hosts.embedded {
            true => self.hosts.beside.is_none(),
            false => self.sidebar.collapsed,
        }
    }
    /// Whether this is another machine's workbench kept in a window without being drawn.
    /// What a workbench does on its own when it shows a tab (taking the tab's input,
    /// bringing back a shell its host lost) waits until it is the one drawn: until then
    /// that machine's own window is where someone is typing.
    pub(super) fn undrawn(&self) -> bool {
        self.hosts.embedded && !self.hosts.shown
    }
    /// `host`'s workbench is the one drawn from now on; the one drawn before is put away.
    fn draw_host(&mut self, host: String, cx: &mut Context<Self>) {
        if self.hosts.showing.as_ref() != Some(&host) {
            self.put_away(cx);
        }
        self.hosts.showing = Some(host);
    }
    /// The machine drawn here is no longer drawn; false when none was.
    fn put_away(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(host) = self.hosts.showing.take() else {
            return false;
        };
        if let Some((_, view)) = self.hosts.views.iter().find(|(h, _)| *h == host) {
            view.update(cx, |w, _| w.hosts.shown = false);
        }
        true
    }
    /// One of this machine's spaces or sessions picked: its main area again.
    pub(super) fn leave_host(&mut self, cx: &mut Context<Self>) {
        if self.put_away(cx) {
            self.nav.pending_focus = true;
        }
    }
    /// 「← 本机」: this window's own workbench again.
    pub(super) fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.put_away(cx);
        self.nav.pending_focus = true;
        window.refresh();
        cx.notify();
    }
    /// Each frame: goes where it was asked to while it was not drawn.
    pub(super) fn settle_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.hosts.reach_again) {
            self.reach_hosts(window, cx);
        }
        // Drawn for the first time, or again after another was: the shown tab's panes are
        // made now, and its machine's hooks read as a window coming to the front reads them.
        if self.undrawn() {
            self.hosts.shown = true;
            self.sync_panes(cx);
            self.read_hooks(cx);
        }
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
        (
            self.hosts.showing.clone(),
            self.hosts.opening.first().cloned(),
        )
    }
    /// Whether `host` is online here as its own host says, and its spaces there.
    pub fn visual_machine_rows(&self, host: &str) -> Option<(bool, Vec<String>)> {
        let machine = (self.machines.iter()).find(|m| m.view.lock().unwrap().spec.host == host)?;
        let v = machine.view.lock().unwrap();
        Some((
            v.link == Link::Online,
            v.spaces.iter().map(|s| s.id.clone()).collect(),
        ))
    }
    /// The terminal input goes to, here.
    pub fn visual_focused(&self) -> Option<String> {
        self.nav.focused.clone()
    }
    /// The title row's 鸟瞰 pressed.
    pub fn visual_press_board(&mut self, cx: &mut Context<Self>) {
        self.toggle_overview(false, cx);
    }
    /// Whether the board shows, and the count the title row shows as the window's own.
    pub fn visual_board_and_wants(&self) -> (bool, Option<usize>) {
        (self.nav.overview, self.hosts.window_wants.map(|(n, _)| n))
    }
    /// Machine `index`'s own workbench here, once reached.
    pub fn visual_machine_workbench(&self, index: usize) -> Option<Entity<Workbench>> {
        self.machine_view(index)
    }
    /// The open peek's pane, wherever it is drawn.
    pub fn visual_peek_bounds(&self, cx: &gpui::App) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.peek.as_ref()?.pane.read(cx).visual_bounds()
    }
    /// `host`'s own workbench here, once reached.
    pub fn visual_failed_host_request(&mut self, cx: &mut Context<Self>) {
        self.request(
            Request::SendText {
                id: "missing-evidence-terminal".into(),
                session: None,
                text: "evidence".into(),
                submit: false,
            },
            cx,
        );
    }
    pub fn visual_host_view(&self, host: &str) -> Option<Entity<Workbench>> {
        (self.hosts.views.iter())
            .find(|(h, _)| h == host)
            .map(|(_, v)| v.clone())
    }
    /// Embedded: whether it is the one drawn, the panes it holds, and the terminals its
    /// machine has.
    pub fn visual_embedded_state(&self) -> (bool, usize, usize) {
        (
            self.hosts.shown,
            self.layout.panes.len(),
            self.workspace.terminals.len(),
        )
    }
    /// The programs tab `id`'s terminals run.
    pub fn visual_tab_programs(&self, id: &str) -> Vec<String> {
        let ids = self.workspace.tab_terminals(id);
        (self.workspace.terminals.iter())
            .filter(|t| ids.contains(&t.id))
            .filter_map(|t| t.argv.first().cloned())
            .collect()
    }
    /// Whether `host` has its own workbench here, drawn or not.
    pub fn visual_reached(&self, host: &str) -> bool {
        self.hosts.views.iter().any(|(h, _)| h == host)
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
    /// `host`'s space chip in this sidebar clicked.
    pub fn visual_pick_machine_space(
        &mut self,
        host: &str,
        space: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = (self.machines.iter())
            .position(|m| m.view.lock().unwrap().spec.host == host)
            .expect("a machine in machines.json");
        self.pick_machine_space(index, space, window, cx);
    }
    /// The one space of another machine the list is narrowed to.
    pub fn visual_machine_space_filter(&self) -> Option<String> {
        self.sidebar.machine_space.clone()
    }
    pub fn visual_go_home(&mut self, cx: &mut Context<Self>) {
        cx.emit(HostEvent::Home);
    }
    /// The machine drawn here and the space its main area shows, or is about to show.
    pub fn visual_shown_space(&self, cx: &gpui::App) -> Option<(String, String)> {
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
    /// Whether the shown machine's space chip `id` is lit in this sidebar.
    pub fn visual_machine_chip_lit(&self, id: &str) -> bool {
        self.machine_chip_lit(id)
    }
    /// Drawn embedded: how wide the window's own sidebar beside it was, None when it was
    /// not drawn beside one.
    pub fn visual_beside(&self) -> Option<f32> {
        self.hosts.beside
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Puts `view` in as machine `host`'s workbench, or takes it out with None.
    pub(super) fn visual_set_host_view(&mut self, host: &str, view: Option<Entity<Workbench>>) {
        self.hosts.views.retain(|(h, _)| h != host);
        if let Some(view) = view {
            self.hosts.views.push((host.to_owned(), view));
        }
    }
}
