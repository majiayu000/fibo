//! The faces of the warm completion animations, as frames: two eyes and a mouth in a shell
//! (`c[` … `]`), each held for some milliseconds. The app's scenes play them and settings'
//! frame editor writes them into a plugin's `effects.json`. Everything is worked out when a
//! script is built: playing one copies a reference to the text, except during a scramble,
//! when a few short strings are made in a fraction of a second.
use crate::effect::Style;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const MAX_FRAMES: usize = 24;
/// One frame's face in characters, shell included.
pub const MAX_WIDTH: usize = 24;
pub const MIN_HOLD_MS: u32 = 20;
pub const MAX_HOLD_MS: u32 = 3_000;
/// The share of the run the frames before the last may take: after it the face is leaving.
pub const MAX_SHARE: f32 = 0.8;
/// How long a frame marked `scramble` shows random characters before settling.
const SCRAMBLE_MS: u32 = 240;
/// How many different random strings that is, whatever the run's length.
const SCRAMBLE_STEPS: usize = 6;
/// Few and all one cell wide, so a scramble adds few glyphs and does not change the width.
const SCRAMBLE_CHARS: [char; 9] = ['-', '^', 'o', '=', '*', '~', '+', '.', ':'];
/// What the frame editor offers with one click.
pub const EYES: [&str; 7] = ["-", "^", "o", "=", "*", "~", "u"];
pub const MOUTHS: [&str; 5] = ["_", "—", ".", "o", "v"];

/// One frame: held for `ms`, the last one until the face leaves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub left: String,
    pub mouth: String,
    pub right: String,
    pub ms: u32,
    /// Arrives through random characters that settle from left to right.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scramble: bool,
}

/// A face as it is written: what `effects.json` holds and the editor changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub open: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub close: String,
    pub frames: Vec<Frame>,
}
impl Sheet {
    /// A frame as shown.
    pub fn text(&self, f: &Frame) -> String {
        format!(
            "{}{} {} {}{}",
            self.open, f.left, f.mouth, f.right, self.close
        )
    }
    /// The milliseconds the frames before the last take.
    pub fn held_ms(&self) -> u32 {
        let n = self.frames.len().saturating_sub(1);
        self.frames[..n].iter().map(|f| f.ms).sum()
    }
    fn check(&self, run_ms: u32) -> Result<(), String> {
        if self.frames.is_empty() || self.frames.len() > MAX_FRAMES {
            return Err(format!("帧数要在 1–{MAX_FRAMES} 之间"));
        }
        let shell = [&self.open, &self.close];
        if shell.iter().any(|s| s.chars().any(char::is_control)) {
            return Err("外框不能有控制字符".into());
        }
        let mut width = None;
        for (i, f) in self.frames.iter().enumerate() {
            let at = i + 1;
            if !(MIN_HOLD_MS..=MAX_HOLD_MS).contains(&f.ms) {
                return Err(format!(
                    "第 {at} 帧的时长要在 {MIN_HOLD_MS}–{MAX_HOLD_MS} 毫秒之间"
                ));
            }
            let parts = [&f.left, &f.mouth, &f.right];
            if parts.iter().any(|p| p.chars().any(char::is_control)) {
                return Err(format!("第 {at} 帧不能有控制字符"));
            }
            if parts.iter().any(|p| p.trim().is_empty()) {
                return Err(format!("第 {at} 帧的眼睛和嘴不能空着"));
            }
            let n = self.text(f).chars().count();
            if n > MAX_WIDTH {
                return Err(format!("第 {at} 帧太长了，最多 {MAX_WIDTH} 个字符"));
            }
            if *width.get_or_insert(n) != n {
                return Err(format!("第 {at} 帧和第 1 帧不一样宽，播放时脸会抖"));
            }
        }
        let most = (run_ms as f32 * MAX_SHARE) as u32;
        if self.held_ms() > most {
            return Err(format!(
                "最后一帧之前加起来 {} 毫秒，这个动画最多 {most} 毫秒，再往后脸就要离开了",
                self.held_ms()
            ));
        }
        Ok(())
    }
}

/// One frame on the run's clock.
#[derive(Debug)]
struct Beat {
    at: f32,
    /// Random characters show until here.
    settled: f32,
    text: Arc<str>,
}

/// A sheet ready to play: when each frame comes on, in fractions of the run.
#[derive(Debug)]
pub struct Script {
    sheet: Sheet,
    beats: Vec<Beat>,
    smile: f32,
}
impl PartialEq for Script {
    fn eq(&self, other: &Self) -> bool {
        self.sheet == other.sheet
            && self.beats.len() == other.beats.len()
            && self.smile == other.smile
    }
}
impl Eq for Script {}

impl Script {
    /// Checks `sheet` for a run of `run_ms` and works out its clock.
    pub fn build(sheet: Sheet, run_ms: u32) -> Result<Self, String> {
        sheet.check(run_ms)?;
        let run = run_ms as f32;
        let mut clock = 0;
        let mut beats = Vec::with_capacity(sheet.frames.len());
        for f in &sheet.frames {
            let at = clock as f32 / run;
            let settled = if f.scramble {
                (clock + SCRAMBLE_MS.min(f.ms)) as f32 / run
            } else {
                at
            };
            beats.push(Beat {
                at,
                settled,
                text: sheet.text(f).into(),
            });
            clock += f.ms;
        }
        // The first time it shows what it ends on, not counting a blink before the end.
        let last = beats.last().map(|b| b.text.clone());
        let smile = beats
            .iter()
            .find(|b| Some(&b.text) == last.as_ref())
            .map_or(0., |b| b.at);
        Ok(Self {
            sheet,
            beats,
            smile,
        })
    }
    pub fn sheet(&self) -> &Sheet {
        &self.sheet
    }
    /// When it reaches the face it ends on: when the scene turns happy.
    pub fn smiles_at(&self) -> f32 {
        self.smile
    }
    /// The index of the frame showing at `t`.
    pub fn frame_at(&self, t: f32) -> usize {
        self.beats.partition_point(|b| b.at <= t).saturating_sub(1)
    }
    /// The face showing at `t`, the same text for the same `t`.
    pub fn at(&self, t: f32) -> Arc<str> {
        let i = self.frame_at(t);
        let beat = &self.beats[i];
        if t >= beat.settled || beat.settled <= beat.at {
            return beat.text.clone();
        }
        let k = (t - beat.at).max(0.) / (beat.settled - beat.at);
        let step = ((k * SCRAMBLE_STEPS as f32) as usize).min(SCRAMBLE_STEPS - 1);
        self.scrambled(i, step).into()
    }
    /// Frame `i` with the characters not settled yet at `step` swapped for random ones.
    fn scrambled(&self, i: usize, step: usize) -> String {
        let f = &self.sheet.frames[i];
        let from = self.sheet.open.chars().count();
        let to = self.sheet.text(f).chars().count() - self.sheet.close.chars().count();
        let settled = (to - from) * step / SCRAMBLE_STEPS;
        self.sheet
            .text(f)
            .chars()
            .enumerate()
            .map(|(at, c)| {
                if c == ' ' || at < from + settled || at >= to {
                    return c;
                }
                let mix = (at * 7 + step * 13 + i * 3) % SCRAMBLE_CHARS.len();
                SCRAMBLE_CHARS[mix]
            })
            .collect()
    }

    /// What a warm style plays unless its plugin draws its own.
    pub fn builtin(style: Style) -> Option<&'static Script> {
        static ALL: OnceLock<Vec<(Style, Script)>> = OnceLock::new();
        ALL.get_or_init(|| {
            Style::FACES
                .into_iter()
                .filter_map(|s| {
                    let script = Script::build(builtin_sheet(s), s.default_ms() as u32);
                    debug_assert!(script.is_ok(), "{s:?}: {script:?}");
                    Some((s, script.ok()?))
                })
                .collect()
        })
        .iter()
        .find(|(s, _)| *s == style)
        .map(|(_, script)| script)
    }
}

/// A script that lives as long as the app, so `effect::Complete` stays a plain copy for
/// each frame. The same sheet is kept once, so this grows only with what was really drawn.
pub fn intern(script: Script) -> &'static Script {
    static KEPT: Mutex<Vec<&'static Script>> = Mutex::new(Vec::new());
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(same) = kept.iter().find(|k| ***k == script) {
        return same;
    }
    let script: &'static Script = Box::leak(Box::new(script));
    kept.push(script);
    script
}

fn frame(left: &str, mouth: &str, right: &str, ms: u32) -> Frame {
    Frame {
        left: left.into(),
        mouth: mouth.into(),
        right: right.into(),
        ms,
        scramble: false,
    }
}
fn scramble(left: &str, mouth: &str, right: &str, ms: u32) -> Frame {
    Frame {
        scramble: true,
        ..frame(left, mouth, right, ms)
    }
}
/// The frames each style ships with. Long holds with short ones between are the rhythm;
/// after the smile each one blinks or lights up once, so it is not a still picture.
pub fn builtin_sheet(style: Style) -> Sheet {
    let plain = |frames| Sheet {
        open: String::new(),
        close: String::new(),
        frames,
    };
    match style {
        Style::Relief => Sheet {
            open: " ".into(),
            close: String::new(),
            frames: vec![
                frame("-", "_", "- ", 420),
                frame("-", "_", "-;", 360),
                frame("-", ".", "-;", 240),
                frame("-", "o", "- ", 300),
                frame("-", "_", "- ", 120),
                frame("^", "_", "- ", 120),
                frame("^", "—", "^ ", 480),
                frame("-", "—", "- ", 90),
                frame("^", "—", "^ ", 0).held(),
            ],
        },
        Style::Wake => plain(vec![
            frame("=", "_", "=", 1_200),
            frame("=", "_", "o", 240),
            frame("o", "_", "o", 180),
            frame("-", "_", "-", 90),
            frame("o", "_", "o", 150),
            scramble("^", "—", "^", 360),
            frame("-", "—", "-", 90),
            frame("^", "—", "^", 0).held(),
        ]),
        Style::Sunrise => plain(vec![
            frame("-", "_", "-", 1_410),
            scramble("^", "—", "^", 420),
            frame("*", "—", "*", 120),
            frame("^", "—", "^", 0).held(),
        ]),
        Style::Heart => plain(vec![
            frame("-", "_", "-", 1_080),
            frame("o", "_", "o", 180),
            scramble("^", "—", "^", 420),
            frame("*", "—", "*", 120),
            frame("^", "—", "^", 0).held(),
        ]),
        Style::Cat => Sheet {
            open: "=".into(),
            close: "=".into(),
            frames: vec![
                frame("-", ".", "-", 780),
                frame("^", ".", "-", 210),
                frame("-", ".", "^", 210),
                frame("-", ".", "-", 180),
                frame("^", ".", "^", 540),
                frame("-", ".", "-", 90),
                frame("^", ".", "^", 0).held(),
            ],
        },
        Style::Tea => Sheet {
            open: "c[".into(),
            close: "]".into(),
            frames: vec![
                frame("-", "_", "-", 1_380),
                frame("^", "_", "-", 120),
                frame("^", "—", "^", 540),
                frame("-", "—", "-", 120),
                frame("^", "—", "^", 0).held(),
            ],
        },
        _ => plain(vec![
            frame("-", "_", "-", 540),
            scramble("^", "—", "^", 300),
            frame("*", "—", "*", 120),
            frame("^", "—", "^", 0).held(),
        ]),
    }
}
impl Frame {
    /// The last frame: it stays until the face leaves, so its own length means nothing.
    fn held(mut self) -> Self {
        self.ms = 300;
        self
    }
}

/// The plugin id for a new drawing: the first free `my-face-<n>`.
pub fn new_plugin_id(plugins: &Path) -> String {
    (1u32..)
        .map(|n| format!("my-face-{n}"))
        .find(|id| !plugins.join(id).exists())
        .unwrap_or_else(|| "my-face".into())
}

/// Writes a drawing as a plugin in `plugins`, replacing what `id` held. Nothing changes on
/// disk if the sheet does not check out.
pub fn save_plugin(
    plugins: &Path,
    id: &str,
    name: &str,
    style: Style,
    run_ms: u32,
    sheet: &Sheet,
) -> Result<PathBuf, String> {
    Script::build(sheet.clone(), run_ms)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("给它起个名字".into());
    }
    let fail = |e: std::io::Error| format!("保存失败：{e}");
    let staging = plugins.join(format!(".{id}.saving"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(fail)?;
    let manifest = serde_json::json!({
        "api": crate::plugin::API,
        "id": id,
        "name": format!("完成动画：{name}"),
        "version": "1.0.0",
        "description": format!("自己画的：{}", sheet.frames.iter().map(|f| sheet.text(f)).collect::<Vec<_>>().join(" → ")),
        "provides": ["effects"],
    });
    let effects = serde_json::json!({
        "api": crate::effect::API,
        "complete": {
            "style": style.name(),
            "duration_ms": run_ms,
            "land": "sidebar",
            "face": sheet,
        },
    });
    let write = |file: &str, value: &serde_json::Value| {
        let text = serde_json::to_string_pretty(value).unwrap_or_default();
        std::fs::write(staging.join(file), text + "\n").map_err(fail)
    };
    write("plugin.json", &manifest)?;
    write("effects.json", &effects)?;
    crate::plugin::check(&staging).map_err(|e| format!("保存失败：{e}"))?;
    let dir = plugins.join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(fail)?;
    }
    std::fs::rename(&staging, &dir).map_err(fail)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(style: Style) -> &'static Script {
        Script::builtin(style).unwrap()
    }
    /// Every text a run shows, sampled finely.
    fn run(s: &Script) -> Vec<String> {
        (0..=1000)
            .map(|i| s.at(i as f32 / 1000.).to_string())
            .collect()
    }

    #[test]
    fn every_face_starts_tired_and_ends_smiling() {
        for style in Style::FACES {
            let s = script(style);
            let start = s.at(0.);
            assert!(!start.contains('^'), "{style:?} starts tired: {start}");
            let end = s.at(1.);
            assert_eq!(end.matches('^').count(), 2, "{style:?} ends ^ ^: {end}");
            assert_eq!(
                s.at(s.smiles_at() + 0.085).matches('^').count(),
                2,
                "{style:?} at its turn"
            );
            assert!(s.smiles_at() > 0.1, "{style:?} stays tired a while");
            assert!(style.greeting().is_some());
        }
    }

    #[test]
    fn after_the_smile_it_moves_once_and_comes_back() {
        for style in Style::FACES {
            let s = script(style);
            let after = run(s).split_off((s.smiles_at() * 1000.) as usize + 100);
            let end = after.last().unwrap().clone();
            assert!(
                after.iter().any(|f| *f != end),
                "{style:?} is not a still picture"
            );
        }
    }

    #[test]
    fn the_width_never_changes_even_while_scrambling() {
        for style in Style::FACES {
            let s = script(style);
            let width = s.at(0.).chars().count();
            for f in run(s) {
                assert_eq!(f.chars().count(), width, "{style:?}: {f}");
            }
        }
    }

    #[test]
    fn waking_opens_one_eye_before_the_other() {
        let s = script(Style::Wake);
        let shown = run(s);
        let one = shown.iter().position(|f| f == "= _ o").unwrap();
        let both = shown.iter().position(|f| f == "o _ o").unwrap();
        assert!(one < both);
    }

    #[test]
    fn a_scramble_is_short_settles_left_to_right_and_uses_few_characters() {
        let s = script(Style::Hop);
        let shown = run(s);
        let turn = (s.smiles_at() * 1000.).ceil() as usize;
        let settled = shown[turn..].iter().position(|f| f == "^ — ^").unwrap();
        assert!(settled > 0, "it scrambles first");
        assert!(settled <= (SCRAMBLE_MS as usize * 1000).div_ceil(3000) + 1);
        let random: std::collections::BTreeSet<&String> =
            shown[turn..turn + settled].iter().collect();
        assert!(
            random.len() >= 3 && random.len() <= SCRAMBLE_STEPS,
            "{random:?}"
        );
        for f in &random {
            assert!(
                f.chars()
                    .all(|c| c == ' ' || SCRAMBLE_CHARS.contains(&c) || "^—".contains(c)),
                "{f}"
            );
        }
        // The last step has the left eye already where it ends.
        assert!(shown[turn + settled - 1].starts_with('^'));
    }

    #[test]
    fn a_sheet_that_would_jitter_or_outstay_is_refused() {
        let sheet = |frames| Sheet {
            open: String::new(),
            close: String::new(),
            frames,
        };
        let ok = sheet(vec![frame("-", "_", "-", 500), frame("^", "—", "^", 300)]);
        assert!(Script::build(ok.clone(), 3_000).is_ok());
        let wide = sheet(vec![frame("-", "_", "-", 500), frame("^^", "—", "^", 300)]);
        assert!(Script::build(wide, 3_000).unwrap_err().contains("不一样宽"));
        let long = sheet(vec![frame("-", "_", "-", 2_500), frame("^", "—", "^", 300)]);
        assert!(
            Script::build(long, 3_000)
                .unwrap_err()
                .contains("脸就要离开")
        );
        let quick = sheet(vec![frame("-", "_", "-", 1)]);
        assert!(Script::build(quick, 3_000).unwrap_err().contains("时长"));
        assert!(Script::build(sheet(vec![]), 3_000).is_err());
        let blank = sheet(vec![frame(" ", "_", "-", 100)]);
        assert!(Script::build(blank, 3_000).unwrap_err().contains("空着"));
    }

    #[test]
    fn the_smile_is_the_first_time_it_shows_its_last_face() {
        let s = Script::build(
            Sheet {
                open: String::new(),
                close: String::new(),
                frames: vec![
                    frame("-", "_", "-", 300),
                    frame("u", "_", "u", 300),
                    frame("-", "_", "-", 300),
                    frame("u", "_", "u", 100),
                ],
            },
            3_000,
        )
        .unwrap();
        assert!((s.smiles_at() - 0.1).abs() < 1e-5);
    }

    #[test]
    fn the_same_drawing_is_kept_once() {
        let build = || Script::build(builtin_sheet(Style::Tea), 3_000).unwrap();
        assert!(std::ptr::eq(intern(build()), intern(build())));
    }

    #[test]
    fn a_saved_drawing_loads_as_a_plugin_and_saving_again_replaces_it() {
        let d = std::env::temp_dir().join(format!("kl-face-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let id = new_plugin_id(&d);
        assert_eq!(id, "my-face-1");
        let mut sheet = builtin_sheet(Style::Tea);
        let dir = save_plugin(&d, &id, "我的茶", Style::Tea, 3_000, &sheet).unwrap();
        assert_eq!(new_plugin_id(&d), "my-face-2");
        let (presets, report) = crate::effect::load(vec![dir.join("effects.json")]);
        assert!(report[0].result.is_ok(), "{:?}", report[0].result);
        let face = presets[0].complete.face.expect("the drawing");
        assert_eq!(face.sheet(), &sheet);

        sheet.frames[0].left = "u".into();
        sheet.frames[0].right = "u".into();
        save_plugin(&d, &id, "我的茶", Style::Tea, 3_000, &sheet).unwrap();
        let (presets, _) = crate::effect::load(vec![dir.join("effects.json")]);
        assert_eq!(
            presets[0].complete.face.unwrap().at(0.).as_ref(),
            "c[u _ u]"
        );

        sheet.frames.clear();
        assert!(save_plugin(&d, &id, "我的茶", Style::Tea, 3_000, &sheet).is_err());
        assert!(
            dir.join("effects.json").exists(),
            "a bad sheet leaves the saved one"
        );
        assert!(!d.join(format!(".{id}.saving")).exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
