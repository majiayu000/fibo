//! How the left column lists sessions (`sidebar.json`). A preset only arranges what the
//! host already knows: it picks a grouping, which buckets fold into one summary row, when a
//! row shows its second line and how many rows come before 鸟瞰. Every preset reads the
//! same status judgement, so "needs you" means the same thing in all of them.
//!
//! The built-in presets (会话优先 / 机器栏 / 按机器 / 目录) are written in this format too; plugins
//! add presets beside them and never replace one.
use crate::agent::Loaded;
use crate::status::Status;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The `sidebar.json` format's major version.
pub const API: u32 = 1;

/// What a preset groups rows by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    /// 会话优先, the default: what needs you on top, then every other session newest
    /// first; gone quiet folds into one line. Each row's second line leads with the
    /// session's short id, so sessions with the same title can be told apart.
    Recent,
    /// One list, what needs you first.
    Flat,
    /// A section per bucket: 需要你, 在跑, 等你验收, 停下了, 新开的, 很久没动.
    Status,
    /// 需要你 pinned on top, then one folder per working directory.
    Directory,
    /// Sections computed by the plugin's sandboxed WebAssembly module (`wasm`).
    Wasm,
}

/// Where other machines' sessions go in the column. Only 会话优先's list can hold them;
/// every other arrangement keeps them under this machine's list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Machines {
    /// Under this machine's list, a heading per machine.
    #[default]
    Below,
    /// One list with every machine's rows; a chip shows one machine alone.
    Mixed,
    /// A rail picks the one machine the whole column shows.
    Rail,
    /// A folding section per machine.
    Sections,
}

/// Coarse states a preset can name. The host maps its finer states onto these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    /// Waiting on you or failed.
    Attention,
    Running,
    /// Finished, for you to check.
    Done,
    /// Stopped short or idle at the prompt.
    Stopped,
    /// Just opened or `/clear`ed: no turn has started yet.
    Fresh,
    /// Untouched past the stale days.
    Stale,
}
impl Bucket {
    pub const ALL: [Self; 6] = [
        Self::Attention,
        Self::Running,
        Self::Done,
        Self::Stopped,
        Self::Fresh,
        Self::Stale,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Attention => crate::status::needs_you_word(),
            Self::Running => Status::Running.label(),
            Self::Done => Status::Review.label(),
            Self::Stopped => Status::Paused.label(),
            Self::Fresh => Status::Fresh.label(),
            Self::Stale => Status::Stale.label(),
        }
    }
}

/// When a session row shows its second line (what it stopped at, what it is doing).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    /// Never; the hover card still has it.
    #[default]
    Never,
    /// For rows that need you, rows still running, and the selected one.
    Attention,
    Always,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub group: Grouping,
    /// Buckets folded into one summary row until clicked; `attention` cannot fold.
    #[serde(default)]
    pub fold: Vec<Bucket>,
    #[serde(default)]
    pub detail: Detail,
    /// Session rows before the rest hand off to 鸟瞰, where `row_limit` applies it.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// With `"group": "wasm"`: the module beside `sidebar.json` that arranges the list.
    #[serde(default)]
    pub wasm: Option<String>,
    /// Where other machines' sessions go.
    #[serde(default)]
    pub machines: Machines,
    #[serde(skip)]
    pub file: PathBuf,
}
fn default_limit() -> usize {
    12
}
impl Preset {
    /// The module's full path, for a `wasm` preset.
    pub fn wasm_path(&self) -> Option<PathBuf> {
        let name = self.wasm.as_ref()?;
        Some(self.file.parent()?.join(name))
    }
    /// How many session rows a list shows. 会话优先's layouts have no such number: the
    /// column scrolls, and a shared count let 要你处理 use up 在跑's rows (11-06).
    pub fn row_limit(&self) -> usize {
        if self.group == Grouping::Recent {
            usize::MAX
        } else {
            self.limit
        }
    }
    pub fn folds(&self, b: Bucket) -> bool {
        self.fold.contains(&b)
    }
    /// Lists every space's sessions, with chips to narrow to one. A flat list keeps one
    /// space (its column lists the spaces to pick from), and a module gets what it always got.
    pub fn all_spaces(&self) -> bool {
        matches!(
            self.group,
            Grouping::Recent | Grouping::Status | Grouping::Directory
        )
    }
    /// A layout that means nothing with no other machine, so it is not offered then.
    pub fn only_with_machines(&self) -> bool {
        matches!(self.machines, Machines::Rail | Machines::Sections)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    api: u32,
    presets: Vec<Preset>,
}

/// The built-in presets, in the order the switch shows them. Their names are the Chinese
/// keys; say them with `i18n::say` where shown (a plugin's own name passes through).
pub fn builtin() -> Vec<Preset> {
    let p = |id: &str, name: &str, group, fold: &[Bucket], detail| Preset {
        id: id.into(),
        name: name.into(),
        group,
        fold: fold.to_vec(),
        detail,
        limit: default_limit(),
        wasm: None,
        machines: Machines::Below,
        file: PathBuf::new(),
    };
    let recent = |id: &str, name: &str, machines| Preset {
        machines,
        ..p(id, name, Grouping::Recent, &[Bucket::Stale], Detail::Always)
    };
    vec![
        recent("recent", crate::tk!("会话优先"), Machines::Mixed),
        recent("machine-rail", crate::tk!("机器栏"), Machines::Rail),
        recent("machine-fold", crate::tk!("按机器"), Machines::Sections),
        // Every folder's sessions stay reachable here: no rows hand off to 鸟瞰.
        Preset {
            limit: 100,
            ..p(
                "directory",
                crate::tk!("目录"),
                Grouping::Directory,
                &[Bucket::Done, Bucket::Stopped, Bucket::Fresh, Bucket::Stale],
                Detail::Attention,
            )
        },
    ]
}

fn read(file: &Path, taken: &[Preset]) -> Result<Vec<Preset>, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("读不了：{e}"))?;
    let f: File = serde_json::from_slice(&bytes).map_err(|e| crate::plugin::bad_format(&e))?;
    if f.api != API {
        return Err(format!("左栏格式是 api {}，这个版本只认 api {API}", f.api));
    }
    if f.presets.is_empty() {
        return Err("presets 至少写一个".into());
    }
    let mut presets = f.presets;
    for i in 0..presets.len() {
        let (before, rest) = presets.split_at_mut(i);
        let p = &mut rest[0];
        if !crate::plugin::valid_id(&p.id) {
            return Err(format!(
                "左栏 id \"{}\" 只能用小写字母、数字和 -，不超过 40 个字",
                p.id
            ));
        }
        if taken.iter().chain(before.iter()).any(|t| t.id == p.id) {
            return Err(format!("左栏 id \"{}\" 和内置或前面的插件重名", p.id));
        }
        if p.name.trim().is_empty() || p.name.chars().count() > 6 {
            return Err(format!("左栏 {} 的 name 要写，最多 6 个字", p.id));
        }
        if p.fold.contains(&Bucket::Attention) {
            return Err(format!(
                "左栏 {} 不能折叠「{}」的会话",
                p.id,
                crate::status::needs_you_word()
            ));
        }
        if !(1..=100).contains(&p.limit) {
            return Err(format!("左栏 {} 的 limit 要在 1 到 100 之间", p.id));
        }
        if p.machines != Machines::Below && p.group != Grouping::Recent {
            return Err(format!(
                "左栏 {} 写了 machines，group 要写 \"recent\"",
                p.id
            ));
        }
        match (p.group, p.wasm.as_deref()) {
            (Grouping::Wasm, Some(name)) => {
                // Only a file inside the plugin's own directory.
                let plain = Path::new(name)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)));
                if !plain || !name.ends_with(".wasm") {
                    return Err(format!(
                        "左栏 {} 的 wasm 要写插件目录里的 .wasm 文件名",
                        p.id
                    ));
                }
                let module = file.parent().map(|d| d.join(name));
                if !module.is_some_and(|m| m.is_file()) {
                    return Err(format!("左栏 {} 的 {name} 不在插件目录里", p.id));
                }
            }
            (Grouping::Wasm, None) => {
                return Err(format!("左栏 {} 用 wasm 排法，要写 wasm 文件名", p.id));
            }
            (_, Some(_)) => {
                return Err(format!("左栏 {} 写了 wasm，group 要写 \"wasm\"", p.id));
            }
            _ => {}
        }
        p.file = file.to_owned();
    }
    Ok(presets)
}

/// The built-in presets, then each file's in order; a file with any bad preset loads none.
pub fn load(files: Vec<PathBuf>) -> (Vec<Preset>, Vec<Loaded>) {
    let mut presets = builtin();
    let mut report = Vec::new();
    for file in files {
        let result = read(&file, &presets);
        report.push(Loaded {
            file: file.clone(),
            result: result
                .as_ref()
                .map(|found| {
                    found
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join("、")
                })
                .map_err(Clone::clone),
        });
        if let Ok(found) = result {
            presets.extend(found);
        }
    }
    (presets, report)
}

static PRESETS: crate::plugin::Slot<(Vec<Preset>, Vec<Loaded>)> = crate::plugin::Slot::new();

/// Loads the installed plugins' presets once (after `plugin::install`).
pub fn install() {
    PRESETS.get_or_init(|| load(crate::plugin::files(crate::plugin::Kind::Sidebar)));
}
/// Loads them again after plugins changed (`plugin::reload`).
pub(crate) fn reload() {
    PRESETS.set(load(crate::plugin::files(crate::plugin::Kind::Sidebar)));
}
/// Every preset: the built-in three, then the plugins'.
pub fn all() -> &'static [Preset] {
    static BUILTIN: OnceLock<Vec<Preset>> = OnceLock::new();
    match PRESETS.get() {
        Some((p, _)) => p,
        None => BUILTIN.get_or_init(builtin),
    }
}
/// The preset with this id, or the default when it is gone: a removed plugin's choice, or
/// the 经典 and 状态 that were once built in.
pub fn find(id: &str) -> &'static Preset {
    let all = all();
    all.iter().find(|p| p.id == id).unwrap_or(&all[0])
}
pub fn report() -> &'static [Loaded] {
    PRESETS.get().map_or(&[], |(_, r)| r.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layouts_that_scroll_hide_no_rows_and_the_others_keep_their_limit() {
        for id in ["recent", "machine-rail", "machine-fold"] {
            assert_eq!(find(id).row_limit(), usize::MAX, "{id}");
        }
        assert_eq!(find("directory").row_limit(), 100);
    }

    #[test]
    fn plugin_presets_add_to_the_builtin_ones() {
        let d = std::env::temp_dir().join(format!("kl-sidebar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let write = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, body).unwrap();
            p
        };
        let good = write(
            "a.json",
            r#"{"api":1,"presets":[{"id":"quiet","name":"安静","group":"status","fold":["running","done","stopped","stale"],"detail":"always","limit":8}]}"#,
        );
        let dup = write(
            "b.json",
            r#"{"api":1,"presets":[{"id":"directory","name":"又一个","group":"flat"}]}"#,
        );
        let attention = write(
            "c.json",
            r#"{"api":1,"presets":[{"id":"hide","name":"藏","group":"flat","fold":["attention"]}]}"#,
        );
        let unknown = write(
            "d.json",
            r#"{"api":1,"presets":[{"id":"x","name":"x","group":"by-tag"}]}"#,
        );
        let (presets, report) = load(vec![good, dup, attention, unknown]);
        assert_eq!(
            presets.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            [
                "recent",
                "machine-rail",
                "machine-fold",
                "directory",
                "quiet"
            ]
        );
        assert_eq!(presets[4].limit, 8);
        assert!(presets[4].folds(Bucket::Running));
        assert_eq!(report[0].result, Ok("安静".into()));
        assert!(report[1].result.as_ref().unwrap_err().contains("重名"));
        assert!(
            report[2]
                .result
                .as_ref()
                .unwrap_err()
                .contains(crate::status::needs_you_word())
        );
        assert!(report[3].result.as_ref().unwrap_err().contains("格式不对"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_wasm_preset_names_a_module_inside_its_plugin() {
        let d = std::env::temp_dir().join(format!("kl-sidebar-wasm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("view.wasm"), b"\0asm").unwrap();
        let write = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, body).unwrap();
            p
        };
        let good = write(
            "a.json",
            r#"{"api":1,"presets":[{"id":"mine","name":"我的","group":"wasm","wasm":"view.wasm"}]}"#,
        );
        let outside = write(
            "b.json",
            r#"{"api":1,"presets":[{"id":"x","name":"x","group":"wasm","wasm":"../view.wasm"}]}"#,
        );
        let missing = write(
            "c.json",
            r#"{"api":1,"presets":[{"id":"y","name":"y","group":"wasm","wasm":"gone.wasm"}]}"#,
        );
        let stray = write(
            "d.json",
            r#"{"api":1,"presets":[{"id":"z","name":"z","group":"flat","wasm":"view.wasm"}]}"#,
        );
        let (presets, report) = load(vec![good, outside, missing, stray]);
        assert_eq!(
            presets.last().unwrap().wasm_path(),
            Some(d.join("view.wasm"))
        );
        assert!(
            report[1]
                .result
                .as_ref()
                .unwrap_err()
                .contains(".wasm 文件名")
        );
        assert!(
            report[2]
                .result
                .as_ref()
                .unwrap_err()
                .contains("不在插件目录里")
        );
        assert!(report[3].result.as_ref().unwrap_err().contains("group"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_choice_of_a_retired_layout_opens_the_default() {
        assert_eq!(find("classic").id, "recent");
        assert_eq!(find("status").id, "recent");
    }

    #[test]
    fn only_a_flat_list_or_a_module_keeps_to_one_space() {
        let with = |group| Preset {
            group,
            ..find("recent").clone()
        };
        for group in [Grouping::Recent, Grouping::Status, Grouping::Directory] {
            assert!(with(group).all_spaces(), "{group:?}");
        }
        for group in [Grouping::Flat, Grouping::Wasm] {
            assert!(!with(group).all_spaces(), "{group:?}");
        }
    }

    #[test]
    fn a_preset_says_where_other_machines_go() {
        assert_eq!(find("recent").machines, Machines::Mixed);
        assert_eq!(find("machine-rail").machines, Machines::Rail);
        assert_eq!(find("machine-fold").machines, Machines::Sections);
        assert_eq!(find("directory").machines, Machines::Below);
        assert!(!find("recent").only_with_machines());
        assert!(find("machine-rail").only_with_machines());
        let d = std::env::temp_dir().join(format!("kl-sidebar-machines-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let write = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, body).unwrap();
            p
        };
        let good = write(
            "a.json",
            r#"{"api":1,"presets":[{"id":"mine","name":"我的","group":"recent","machines":"rail","limit":6}]}"#,
        );
        let bad = write(
            "b.json",
            r#"{"api":1,"presets":[{"id":"nope","name":"不行","group":"flat","machines":"sections"}]}"#,
        );
        let (presets, report) = load(vec![good, bad]);
        assert_eq!(presets.last().unwrap().machines, Machines::Rail);
        assert!(report[1].result.as_ref().unwrap_err().contains("machines"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_choice_falls_back_to_session_first() {
        assert_eq!(find("gone").id, "recent");
        assert_eq!(find("gone").group, Grouping::Recent);
        assert_eq!(find("directory").group, Grouping::Directory);
    }
}
