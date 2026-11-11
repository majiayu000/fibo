use super::*;

/// The left column on this machine: the chosen preset, whether it is folded away, and
/// the groups you opened or closed, and the pinned terminals. The preset, the fold and
/// the pins are kept in `<config>/sidebar`; opened groups last until quit.
#[derive(Default)]
pub(in crate::workbench) struct SidebarPrefs {
    pub preset: String,
    pub collapsed: bool,
    /// Open (true) or closed, as you last left each group. A group you never touched
    /// follows its default, which may change with what is in it; one you touched stays
    /// as you left it (storing only "flipped" turned it over when the default moved).
    pub toggled: HashMap<String, bool>,
    /// WASM presets loaded so far, with their last answers.
    pub wasm: super::super::sidebar_wasm::Views,
    /// Terminals whose session sits in 置顶, in the order pinned; dropped when it ends.
    pub pinned: Vec<String>,
    /// The view menu opened from the ≑ beside 会话, where it was opened. Not saved.
    pub view_menu: Option<gpui::Point<gpui::Pixels>>,
    /// 会话优先 narrowed to one space's sessions by clicking its chip; None lists every
    /// space. Not saved: a new launch starts on everything.
    pub space_filter: Option<String>,
    /// The machine (its host in `machines.json`) the column shows alone. Not saved.
    pub machine_scope: Option<String>,
    /// That machine's space shown alone; its host owns the ids, so it is not `space_filter`.
    pub machine_space: Option<String>,
    /// 先不管这台's menu: which machine and where it was opened. Not saved.
    pub machine_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    pub(in crate::workbench) motion: super::super::sidebar_motion::SidebarMotion,
    pub(in crate::workbench) scroll: ListScroll,
}
impl SidebarPrefs {
    fn path() -> Option<PathBuf> {
        crate::ui::status_colors::config_dir().map(|d| d.join("sidebar"))
    }
    pub fn load() -> Self {
        let text = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let mut lines = text.lines().map(str::trim);
        let preset = lines.next().unwrap_or_default().to_owned();
        let rest: Vec<&str> = lines.collect();
        Self {
            preset,
            collapsed: rest.contains(&"collapsed"),
            toggled: HashMap::new(),
            wasm: Default::default(),
            pinned: rest
                .iter()
                .filter_map(|l| l.strip_prefix("pin "))
                .map(str::to_owned)
                .collect(),
            view_menu: None,
            space_filter: None,
            machine_scope: None,
            machine_space: None,
            machine_menu: None,
            motion: Default::default(),
            scroll: Default::default(),
        }
    }
    /// Best effort: a choice that cannot be written still holds until quit.
    pub fn save(&self) {
        if let Some(path) = Self::path() {
            let _ = path.parent().map(std::fs::create_dir_all);
            let mut body = format!("{}\n", self.preset);
            if self.collapsed {
                body.push_str("collapsed\n");
            }
            for terminal in &self.pinned {
                body.push_str(&format!("pin {terminal}\n"));
            }
            let _ = crate::ui::status_colors::write_atomic(&path, body.as_bytes());
        }
    }
    pub fn preset(&self) -> &'static Preset {
        presets::find(&self.preset)
    }
    /// Every preset but a plugin's flat list uses the denser look of sidebar-density.html:
    /// the spaces fold into one line, rows carry a state symbol and a time.
    pub fn dense(&self) -> bool {
        self.preset().group != Grouping::Flat
    }
}
