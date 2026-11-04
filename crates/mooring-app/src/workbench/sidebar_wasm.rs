//! Left-column presets arranged by a plugin's WebAssembly module (`"group": "wasm"`).
//!
//! The module runs in `mooring_plugin_host`'s sandbox: no files, network or processes,
//! each call time- and memory-limited. It is handed the sessions the column would list
//! (title, state, times, where each stopped; never a transcript), plus the latest closed
//! ones, and answers with sections of session, folder and note rows. Mooring draws the answer in its own style; a session the
//! module names that the column does not list is ignored, and any failure falls back to
//! the classic list with the reason shown.
use super::sidebar_list::{End, Listed};
use super::*;
use mooring_core::sidebar::{Bucket, Preset};
use mooring_plugin_host as host;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::sync::OnceLock;

/// Adds one session row to the column, within the preset's row limit.
pub(super) type AddRow<'a> =
    dyn FnMut(&mut Vec<gpui::AnyElement>, Listed, Option<String>, End) + 'a;
/// How many closed sessions a module is handed, newest first.
pub(super) const CLOSED_SHOWN: usize = 50;
/// The longest trailing text a module may put at a row's end.
const TRAILING_CHARS: usize = 12;

#[derive(Serialize)]
struct State<'a> {
    sessions: Vec<StateSession<'a>>,
    /// Every closed session in the space; `sessions` carries only the newest of them.
    closed_total: usize,
    now: u64,
}
#[derive(Serialize)]
struct StateSession<'a> {
    id: &'a str,
    title: String,
    agent: &'a str,
    /// The id's last four letters or digits, upper-cased: what the board card shows.
    short: String,
    group: &'static str,
    cwd: &'a str,
    updated: u64,
    running_since: Option<u64>,
    /// What the session is at now, one line: the question, the error or the last reply.
    line: Option<String>,
    stopped: Option<String>,
    done: Option<String>,
    needs_you: bool,
    closed: bool,
    selected: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Reply {
    sections: Vec<Section>,
}
#[derive(Clone, Debug, Deserialize)]
struct Section {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    folded_default: bool,
    #[serde(default)]
    rows: Vec<Row>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind")]
enum Row {
    Session {
        id: String,
        #[serde(default)]
        detail: Option<String>,
        #[serde(default)]
        detail_mode: Option<String>,
        #[serde(default)]
        trailing: Trailing,
    },
    /// A folder header: it folds the session rows after it, up to the next folder or note.
    Folder {
        cwd: String,
        #[serde(default)]
        folded_default: bool,
    },
    Note {
        text: String,
        #[serde(default)]
        action: Option<String>,
    },
}

/// The closed sessions a module is handed: the newest few, and how many there are.
#[derive(Default)]
pub(super) struct Closed<'a> {
    pub rows: Vec<Listed<'a>>,
    pub total: usize,
}

/// What a session row ends with. Mooring draws it, so a running clock ticks without
/// asking the module again: `"state"` (the default), `"age"`, `"elapsed"`, `"none"` or
/// `{"text": "…"}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Trailing {
    #[default]
    State,
    Age,
    /// How long the turn has run; a session not running shows its age.
    Elapsed,
    None,
    #[serde(untagged)]
    Text {
        text: String,
    },
}

/// One module, compiled off the UI thread on first use, and its last answer: an unchanged
/// state is not asked again, so redraws cost a byte comparison.
pub(super) struct Loaded {
    module: Result<host::Module, String>,
    /// None between a failed call and the next: what it left in its memory is not reused.
    plugin: Option<host::Plugin>,
    last: Option<(Vec<u8>, Result<Reply, String>)>,
    /// Calls that failed in a row, and when it may be asked again after too many.
    failures: u32,
    paused_until: Option<std::time::Instant>,
}
/// What is shown while a module compiles: the classic list, with no message.
const COMPILING: &str = "正在加载";
/// Calls that fail in a row before the module is left alone for `PAUSE`: each costs the UI
/// thread up to its deadline, on every state change.
const FAILURES: u32 = 3;
const PAUSE: std::time::Duration = std::time::Duration::from_secs(30);
/// Sections and rows taken from a reply. The bytes are capped by the host; this caps what
/// turns into elements, on the UI thread, outside the call's deadline.
const MAX_SECTIONS: usize = 100;
const MAX_ROWS: usize = 2000;

/// `Default` makes a new set after a reload; a compile that was going for the old set finds
/// its id gone and drops what it made.
pub(super) struct Views {
    modules: RefCell<HashMap<String, Loaded>>,
    compiling: RefCell<std::collections::HashSet<String>>,
    id: u64,
}
impl Default for Views {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self {
            modules: Default::default(),
            compiling: Default::default(),
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// A reply the sidebar will draw, or why not.
fn accept(reply: Reply) -> Result<Reply, String> {
    let rows: usize = reply.sections.iter().map(|s| s.rows.len()).sum();
    if reply.sections.len() > MAX_SECTIONS || rows > MAX_ROWS {
        return Err(format!(
            "回复太多（{} 段 {rows} 行，最多 {MAX_SECTIONS} 段 {MAX_ROWS} 行）",
            reply.sections.len()
        ));
    }
    Ok(reply)
}

fn engine() -> Result<&'static host::Engine, String> {
    static ENGINE: OnceLock<Result<host::Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let engine = host::engine().map_err(|e| e.to_string())?;
            host::start_ticker(&engine);
            Ok(engine)
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Reads and compiles it. Slow: never on the UI thread.
fn compile(preset: &Preset) -> Result<host::Module, String> {
    let path = preset.wasm_path().ok_or("没有 wasm 文件")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("读不了 {}：{e}", path.display()))?;
    host::module(engine()?, &bytes).map_err(|e| format!("不是可用的 wasm：{e}"))
}
fn instantiate(module: &host::Module) -> Result<host::Plugin, String> {
    host::Plugin::new(engine()?, module).map_err(|e| format!("加载失败：{e}"))
}

/// The state string a session's group goes by, the same coarse words as the spec.
fn group_name(g: Status) -> &'static str {
    // The plugin protocol keeps its coarse words: the same buckets as the native list.
    match super::sidebar_list::bucket(g) {
        // Finished turns joined 要你处理 on 11-03; plugins still read them as `review`.
        Bucket::Attention => match g {
            Status::Waiting | Status::Error => g.key(),
            _ => "review",
        },
        Bucket::Running => "running",
        Bucket::Done => "review",
        Bucket::Stopped => "paused",
        Bucket::Fresh => "fresh",
        Bucket::Stale => "stale",
    }
}

/// The id's tail: ids share prefixes (dates, `visual-`), their ends differ.
pub(super) fn short_id(cli_session_id: &str) -> String {
    let alnum: Vec<char> = cli_session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    alnum[alnum.len().saturating_sub(4)..]
        .iter()
        .collect::<String>()
        .to_uppercase()
}

impl Workbench {
    fn wasm_state(&self, open: &[Listed], closed: &Closed, current: Option<&str>) -> Vec<u8> {
        let first = |t: &str| {
            t.lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.chars().take(T::TAB_TIP_REASON_CHARS).collect::<String>())
        };
        let state = State {
            sessions: open
                .iter()
                .map(|l| (l, false))
                .chain(closed.rows.iter().map(|l| (l, true)))
                .map(|(listed, closed)| {
                    let (s, g, _) = listed;
                    let content = self.workspace.content.get(&s.id);
                    // The sidebar's own answer, so 要你处理 means the same in both.
                    let b = self.row_bucket(listed);
                    StateSession {
                        id: &s.id,
                        title: self.session_title(s),
                        agent: s.runtime.name(),
                        short: short_id(&s.cli_session_id),
                        group: group_name(*g),
                        cwd: &s.cwd,
                        updated: self.session_updated(s),
                        running_since: content
                            .filter(|_| b == Bucket::Running)
                            .map(|c| c.turn_started)
                            .filter(|t| *t > 0),
                        line: content
                            .map(|c| mooring_core::content::short(&c.headline(), 100))
                            .filter(|l| !l.trim().is_empty()),
                        stopped: match b {
                            Bucket::Attention => self.attention_reason(s, *g),
                            Bucket::Stopped => content.and_then(|c| first(&c.last_reply)),
                            _ => None,
                        },
                        // Finished for you to check: 要你处理 now, but still the last word.
                        done: content
                            .filter(|_| *g == Status::Review)
                            .and_then(|c| first(&c.last_reply)),
                        needs_you: b == Bucket::Attention,
                        closed,
                        selected: current == Some(s.id.as_str()),
                    }
                })
                .collect(),
            closed_total: closed.total,
            // Whole minutes, so a clock tick alone never asks the module again.
            now: now() / 60 * 60,
        };
        serde_json::to_vec(&state).unwrap_or_default()
    }
    /// The module's sections for this state, from its last answer when nothing changed.
    /// The first ask starts its compile and shows the classic list until that lands.
    fn wasm_reply(
        &self,
        preset: &Preset,
        open: &[Listed],
        closed: &Closed,
        current: Option<&str>,
        cx: &Context<Self>,
    ) -> Result<Reply, String> {
        let state = self.wasm_state(open, closed, current);
        let mut views = self.sidebar.wasm.modules.borrow_mut();
        let Some(view) = views.get_mut(&preset.id) else {
            self.start_compile(preset, cx);
            return Err(COMPILING.to_owned());
        };
        if let Some((input, reply)) = &view.last
            && *input == state
        {
            return reply.clone();
        }
        let module = view.module.as_ref().map_err(|e| e.clone())?;
        if let Some(until) = view.paused_until {
            if std::time::Instant::now() < until {
                return view
                    .last
                    .as_ref()
                    .map_or(Err("连续失败，稍后再试".to_owned()), |(_, r)| {
                        r.clone()
                    });
            }
            view.paused_until = None;
        }
        let plugin = match view.plugin.take() {
            Some(p) => p,
            None => instantiate(module)?,
        };
        let mut plugin = plugin;
        let reply = plugin
            .render(&state)
            .map_err(|e| format!("调用出错：{e}"))
            .and_then(|bytes| {
                serde_json::from_slice::<Reply>(&bytes).map_err(|e| format!("回复格式不对：{e}"))
            })
            .and_then(accept);
        if reply.is_ok() {
            view.failures = 0;
            view.plugin = Some(plugin);
        } else {
            // Its instance is dropped: the next call starts clean.
            view.failures += 1;
            if view.failures >= FAILURES {
                view.failures = 0;
                view.paused_until = Some(std::time::Instant::now() + PAUSE);
            }
        }
        view.last = Some((state, reply.clone()));
        reply
    }
    /// Compiles a preset's module on a background thread, once, and redraws when it lands.
    fn start_compile(&self, preset: &Preset, cx: &Context<Self>) {
        let views = &self.sidebar.wasm;
        if !views.compiling.borrow_mut().insert(preset.id.clone()) {
            return;
        }
        let (preset, id) = (preset.clone(), views.id);
        cx.spawn(async move |this, cx| {
            let key = preset.id.clone();
            let module = cx.background_spawn(async move { compile(&preset) }).await;
            let _ = this.update(cx, |this, cx| {
                let views = &this.sidebar.wasm;
                if views.id != id {
                    return;
                }
                views.compiling.borrow_mut().remove(&key);
                views.modules.borrow_mut().insert(
                    key,
                    Loaded {
                        module,
                        plugin: None,
                        last: None,
                        failures: 0,
                        paused_until: None,
                    },
                );
                cx.notify();
            });
        })
        .detach();
    }
    /// The rows for a `wasm` preset, or the classic list under the reason it failed.
    /// Returns whether the module's answer is shown: it then owns the closed sessions too.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn wasm_rows(
        &self,
        preset: &Preset,
        sessions: Vec<Listed>,
        closed: &Closed,
        current: Option<&str>,
        push: &mut AddRow,
        out: &mut Vec<gpui::AnyElement>,
        cx: &Context<Self>,
    ) -> bool {
        let reply = match self.wasm_reply(preset, &sessions, closed, current, cx) {
            Ok(r) => r,
            Err(why) if why == COMPILING => {
                for l in sessions {
                    push(out, l, None, End::State);
                }
                return false;
            }
            Err(why) => {
                out.push(
                    label(format!(
                        "左栏「{}」没能排出来，先按经典显示：{why}",
                        preset.name
                    ))
                    .px(px(T::S8))
                    .mt(px(T::S12))
                    .text_size(px(T::CHIP_SIZE))
                    .text_color(Tone::Danger.ink())
                    .into_any_element(),
                );
                for l in sessions {
                    push(out, l, None, End::State);
                }
                return false;
            }
        };
        let find = |id: &str| {
            sessions
                .iter()
                .chain(closed.rows.iter())
                .find(|l| l.0.id == id)
                .copied()
        };
        for section in reply.sections {
            let key = format!("wasm:{}:{}", preset.id, section.id);
            let open = self.sidebar.open(&key, !section.folded_default);
            let named = section
                .rows
                .iter()
                .filter(|r| matches!(r, Row::Session { id, .. } if find(id).is_some()))
                .count();
            let notes = section.rows.iter().any(|r| matches!(r, Row::Note { .. }));
            if named == 0 && !notes {
                continue;
            }
            if let Some(title) = section.title.clone() {
                out.push(
                    self.heading(title, section.count.unwrap_or(named), cx)
                        .into_any_element(),
                );
            }
            // Whether the folder the next session rows sit under is open.
            let mut in_open_folder = true;
            for (i, row) in section.rows.iter().enumerate() {
                match row {
                    Row::Note { text, action } => {
                        in_open_folder = true;
                        out.push(self.wasm_note(
                            format!("{key}:{i}"),
                            &key,
                            text.clone(),
                            action.as_deref(),
                            open,
                            cx,
                        ))
                    }
                    Row::Folder {
                        cwd,
                        folded_default,
                    } => {
                        let under: Vec<Listed> = section.rows[i + 1..]
                            .iter()
                            .take_while(|r| matches!(r, Row::Session { .. }))
                            .filter_map(|r| match r {
                                Row::Session { id, .. } => find(id),
                                _ => None,
                            })
                            .collect();
                        let folder = format!("{key}:dir:{cwd}");
                        in_open_folder = self.sidebar.open(&folder, !folded_default);
                        if !open || under.is_empty() {
                            continue;
                        }
                        let needs = under
                            .iter()
                            .filter(|l| self.row_bucket(l) == Bucket::Attention)
                            .count();
                        let running = under.iter().any(|l| self.row_bucket(l) == Bucket::Running);
                        out.push(self.folder_row(
                            cwd,
                            &folder,
                            under.len(),
                            needs,
                            in_open_folder,
                            running,
                            cx,
                        ));
                    }
                    Row::Session {
                        id,
                        detail,
                        detail_mode,
                        trailing,
                    } => {
                        let Some(l) = find(id) else {
                            continue;
                        };
                        // The session you are in stays in sight while its section is folded.
                        let selected = current == Some(id.as_str());
                        if !(open && in_open_folder) && !selected {
                            continue;
                        }
                        let shown = match detail_mode.as_deref() {
                            Some("always") => detail.clone(),
                            Some("selected") if selected => detail.clone(),
                            _ => None,
                        };
                        push(out, l, shown, self.wasm_end(l, trailing, cx));
                    }
                }
            }
        }
        true
    }
    /// When the unfinished turn began, including one only its sub-agents keep going.
    pub(super) fn turn_started(&self, s: &AgentSession) -> Option<u64> {
        self.workspace
            .content
            .get(&s.id)
            .and_then(mooring_core::content::Content::running_since)
    }
    fn wasm_end(&self, (s, _, _): Listed, trailing: &Trailing, cx: &Context<Self>) -> End {
        let ago = || crate::model::time::ago(self.session_updated(s), crate::model::time::now());
        match trailing {
            Trailing::State => End::State,
            Trailing::None => End::Nothing,
            Trailing::Age => End::Text(ago()),
            Trailing::Elapsed => match self.turn_started(s) {
                Some(at) => {
                    self.tick_turn(cx);
                    End::Text(super::layout::turn_clock(now().saturating_sub(at)))
                }
                None => End::Text(ago()),
            },
            Trailing::Text { text } => End::Text(text.chars().take(TRAILING_CHARS).collect()),
        }
    }
    /// A note row: plain text, or a way to fold its section, open ⌘P, 鸟瞰 or the closed
    /// sessions there.
    fn wasm_note(
        &self,
        id: String,
        section: &str,
        text: String,
        action: Option<&str>,
        open: bool,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let row = div()
            .id(gpui::SharedString::from(format!("sidebar-note-{id}")))
            .min_h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S6))
            .rounded(px(T::CONTROL_RADIUS))
            .text_size(px(T::CHIP_SIZE))
            .text_color(T::color(T::muted()))
            .child(div().flex_1().min_w_0().truncate().child(text));
        let section = section.to_owned();
        match action {
            Some("toggle") => {
                row.cursor_pointer()
                    .hover(|s| s.bg(T::color(T::raised())))
                    .child(if open { "⌄" } else { "›" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_sidebar_group(&section, open, cx)
                    }))
                    .into_any_element()
            }
            Some("palette") => row
                .cursor_pointer()
                .hover(|s| s.bg(T::color(T::raised())))
                .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx)))
                .into_any_element(),
            Some("overview") => row
                .cursor_pointer()
                .hover(|s| s.bg(T::color(T::raised())))
                .on_click(cx.listener(|this, _, _, cx| this.show_overview(cx)))
                .into_any_element(),
            Some("closed") => row
                .cursor_pointer()
                .hover(|s| s.bg(T::color(T::raised())))
                .on_click(cx.listener(|this, _, _, cx| this.show_closed(cx)))
                .into_any_element(),
            _ => row.into_any_element(),
        }
    }
}

#[cfg(test)]
#[path = "sidebar_wasm_tests.rs"]
mod tests;
