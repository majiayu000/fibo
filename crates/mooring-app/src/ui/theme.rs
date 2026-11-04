//! Design tokens from specs/terminal-workbench/design/README.md.
//! UI dimensions live here; terminal cell colors remain owned by the VT.
pub use super::palette::Palette;
pub use crate::model::session_state::Status;
use gpui::{App, Font, Hsla, Rgba, font};
pub use mooring_core::terminal::{Scheme, TermColors};
use std::sync::{
    Mutex,
    atomic::{AtomicPtr, Ordering},
};

static DARK: Palette = Palette::of_scheme(Scheme::Dark);
static CURRENT: AtomicPtr<Palette> = AtomicPtr::new(std::ptr::addr_of!(DARK).cast_mut());
static KEPT: Mutex<Vec<&'static Palette>> = Mutex::new(Vec::new());

pub fn hsla(c: u32) -> Hsla {
    Rgba {
        r: ((c >> 16) & 255) as f32 / 255.,
        g: ((c >> 8) & 255) as f32 / 255.,
        b: (c & 255) as f32 / 255.,
        a: 1.,
    }
    .into()
}
pub struct Theme;
/// Colors of the native menu bar panel; status hues show only on dots and quoted text.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct MenuPalette {
    pub background: u32,
    pub sunken: u32,
    pub hover: u32,
    pub text: u32,
    pub secondary: u32,
    pub muted: u32,
    pub border: u32,
    pub border_strong: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub wait: u32,
    pub wait_ink: u32,
    pub error: u32,
    pub error_ink: u32,
    pub error_wash: u32,
    pub review: u32,
    pub running: u32,
    pub stale: u32,
}

/// JetBrains Mono 2.304 (OFL, assets/JETBRAINS-MONO-LICENSE): the face Ghostty embeds
/// as its default, registered in-process so `Theme::mono` never depends on an install.
pub const BUNDLED_MONO: [&[u8]; 4] = [
    include_bytes!("../../assets/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../assets/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../assets/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../assets/JetBrainsMono-BoldItalic.ttf"),
];
/// A status's colors: one hue per state from `colors.json` or the default, its ink and
/// wash derived from it and the terminal. Status colors are for sessions only; anything
/// else that is fine, a warning or a failure takes a [`Tone`](super::tone::Tone).
pub trait StatusStyle: Copy {
    fn hue(self) -> u32;
    /// Dot, ink and wash.
    fn colors(self) -> (Hsla, Hsla, Hsla);
}
impl StatusStyle for Status {
    fn hue(self) -> u32 {
        let colors = super::status_colors::StatusColors::get();
        Theme::palette().status[colors.index_of(self)].unwrap_or_else(|| colors.hue(self))
    }
    fn colors(self) -> (Hsla, Hsla, Hsla) {
        let (dot, ink, bg) = status_colors_in(self, Theme::palette());
        (hsla(dot), hsla(ink), hsla(bg))
    }
}
/// Dot, ink and wash on one palette.
fn status_colors_in(status: Status, p: &Palette) -> (u32, u32, u32) {
    let hue = status.hue();
    match status {
        Status::Stale => (legible(hue, p), p.secondary, p.raised),
        _ => tone_of(hue, p),
    }
}
/// Ink moves the hue toward the text color, so it reads on either background.
pub(super) fn tone_of(hue: u32, p: &Palette) -> (u32, u32, u32) {
    (
        legible(hue, p),
        mix(hue, p.vivid, p.ink_share),
        mix(p.terminal_bg, hue, p.wash_share),
    )
}
/// The hue pushed toward the text color until it reaches 3:1 on the terminal background,
/// so a yellow or green dot still reads on white; a hue that already passes is untouched.
fn legible(hue: u32, p: &Palette) -> u32 {
    readable(hue, p.terminal_bg, p.vivid, 3.)
}
/// The hue pushed toward `vivid` until it reaches `ratio` on `bg`.
fn readable(hue: u32, bg: u32, vivid: u32, ratio: f32) -> u32 {
    (0..=1000)
        .step_by(50)
        .map(|share| mix(hue, vivid, share))
        .find(|&c| contrast(c, bg) >= ratio)
        .unwrap_or(vivid)
}
/// WCAG contrast ratio between two colors.
pub fn contrast(a: u32, b: u32) -> f32 {
    let luminance = |c: u32| {
        let channel = |shift: u32| {
            let v = ((c >> shift) & 255) as f32 / 255.;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}
/// `share` per mille of `to` blended into `from`, per channel; const so tokens stay constants.
pub const fn mix(from: u32, to: u32, share: u32) -> u32 {
    let mut out = 0;
    let mut shift = 0;
    while shift < 24 {
        let a = (from >> shift) & 255;
        let b = (to >> shift) & 255;
        out |= ((a * (1000 - share) + b * share + 500) / 1000) << shift;
        shift += 8;
    }
    out
}
macro_rules! tokens {
    ($($name:ident),* $(,)?) => {
        impl Theme {
            $(pub fn $name() -> u32 {
                Self::palette().$name
            })*
        }
    };
}
tokens!(
    terminal_bg,
    terminal_fg,
    app,
    panel,
    sidebar,
    pane,
    header,
    todo,
    raised,
    border,
    card_border,
    control,
    field_edge,
    button,
    button_hover,
    primary,
    body,
    secondary,
    muted,
    accent,
    on_accent,
    focus,
    link,
    selection,
    chip,
    popover,
    palette_row,
    matched,
    scrim,
    series_other,
    hatch,
    chart_grid,
    chart_ref,
);
/// The 会话优先 column exactly as docs/sidebar-redesign/flat.html draws it: sizes in px,
/// and on dark its own near-black colors; on light it falls back to the palette.
pub struct Flat;
impl Flat {
    pub const WIDTH: f32 = Theme::SIDEBAR;
    pub const BRAND_ROW: f32 = 26.;
    pub const BRAND_TEXT: f32 = 11.;
    /// Letter spacing for MOORING, drawn as gaps between its letters.
    pub const BRAND_TRACK: f32 = 2.;
    pub const ICON_BUTTON: f32 = 26.;
    pub const ICON: f32 = 14.;
    pub const HOST_DOT: f32 = 6.;
    pub const SEARCH: f32 = 30.;
    pub const SEARCH_ICON: f32 = 13.;
    pub const FIELD_RADIUS: f32 = 8.;
    pub const CHIP: f32 = 24.;
    pub const CHIP_DOT: f32 = 9.;
    pub const CHIP_TEXT: f32 = 11.5;
    pub const CHIP_GAP: f32 = 5.;
    pub const BADGE: f32 = 14.;
    pub const BADGE_TEXT: f32 = 9.5;
    pub const HEAD_TEXT: f32 = 11.;
    pub const HEAD_LINE: f32 = 14.;
    pub const TITLE: f32 = 12.5;
    pub const TITLE_LINE: f32 = 16.;
    pub const SUB: f32 = 11.5;
    pub const SUB_LINE: f32 = 15.;
    pub const TAG: f32 = 11.;
    pub const TAG_DOT: f32 = 7.;
    pub const TAG_MAX: f32 = 76.;
    pub const TIME: f32 = 11.;
    pub const TIME_WIDTH: f32 = 26.;
    pub const STATE: f32 = 8.;
    pub const ROW_RADIUS: f32 = 7.;
    pub const ROW_GAP: f32 = 9.;
    pub const ROW_PAD_Y: f32 = 6.;
    pub const ONE_PAD_Y: f32 = 5.;
    pub const EDGE: f32 = 18.;
    pub const KEY_TEXT: f32 = 10.5;
    pub const FOOT_TEXT: f32 = 12.;
    pub const LINE_SMALL: f32 = 14.;
    pub const LINE_TEXT: f32 = 16.;
    /// A session's subagents: a dot per subagent on its count line, and one branch row
    /// each, hung from a trunk under the row's state mark.
    pub const SUB_PIP: f32 = 6.;
    pub const SUB_PIP_GAP: f32 = 3.;
    /// Past eight subagents the dots give way to a bar of ended and running.
    pub const SUB_BAR: f32 = 56.;
    pub const SUB_BAR_HEIGHT: f32 = 4.;
    pub const SUB_BRANCH: f32 = 14.;
    pub const SUB_TRUNK: f32 = 3.;
    pub const SUB_LEAF: f32 = 20.;
    pub fn bg() -> u32 {
        Theme::palette().sidebar_bg
    }
    pub fn line() -> u32 {
        Theme::palette().sidebar_line
    }
    pub fn field_line() -> u32 {
        Theme::palette().sidebar_field_line
    }
    pub fn chip_line() -> u32 {
        Theme::palette().sidebar_chip_line
    }
    pub fn chip_on() -> u32 {
        Theme::palette().sidebar_chip_on
    }
    pub fn chip_on_line() -> u32 {
        Theme::palette().sidebar_chip_on_line
    }
    pub fn hover() -> u32 {
        Theme::palette().sidebar_hover
    }
    pub fn selected() -> u32 {
        Theme::palette().sidebar_selected
    }
    pub fn selected_line() -> u32 {
        Theme::palette().sidebar_selected_line
    }
    /// MOORING and a chosen chip's name.
    pub fn bright() -> u32 {
        Theme::palette().sidebar_bright
    }
    /// A title that needs you, is running or is being checked.
    pub fn title() -> u32 {
        Theme::palette().sidebar_title
    }
    /// A stopped session's title and a chip's name.
    pub fn quiet() -> u32 {
        Theme::palette().sidebar_quiet
    }
    pub fn second() -> u32 {
        Theme::palette().sidebar_second
    }
    pub fn third() -> u32 {
        Theme::palette().sidebar_third
    }
    /// The ✓ of a session you marked complete (flat.html `.st.d`).
    pub fn done() -> u32 {
        Theme::palette().sidebar_done
    }
    /// A stopped session's ring.
    pub fn idle() -> u32 {
        Theme::palette().sidebar_idle
    }
    /// The bar naming the one space the list is narrowed to.
    pub fn scope() -> u32 {
        Theme::palette().sidebar_scope
    }
    pub fn key_line() -> u32 {
        Theme::palette().sidebar_key_line
    }
}
#[allow(dead_code)] // The complete design palette also covers the deferred components.
impl Theme {
    pub const WHITE: u32 = 0xffffff;
    const BLACK: u32 = 0x000000;
    /// The colors the window paints with; read on every render, so one atomic load.
    pub fn palette() -> &'static Palette {
        // SAFETY: CURRENT only ever holds a static or a leaked (never freed) palette.
        unsafe { &*CURRENT.load(Ordering::Acquire) }
    }
    /// A space's gradient, by the space's number.
    pub fn space_tile(n: usize) -> [u32; 2] {
        let tiles = &Self::palette().space_tiles;
        tiles[n % tiles.len()]
    }
    /// What terminals paint with.
    pub fn term() -> TermColors {
        Self::palette().term
    }
    /// Over a terminal's selected cells.
    pub fn terminal_selection() -> Hsla {
        Self::palette()
            .term_selection
            .map_or(Self::color(Self::focus()).opacity(0.25), |c| {
                Self::color(c).opacity(0.45)
            })
    }
    /// The nearest built-in scheme.
    pub fn scheme() -> Scheme {
        Self::term().base()
    }
    /// Make these the window's colors, reporting whether anything changed. Palettes are
    /// kept for the life of the process, one per distinct set of colors: a reference to
    /// one may outlive the switch, and there are only as many as colors were tried.
    pub fn install(palette: Palette) -> bool {
        if *Self::palette() == palette {
            return false;
        }
        let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
        let found = kept.iter().copied().find(|p| **p == palette);
        let current = found.unwrap_or_else(|| {
            let leaked: &'static Palette = Box::leak(Box::new(palette));
            kept.push(leaked);
            leaked
        });
        CURRENT.store(std::ptr::from_ref(current).cast_mut(), Ordering::Release);
        true
    }
    pub fn set_scheme(scheme: Scheme) {
        Self::install(Palette::of_scheme(scheme));
    }
    /// Search matches drawn over terminal text, in either scheme: the hue of `matched`.
    pub const SEARCH_HIT: u32 = 0xe8a33a;
    pub const UI_FONT: &'static str = ".AppleSystemUIFont";
    pub const BODY_SIZE: f32 = 13.;
    pub const SMALL: f32 = 12.;
    pub const META: f32 = 11.5;
    pub const CHIP_SIZE: f32 = 11.;
    pub const MONO_SIZE: f32 = 12.5;
    /// Ghostty's default font-size, so terminal text matches it cell for cell.
    pub const TERMINAL_FONT_SIZE: f32 = 13.;
    pub const CARD_TITLE: f32 = 13.5;
    pub const TITLE: f32 = 20.;
    pub const LINE_HEIGHT: f32 = 1.4;
    pub const S2: f32 = 2.;
    pub const S4: f32 = 4.;
    pub const S6: f32 = 6.;
    pub const S8: f32 = 8.;
    pub const S10: f32 = 10.;
    pub const S12: f32 = 12.;
    pub const S14: f32 = 14.;
    pub const S16: f32 = 16.;
    pub const S20: f32 = 20.;
    /// Wide enough for a session title beside its space tag, time and state.
    pub const SIDEBAR: f32 = 316.;
    pub const TOP: f32 = 46.;
    /// macOS draws the traffic lights inside the top bar; its content starts after them.
    pub const TRAFFIC_LIGHT_X: f32 = 16.;
    pub const TRAFFIC_LIGHT_Y: f32 = 17.;
    pub const TRAFFIC_LIGHT_INSET: f32 = 84.;
    pub const TABS: f32 = 34.;
    pub const TAB_RENAME_MIN: f32 = 64.;
    /// A name being edited gets more room than a tab shows, then shrinks back.
    pub const TAB_RENAME_MAX: f32 = 360.;
    /// A short name still leaves its × at the far edge, clear of a click on the name.
    pub const TAB_MIN_WIDTH: f32 = 96.;
    pub const TAB_MAX_WIDTH: f32 = 200.;
    pub const TAB_TIP_WIDTH: f32 = 420.;
    pub const TAB_TIP_REASON_CHARS: usize = 160;
    pub const TODO_HEIGHT: f32 = 36.;
    pub const PANE_HEADER: f32 = 32.;
    /// The bar under a pane's title while its turn waits for you to check it.
    pub const TURN_BAR: f32 = 38.;
    /// A tab's widest: short names, the rest elided.
    pub const TAB_SHORT: f32 = 160.;
    /// A space's letter tile in the sidebar.
    pub const SPACE_TILE: f32 = 18.;
    pub const FOOTER: f32 = 26.;
    /// The window's bottom bar, its popovers and the bar's quota gauges.
    pub const STATUSBAR: f32 = 28.;
    pub const STATUS_POP: f32 = 400.;
    pub const STATUS_GAUGE: f32 = 30.;
    pub const STATUS_POP_GAUGE: f32 = 56.;
    pub const ROW: f32 = 30.;
    pub const BORDER_WIDTH: f32 = 1.;
    /// The focused split's ring, drawn over its 1px border so the grid keeps its size.
    pub const FOCUS_RING: f32 = 2.;
    pub const CARET_WIDTH: f32 = 1.5;
    pub const PANE_RADIUS: f32 = 8.;
    /// The faint glow inside a pane that asks you something.
    pub const RING_GLOW: f32 = 28.;
    pub const CONTROL_RADIUS: f32 = 6.;
    pub const CHIP_RADIUS: f32 = 4.;
    pub const CARD_RADIUS: f32 = 10.;
    pub const POPOVER_RADIUS: f32 = 12.;
    pub const DOT: f32 = 7.;
    pub const LOGO: f32 = 18.;
    pub const ICON_WIDTH: f32 = 24.;
    pub const ICON_HEIGHT: f32 = 22.;
    /// The glyph inside an `icon_button`, e.g. the sidebar toggle.
    pub const ICON_GLYPH: f32 = 18.;
    /// The ≑ beside 会话 that opens the view menu.
    pub const VIEW_GLYPH: f32 = 14.;
    pub const CARD_MIN: f32 = 300.;
    pub const OVERVIEW_X: f32 = 28.;
    pub const SEARCH_HEIGHT: f32 = 260.;
    pub const MENU_WIDTH: f32 = 230.;
    pub const HERE_WIDTH: f32 = 340.;
    pub const HERE_HEIGHT: f32 = 600.;
    /// The card's 你要它 / 做成了 / 停在 labels, its evidence bar and the resume command
    /// its menu previews.
    pub const HERE_FACT: f32 = 48.;
    pub const EVIDENCE_BAR: f32 = 8.;
    pub const HERE_COMMAND_CHARS: usize = 24;
    pub const CLEANUP_WIDTH: f32 = 960.;
    /// The diagnostics panel and its charts.
    pub const DIAGNOSTIC_WIDTH: f32 = 980.;
    pub const DIAGNOSTIC_HEIGHT: f32 = 860.;
    pub const BIG_NUMBER: f32 = 26.;
    /// The top panels: tick gauge, its ticks, the corner marks and the reading size.
    pub const GAUGE_HEIGHT: f32 = 14.;
    pub const GAUGE_TICK: f32 = 2.;
    pub const GAUGE_PITCH: f32 = 3.;
    pub const CORNER: f32 = 7.;
    pub const PANEL_VALUE: f32 = 15.;
    pub const STACK_HEIGHT: f32 = 22.;
    pub const BAR: f32 = 10.;
    pub const METER: f32 = 4.;
    pub const STEP_WIDTH: f32 = 46.;
    pub const STEP_HEIGHT: f32 = 3.;
    pub const SWATCH: f32 = 10.;
    pub const POINT: f32 = 9.;
    pub const TICK_ROW: f32 = 16.;
    pub const BAR_LABEL: f32 = 168.;
    pub const BAR_VALUE: f32 = 54.;
    pub const AXIS_GUTTER: f32 = 48.;
    pub const SCATTER_HEIGHT: f32 = 220.;
    /// A board card's height before GPUI measures it; the virtual columns correct it.
    pub const CARD_ESTIMATE: f32 = 100.;
    pub const TIP_WIDTH: f32 = 240.;
    /// Room for a centered axis label such as `150%`.
    pub const TICK_LABEL: f32 = 48.;
    pub const CLEANUP_ACTION_HEIGHT: f32 = 64.;
    pub const MENUBAR_WIDTH: f32 = 360.;
    /// The panel grows with its content up to this; five three-line rows fit.
    pub const MENUBAR_HEIGHT: f32 = 600.;
    pub const MENU_TITLE: f32 = 15.;
    pub const MENU_COUNT: f32 = 18.;
    pub const MENU_BUTTON: f32 = 34.;
    /// The menu bar panel is a separate macOS popover that follows the system's light or
    /// dark mode, apart from the window: README「跨界面主题」, menubar-panel.html.
    const MENU_LIGHT: MenuPalette = MenuPalette {
        background: 0xffffff,
        sunken: 0xf7f8fa,
        hover: 0xf4f5f8,
        text: 0x17181c,
        secondary: 0x5d6270,
        muted: 0x767b87,
        border: 0xe3e5ea,
        border_strong: 0xd5d8de,
        accent: 0x4b4fd6,
        on_accent: 0xffffff,
        wait: 0xc2570c,
        wait_ink: 0x9a3f06,
        error: 0xc0392b,
        error_ink: 0x9b2318,
        error_wash: 0xfbeae8,
        review: 0x5e62d8,
        running: 0x1f8a5b,
        stale: 0x8b909c,
    };
    const MENU_DARK: MenuPalette = MenuPalette {
        background: 0x1c1c1e,
        sunken: 0x18181a,
        hover: 0x242426,
        text: 0xecebe9,
        secondary: 0xa3a2a0,
        muted: 0x8a8988,
        border: 0x2e2e2f,
        border_strong: 0x3a3a3b,
        accent: 0xecebe9,
        on_accent: 0x1c1c1e,
        wait: 0xe8823a,
        wait_ink: 0xf3b386,
        error: 0xe5534b,
        error_ink: 0xf2a59f,
        error_wash: 0x3a2222,
        review: 0x8f93ff,
        running: 0x4cc38a,
        stale: 0x8b8a88,
    };
    /// The menu bar panel's colors: its own surfaces, with status hues from the same
    /// [`StatusColors`](super::status_colors::StatusColors) as the window, made readable
    /// on the panel. Nothing here picks a status color of its own.
    pub fn menu_light() -> MenuPalette {
        Self::menu(Self::MENU_LIGHT, Self::BLACK)
    }
    pub fn menu_dark() -> MenuPalette {
        Self::menu(Self::MENU_DARK, Self::WHITE)
    }
    fn menu(base: MenuPalette, vivid: u32) -> MenuPalette {
        let bg = base.background;
        let dot = |s: Status| readable(s.hue(), bg, vivid, 3.);
        let ink = |s: Status| readable(s.hue(), bg, vivid, 4.5);
        MenuPalette {
            wait: dot(Status::Waiting),
            wait_ink: ink(Status::Waiting),
            error: dot(Status::Error),
            error_ink: ink(Status::Error),
            error_wash: mix(bg, Status::Error.hue(), 120),
            review: dot(Status::Review),
            running: dot(Status::Running),
            stale: dot(Status::Stale),
            ..base
        }
    }
    pub const FORM_WIDTH: f32 = 520.;
    pub const DETAIL_WIDTH: f32 = 640.;
    /// The todo form with its 「保存并开始之后」 column, and that column.
    pub const TODO_FORM_WIDTH: f32 = 1080.;
    pub const TODO_PREVIEW_WIDTH: f32 = 400.;
    /// The todo detail page and its right column (关联会话).
    pub const TODO_DETAIL_WIDTH: f32 = 1120.;
    pub const TODO_DETAIL_SIDE: f32 = 340.;
    /// One acceptance criterion line in the form, its drag grip and number columns.
    pub const CRITERION_ROW: f32 = 38.;
    pub const CRITERION_GRIP: f32 = 14.;
    pub const CRITERION_NUMBER: f32 = 22.;
    /// The checkbox before a criterion in the detail, and an evidence row's kind column.
    pub const CRITERION_BOX: f32 = 16.;
    pub const EVIDENCE_KIND: f32 = 38.;
    /// The numbered dots of the form's 「保存并开始之后」 steps.
    pub const STEP_DOT: f32 = 18.;
    /// The detail's big 「n / m 有证据」 number and its per-criterion cells.
    pub const PROGRESS_NUMBER: f32 = 28.;
    pub const PROGRESS_CELL: f32 = 10.;
    pub const PROGRESS_CELL_RADIUS: f32 = 3.;
    pub const DISABLED_OPACITY: f32 = 0.45;
    pub const SETTINGS_WIDTH: f32 = 940.;
    pub const SETTINGS_NAV: f32 = 164.;
    /// A plugin setting being typed in its row.
    pub const OPTION_FIELD: f32 = 220.;
    pub const FORM_TITLE: f32 = 16.;
    pub const TASK_ROW: f32 = 48.;
    pub const SEARCH_WIDTH: f32 = 360.;
    /// The ⌘P palette: a floating panel over the workbench, list left, preview right.
    pub const PALETTE_WIDTH: f32 = 760.;
    pub const PALETTE_TOP: f32 = 72.;
    /// The list and preview keep one height, so moving the selection never resizes it.
    pub const PALETTE_LIST: f32 = 420.;
    pub const PALETTE_PREVIEW: f32 = 268.;
    pub const PALETTE_RADIUS: f32 = 14.;
    /// A result row, one or two lines; the type icon and the right-aligned time column.
    pub const PALETTE_ROW: f32 = 32.;
    pub const PALETTE_ROW_TWO: f32 = 44.;
    pub const PALETTE_MORE: f32 = 28.;
    pub const PALETTE_ICON: f32 = 18.;
    pub const PALETTE_WHEN: f32 = 62.;
    /// The preview's 你要它 / 停在 labels and the no-result page.
    pub const PALETTE_FACT: f32 = 44.;
    pub const PALETTE_EMPTY: f32 = 260.;
    /// The status counts above an empty query and the 「只搜」 suggestions.
    pub const FILTER_PILL: f32 = 24.;
    pub const FILTER_PILL_RADIUS: f32 = 12.;
    /// When the user spoke over a session: the track and its ticks.
    pub const ASK_TRACK: f32 = 10.;
    pub const ASK_TICK: f32 = 8.;
    pub const SEARCH_FIELD: f32 = 48.;
    pub const SEARCH_TEXT: f32 = 15.;
    pub const SEARCH_MIN: f32 = 180.;
    pub const HERE_LABEL: f32 = 96.;
    pub const FIELD_HEIGHT: f32 = 40.;
    pub const WINDOW_WIDTH: f32 = 1440.;
    pub const WINDOW_HEIGHT: f32 = 900.;
    // workbench refresh: overview/sidebar/cleanup
    /// The overview's status distribution bar and its legend pills.
    pub const DIST_BAR: f32 = 12.;
    pub const PILL_HEIGHT: f32 = 28.;
    pub const PILL_RADIUS: f32 = 14.;
    /// The scope switch: a raised track holding pressed and unpressed segments.
    pub const SEGMENT_HEIGHT: f32 = 26.;
    pub const SEGMENT_RADIUS: f32 = 5.;
    pub const SEGMENT_TRACK_RADIUS: f32 = 7.;
    /// Session cards keep their footer on one baseline across a row.
    pub const CARD_MIN_HEIGHT: f32 = 148.;
    pub const TODO_CARD_MIN: f32 = 280.;
    /// A folded group: one bordered row instead of its cards.
    pub const FOLDED_ROW: f32 = 40.;
    /// 很久没动's mini chart of last-activity ages.
    pub const AGE_BAR_WIDTH: f32 = 6.;
    pub const AGE_CHART_HEIGHT: f32 = 16.;
    /// The sidebar's per-space distribution under the selected space.
    pub const MINI_BAR: f32 = 4.;
    /// 逐个清理: one step per queued session, the tally swatches and the age marker.
    pub const CLEANUP_PAGE_WIDTH: f32 = 760.;
    pub const CLEANUP_STEP: f32 = 6.;
    /// Past this many steps the gaps would outgrow the row; segments then touch.
    pub const CLEANUP_STEP_GAPS: usize = 150;
    pub const AGE_MARKER: f32 = 10.;
    pub const CLEANUP_LABEL: f32 = 72.;
    // workbench refresh: main view
    /// The top bar's status counts and the 下一个等你的 chip.
    pub const PULSE_BUTTON: f32 = 24.;
    pub const PULSE_GAP: f32 = 5.;
    pub const NEXT_CHIP: f32 = 28.;
    /// A zero count, a plain shell's dot and a key hint stay in place but recede.
    pub const ZERO_OPACITY: f32 = 0.55;
    pub const FAINT_DOT_OPACITY: f32 = 0.4;
    /// The ≑ beside 会话 at rest: faint but always there, so the views can be found.
    pub const VIEW_IDLE_OPACITY: f32 = 0.45;
    pub const KBD_OPACITY: f32 = 0.7;
    pub const KBD_SIZE: f32 = 10.5;
    pub const SPLIT_CHIP_SIZE: f32 = 10.5;
    /// 验收 progress cells in the todo bar.
    pub const EVIDENCE_CELL_WIDTH: f32 = 10.;
    pub const EVIDENCE_CELL_HEIGHT: f32 = 6.;
    pub const EVIDENCE_CELL_RADIUS: f32 = 2.;
    /// The pane header's washed status chip and its text actions.
    pub const STATE_CHIP: f32 = 20.;
    pub const STATE_CHIP_RADIUS: f32 = 5.;
    pub const PANE_ACTION: f32 = 24.;
    pub const PANE_ACTION_RADIUS: f32 = 5.;
    /// The pane footer's text: cwd, branch and who owns input.
    pub const PANE_FOOTER_SIZE: f32 = 11.;
    // board (specs/overview-kanban/kanban.html)
    /// Each space's avatar: three washes over a base, picked by the space id. No pure red
    /// or orange as a base; those belong to 出错 and 等你回答.
    pub const BOARD_MESHES: [[u32; 4]; 6] = [
        [0xff7ad9, 0x7cf2c9, 0x7a8cff, 0xb58cff],
        [0x6ee7ff, 0xa78bfa, 0xf9a8d4, 0x38bdf8],
        [0xbef264, 0x2dd4bf, 0x60a5fa, 0x34d399],
        [0xf0abfc, 0xfde68a, 0x818cf8, 0xc084fc],
        [0xfcd34d, 0xf472b6, 0xa3e635, 0x93c5fd],
        [0x93c5fd, 0xfda4af, 0x86efac, 0xa5b4fc],
    ];
    /// A branch label's hue by its prefix, like an issue label.
    pub const BRANCH_FEAT: u32 = 0x9d8cff;
    pub const BRANCH_FIX: u32 = 0xf472b6;
    pub const BRANCH_PERF: u32 = 0xfacc15;
    pub const BRANCH_DOCS: u32 = 0x2dd4bf;
    pub const BRANCH_OPS: u32 = 0x60a5fa;
    pub const BRANCH_REFACTOR: u32 = 0xa3e635;
    pub const BOARD_COLUMN_MIN: f32 = 200.;
    pub const BOARD_CARD_SPACING: f32 = 8.;
    pub const BOARD_CARD_GAP: f32 = 7.;
    pub const BOARD_AVATAR: f32 = 20.;
    pub const BOARD_MARK: f32 = 12.;
    pub const BOARD_CLOCK: f32 = 11.;
    pub const BOARD_HEAD_ICON: f32 = 14.;
    pub const BOARD_PILL: f32 = 20.;
    /// The slot a column opens for a card about to land.
    pub const BOARD_SLOT: f32 = 64.;
    /// A column that will not take the card being dragged fades this far back.
    pub const BOARD_REFUSED_OPACITY: f32 = 0.38;
    /// How much the drawer dims the board behind it.
    pub const BOARD_SCRIM: f32 = 0.18;
    pub const BOARD_PILL_GAP: f32 = 5.;
    pub const BOARD_PILL_DOT: f32 = 6.;
    pub const BOARD_PILL_MAX: f32 = 110.;
    /// 等了多久: four rising bars in a framed square.
    pub const BOARD_BAR_WIDTH: f32 = 2.;
    pub const BOARD_BAR_BASE: f32 = 3.;
    pub const BOARD_BAR_STEP: f32 = 2.;
    pub const BOARD_BAR_GAP: f32 = 1.;
    pub const BOARD_BAR_INSET: f32 = 5.;
    pub const BOARD_BAR_FRAME_RADIUS: f32 = 5.;
    pub const BOARD_BAR_RADIUS: f32 = 0.5;
    /// Numbers that change in place keep their width.
    pub fn tabular() -> gpui::FontFeatures {
        gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))
    }
    // end workbench refresh: main view
    /// The chart color of the `i`th ranked app; past the palette, the shared "other".
    pub fn series(i: usize) -> u32 {
        Self::palette()
            .series
            .get(i)
            .copied()
            .unwrap_or(Self::palette().series_other)
    }
    /// The font preview's sample path, as a terminal's ANSI blue would draw it.
    pub const PREVIEW_ANSI_BLUE: u32 = 0x8f93ff;
    /// The opposite color, for a cursor drawn over a cell with no color of its own.
    pub fn inverse(rgb: u32) -> u32 {
        rgb ^ 0xffffff
    }
    pub fn color(value: u32) -> Hsla {
        hsla(value)
    }
    pub fn terminal_mono(cx: &App) -> Font {
        let mut face = Self::mono(cx);
        let family = super::terminal_text::current().family;
        // A family that is not installed (typed into the file, or uninstalled since)
        // would fall through to a proportional face whose letters miss the cells; the
        // monospace default stands in, and the setting stays as you wrote it.
        if !family.is_empty() && installed(&family, cx) {
            face.family = family.into();
        }
        face
    }
    pub fn mono(cx: &App) -> Font {
        static MONO: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let mut face = font(
            MONO.get_or_init(|| {
                let names = cx.text_system().all_font_names();
                ["JetBrains Mono", "SF Mono", "Menlo"]
                    .into_iter()
                    .find(|candidate| names.iter().any(|n| n == candidate))
                    .unwrap_or("Menlo")
                    .to_owned()
            })
            .clone(),
        );
        // Terminal cells are independently addressable; match Ghostty's reference
        // configuration rather than letting contextual ligatures cross cell edits.
        face.features = gpui::FontFeatures(std::sync::Arc::new(vec![
            ("liga".into(), 0),
            ("calt".into(), 0),
        ]));
        face.fallbacks = Some(gpui::FontFallbacks::from_fonts(vec![
            "Symbols Nerd Font Mono".into(),
        ]));
        face
    }
}

/// Whether a font family is installed, asked once per family name: listing every font
/// is slow and the terminal face is read on each frame.
fn installed(family: &str, cx: &App) -> bool {
    static SEEN: std::sync::Mutex<Option<(String, bool)>> = std::sync::Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((name, there)) = seen.as_ref()
        && name == family
    {
        return *there;
    }
    let there = cx
        .text_system()
        .all_font_names()
        .iter()
        .any(|n| n == family);
    *seen = Some((family.to_owned(), there));
    there
}

#[cfg(test)]
mod tests {
    use super::{Scheme, Theme, mix};

    #[test]
    fn mix_blends_each_channel() {
        assert_eq!(mix(0x000000, 0xffffff, 0), 0x000000);
        assert_eq!(mix(0x000000, 0xffffff, 1000), 0xffffff);
        assert_eq!(mix(0x102030, 0x304050, 500), 0x203040);
        assert_eq!(Theme::pane(), Theme::terminal_bg());
    }

    #[test]
    fn lifted_schemes_keep_the_dark_sidebar_ladder_on_their_own_colors() {
        use crate::ui::palette::{Palette, restate};
        for scheme in Scheme::ALL {
            assert_eq!(Palette::of_scheme(scheme).terminal_bg, scheme.bg());
            if scheme.is_light() || scheme == Scheme::Dark {
                continue;
            }
            let restate = |dark| restate(dark, scheme.bg(), scheme.fg());
            assert_eq!(restate(Scheme::Dark.bg()), scheme.bg());
            assert_eq!(restate(Scheme::Dark.fg()), scheme.fg());
            // The sidebar's grays keep their order from ground to bright text.
            let ladder = [
                0x0c0c0f, 0x121216, 0x1c1c21, 0x45454d, 0x7e7e87, 0xc9c9cf, 0xececee,
            ];
            for pair in ladder.windows(2) {
                assert!(
                    crate::ui::palette::gray(restate(pair[0]))
                        < crate::ui::palette::gray(restate(pair[1]))
                );
            }
            // Text stays readable on the ground: about 10:1, never below 7:1.
            let lum = |c: u32| {
                let ch = |v: u32| {
                    let v = v as f64 / 255.;
                    if v <= 0.04045 {
                        v / 12.92
                    } else {
                        ((v + 0.055) / 1.055).powf(2.4)
                    }
                };
                0.2126 * ch(c >> 16 & 255) + 0.7152 * ch(c >> 8 & 255) + 0.0722 * ch(c & 255)
            };
            let ratio = (lum(scheme.fg()) + 0.05) / (lum(scheme.bg()) + 0.05);
            assert!(
                (7. ..13.).contains(&ratio),
                "{scheme:?} contrast {ratio:.1}"
            );
        }
    }

    #[test]
    fn light_scheme_swaps_every_surface() {
        use super::{DARK, Palette};
        const LIGHT: Palette = Palette::of_scheme(Scheme::Light);
        assert_eq!(LIGHT.terminal_bg, Scheme::Light.bg());
        assert!(LIGHT.app < LIGHT.pane && DARK.app < DARK.pane);
        // Text stays far from its background in both schemes.
        for p in [DARK, LIGHT] {
            let gap = |a: u32, b: u32| ((a & 255) as i32 - (b & 255) as i32).abs();
            assert!(gap(p.body, p.pane) > 180);
            assert!(gap(p.secondary, p.pane) > 100);
            assert!(gap(p.on_accent, p.accent) > 180);
        }
    }

    /// Light meets the contrast floors: text 4.5:1, focus and field edges 3:1, status
    /// dots 3:1 even for the pale yellow and green hues; its surfaces step visibly.
    #[test]
    fn light_scheme_meets_contrast_floors() {
        use super::{Palette, Status, contrast, status_colors_in};
        let p = Palette::of_scheme(Scheme::Light);
        for (name, text) in [
            ("body", p.body),
            ("secondary", p.secondary),
            ("muted", p.muted),
        ] {
            assert!(contrast(text, p.pane) >= 4.5, "{name}");
        }
        assert!(contrast(p.focus, p.pane) >= 3.);
        assert!(contrast(p.field_edge, p.pane) >= 3.);
        assert!(
            contrast(p.focus, p.field_edge) >= 1.5,
            "focus must differ from the edge"
        );
        assert!(contrast(p.app, p.pane) >= 1.15);
        assert!(contrast(p.card_border, p.pane) >= 1.6);
        // The sidebar is the one gray step under the white panes (the v3 design).
        assert!(contrast(p.pane, p.sidebar) >= 1.05);
        assert!(
            contrast(p.raised, p.sidebar) >= 1.05,
            "selected row on the sidebar"
        );
        for group in Status::ALL {
            let (dot, ink, _) = status_colors_in(group, &p);
            assert!(contrast(dot, p.pane) >= 3., "{} dot", group.label());
            assert!(contrast(ink, p.pane) >= 4.5, "{} ink", group.label());
        }
    }

    /// Colors and sizes come from `Theme`; a literal in a component drifts from the tokens.
    /// The terminal renderer and the visual-test harness measure cells, not chrome.
    #[test]
    fn components_take_colors_and_sizes_from_tokens() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files: Vec<_> = [
            "terminal/pane.rs",
            "ui/field.rs",
            "ui/widgets.rs",
            "desktop.rs",
        ]
        .iter()
        .map(|name| src.join(name))
        .collect();
        // Every file under workbench/, its submodule folders included.
        let mut dirs = vec![src.join("workbench")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        let mut offenders = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                if color_literal(line) || size_literal(line) {
                    offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "use Theme tokens:\n{}",
            offenders.join("\n")
        );
    }

    fn color_literal(line: &str) -> bool {
        line.match_indices("0x").any(|(at, _)| {
            let hex = line[at + 2..]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            hex == 6
        })
    }

    fn size_literal(line: &str) -> bool {
        line.match_indices("px(").any(|(at, _)| {
            let rest = &line[at + 3..];
            rest.starts_with(|c: char| c.is_ascii_digit()) && !rest.starts_with("0.)")
        })
    }
}
