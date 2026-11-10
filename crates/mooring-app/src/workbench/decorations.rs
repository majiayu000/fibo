//! Plugin pane decorations (`mooring_core::decoration`): each command runs in the
//! background for the pane's directory and session, and the line it last printed is drawn
//! in the pane's title bar or its title's hover card. Drawing only reads what is cached;
//! a line due again is shown as it was until the new run lands.
use super::*;
use mooring_core::decoration::{self, Decoration, Pane as DecoratedPane, Place, Tone};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

struct Shown {
    ran: Instant,
    line: Option<String>,
    running: bool,
}
/// (decoration index, pane facts) -> its last run.
static SHOWN: LazyLock<Mutex<HashMap<(usize, DecoratedPane), Shown>>> =
    LazyLock::new(Default::default);
/// Runs not drawn for this long are forgotten once the cache grows.
const FORGET: Duration = Duration::from_secs(600);
const KEEP: usize = 256;

/// Bumped by `forget`: a run that started before it reports a decoration that may no longer
/// be at its index, so what it finds is dropped.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Whether a run started in generation `started` may still write its line.
fn current(started: u64) -> bool {
    GENERATION.load(std::sync::atomic::Ordering::SeqCst) == started
}

/// Drops every cached line: the decorations were reloaded and their indexes moved.
pub(super) fn forget() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut shown) = SHOWN.lock() {
        shown.clear();
    }
}

/// A pane's decoration lines, by where they go.
#[derive(Default)]
pub(super) struct Lines {
    pub(super) title: Vec<(String, Tone)>,
    pub(super) tip: Vec<String>,
}

impl Workbench {
    /// Plugins were read again: decorations and WASM sidebars cache by position and file, and
    /// which Agents exist may have changed, so all of that starts afresh.
    pub(super) fn plugins_read_again(&mut self, cx: &mut Context<Self>) {
        forget();
        self.sidebar.wasm = Default::default();
        self.tasks.found = None;
        // The welcome page lists what the shell finds and nothing else asks for it there.
        if self.first_run.on {
            self.probe_agents(cx);
        }
        cx.refresh_windows();
    }
    /// Plugins changed from a terminal or another window: the host said so, read them here.
    pub(super) fn follow_plugins(&mut self, revision: u64, cx: &mut Context<Self>) {
        let before = self.store.recognition().plugins_revision;
        if revision != before && before != 0 {
            mooring_core::plugin::reload();
            self.plugins_read_again(cx);
        }
    }
    pub(super) fn pane_decorations(&self, terminal: &str, cx: &Context<Self>) -> Lines {
        let mut lines = Lines::default();
        if decoration::all().is_empty() {
            return lines;
        }
        let ctx = self.action_context(terminal, Some(cx));
        if ctx.cwd.is_empty() {
            return lines;
        }
        let pane = DecoratedPane {
            cwd: ctx.cwd,
            agent: ctx.agent,
            session: ctx.session_id,
            last: self
                .store
                .recognition()
                .terminal_commands
                .get(terminal)
                .copied(),
        };
        for (i, d) in decoration::all().iter().enumerate() {
            if !d.shown(&pane) {
                continue;
            }
            if let Some(line) = self.decoration_line(i, d, &pane, cx) {
                match d.place {
                    Place::Title => lines.title.push((line, d.tone)),
                    Place::Tip => lines.tip.push(line),
                }
            }
        }
        lines
    }
    /// The cached line; a missing or due one is run in the background, which redraws when
    /// it lands and again when it is due, so the line stays current while it is shown.
    fn decoration_line(
        &self,
        i: usize,
        d: &'static Decoration,
        pane: &DecoratedPane,
        cx: &Context<Self>,
    ) -> Option<String> {
        let mut shown = SHOWN.lock().ok()?;
        let key = (i, pane.clone());
        let old = match shown.get(&key) {
            Some(s) if s.running || s.ran.elapsed() < d.refresh() => return s.line.clone(),
            Some(s) => s.line.clone(),
            None => None,
        };
        if shown.len() > KEEP {
            shown.retain(|_, s| s.running || s.ran.elapsed() < FORGET);
        }
        shown.insert(
            key.clone(),
            Shown {
                ran: Instant::now(),
                line: old.clone(),
                running: true,
            },
        );
        drop(shown);
        let generation = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        cx.spawn(async move |this, cx| {
            let pane = key.1.clone();
            let line = cx
                .background_spawn(async move {
                    d.run(&pane).unwrap_or_else(|e| {
                        eprintln!("窗格装饰 {}：{e}", d.id);
                        None
                    })
                })
                .await;
            if !current(generation) {
                return;
            }
            let changed = SHOWN.lock().is_ok_and(|mut shown| {
                let before = shown.insert(
                    key,
                    Shown {
                        ran: Instant::now(),
                        line: line.clone(),
                        running: false,
                    },
                );
                before.and_then(|s| s.line) != line
            });
            if changed {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
            // Due again: a redraw runs it once more if the pane is still on screen.
            cx.background_executor().timer(d.refresh()).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run still going when plugins reload finds its generation gone and writes nothing.
    #[test]
    fn a_run_from_before_a_reload_may_not_write_its_line() {
        let started = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        assert!(current(started));
        forget();
        assert!(!current(started));
    }
}
