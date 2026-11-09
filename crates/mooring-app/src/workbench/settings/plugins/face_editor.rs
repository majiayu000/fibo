//! 自己画一个: the frames of a face, edited one at a time and saved as a plugin. It exists
//! only while open: nothing here runs while a completion animation plays, which reads the
//! saved frames like the shipped ones. 试播 on a pane closes settings, so the draft waits
//! for the next time it is opened.
use super::*;
use crate::ui::field::Field;
use gpui::{Subscription, relative};
use mooring_core::effect::Style;
use mooring_core::face::{self, Frame, Script, Sheet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PREVIEW_SIZE: f32 = 40.;
const PREVIEW_HEIGHT: f32 = 88.;
const STRIP_HEIGHT: f32 = 6.;
const SHELL_FIELD: f32 = 56.;
const PART_FIELD: f32 = 64.;
const HOLD_FIELD: f32 = 76.;
const NAME_FIELD: f32 = 200.;
const LABEL_COLUMN: f32 = 44.;
/// How often 播放 redraws the preview, only while it plays.
const PLAY_TICK_MS: u64 = 33;

/// What 试播 on a pane leaves behind for when settings opens again.
#[derive(Clone)]
struct Draft {
    id: Option<String>,
    name: String,
    style: Style,
    sheet: Sheet,
}
static DRAFT: Mutex<Option<Draft>> = Mutex::new(None);
fn draft() -> std::sync::MutexGuard<'static, Option<Draft>> {
    DRAFT.lock().unwrap_or_else(|e| e.into_inner())
}
/// Whether a drawing is waiting to be picked up.
pub(super) fn has_draft() -> bool {
    draft().is_some()
}

pub(in crate::workbench) struct FaceEditor {
    /// The plugin being changed; None makes a new one on 保存.
    id: Option<String>,
    style: Style,
    sheet: Sheet,
    selected: usize,
    name: Entity<Field>,
    open: Entity<Field>,
    close: Entity<Field>,
    left: Entity<Field>,
    mouth: Entity<Field>,
    right: Entity<Field>,
    hold: Entity<Field>,
    _watching: Vec<Subscription>,
    /// When 播放 started, while it plays.
    playing: Option<Instant>,
    /// Why 保存 failed.
    failed: Option<String>,
}
impl FaceEditor {
    fn run_ms(&self) -> u32 {
        self.style.default_ms() as u32
    }
    fn script(&self) -> Result<Script, String> {
        Script::build(self.sheet.clone(), self.run_ms())
    }
}

impl SettingsView {
    /// Opens the editor on `sheet`, or on the waiting draft when starting from nothing.
    pub(super) fn face_open(
        &mut self,
        start: Option<(Option<String>, String, Style, Sheet)>,
        cx: &mut Context<Self>,
    ) {
        let waiting = draft().take();
        let (id, name, style, sheet) = match (start, waiting) {
            (Some(s), _) => s,
            (None, Some(d)) => (d.id, d.name, d.style, d.sheet),
            (None, None) => (
                None,
                String::new(),
                Style::Wake,
                face::builtin_sheet(Style::Wake),
            ),
        };
        let field = |cx: &mut Context<Self>, label: &str, text: &str| {
            let label = label.to_owned();
            let text = text.to_owned();
            cx.new(move |cx| Field::new(&label, &text, cx).compact())
        };
        let first = sheet.frames.first().cloned();
        let part = |pick: fn(&Frame) -> String| first.as_ref().map_or_else(String::new, pick);
        let name = field(cx, t!("名字"), &name);
        let open = field(cx, t!("左边"), &sheet.open);
        let close = field(cx, t!("右边"), &sheet.close);
        let left = field(cx, t!("左眼"), &part(|f| f.left.clone()));
        let mouth = field(cx, t!("嘴"), &part(|f| f.mouth.clone()));
        let right = field(cx, t!("右眼"), &part(|f| f.right.clone()));
        let hold = field(cx, t!("毫秒"), &part(|f| f.ms.to_string()));
        let watching = [&name, &open, &close, &left, &mouth, &right, &hold]
            .into_iter()
            .map(|f| cx.observe(f, |this, _, cx| this.face_read_fields(cx)))
            .collect();
        self.market.face = Some(FaceEditor {
            id,
            style,
            sheet,
            selected: 0,
            name,
            open,
            close,
            left,
            mouth,
            right,
            hold,
            _watching: watching,
            playing: None,
            failed: None,
        });
        cx.notify();
    }
    /// What was typed goes into the frame being edited.
    fn face_read_fields(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.market.face.as_mut() else {
            return;
        };
        let read = |f: &Entity<Field>| f.read(cx).text.clone();
        e.sheet.open = read(&e.open);
        e.sheet.close = read(&e.close);
        let (left, mouth, right, hold) =
            (read(&e.left), read(&e.mouth), read(&e.right), read(&e.hold));
        if let Some(f) = e.sheet.frames.get_mut(e.selected) {
            f.left = left;
            f.mouth = mouth;
            f.right = right;
            // Not a number: refused by the check under the frames until it is one.
            f.ms = hold.trim().parse().unwrap_or(0);
        }
        e.failed = None;
        cx.notify();
    }
    /// Shows the selected frame, and the shell, in the fields.
    fn face_fill_fields(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.market.face.as_ref() else {
            return;
        };
        let Some(f) = e.sheet.frames.get(e.selected).cloned() else {
            return;
        };
        let set = |field: &Entity<Field>, text: &str, cx: &mut Context<Self>| {
            field.update(cx, |field, cx| {
                field.set_text(text);
                cx.notify();
            });
        };
        let (open, close) = (e.sheet.open.clone(), e.sheet.close.clone());
        let fields = [
            (e.open.clone(), open),
            (e.close.clone(), close),
            (e.left.clone(), f.left),
            (e.mouth.clone(), f.mouth),
            (e.right.clone(), f.right),
            (e.hold.clone(), f.ms.to_string()),
        ];
        for (field, text) in fields {
            set(&field, &text, cx);
        }
    }
    /// Changes the sheet and shows the frame `selected` it leaves selected.
    fn face_change(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut FaceEditor)) {
        let Some(e) = self.market.face.as_mut() else {
            return;
        };
        change(e);
        e.selected = e.selected.min(e.sheet.frames.len().saturating_sub(1));
        e.failed = None;
        self.face_fill_fields(cx);
        cx.notify();
    }
    fn face_play(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.market.face.as_mut() else {
            return;
        };
        if e.playing.is_some() || e.script().is_err() {
            return;
        }
        let run = Duration::from_millis(u64::from(e.run_ms()));
        e.playing = Some(Instant::now());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let started = Instant::now();
            while started.elapsed() < run {
                cx.background_executor()
                    .timer(Duration::from_millis(PLAY_TICK_MS))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if let Some(e) = this.market.face.as_mut() {
                    e.playing = None;
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// Plays the drawing in its scene on the shown pane, which closes settings.
    fn face_try(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.market.face.as_mut() else {
            return;
        };
        let script = match e.script() {
            Ok(s) => s,
            Err(why) => {
                e.failed = Some(why);
                cx.notify();
                return;
            }
        };
        *draft() = Some(Draft {
            id: e.id.clone(),
            name: e.name.read(cx).text.clone(),
            style: e.style,
            sheet: e.sheet.clone(),
        });
        let complete = Complete {
            face: Some(face::intern(script)),
            ..Complete::of(e.style)
        };
        cx.emit(SettingsAction::PreviewComplete(Some(complete)));
    }
    /// Writes the plugin and plays it from now on.
    fn face_save(&mut self, cx: &mut Context<Self>) {
        let Some(e) = self.market.face.as_mut() else {
            return;
        };
        let Some(plugins) = plugin::config().map(plugin::dir) else {
            e.failed = Some(t!("找不到配置目录").into());
            cx.notify();
            return;
        };
        let id =
            e.id.clone()
                .unwrap_or_else(|| face::new_plugin_id(&plugins));
        let name = e.name.read(cx).text.clone();
        let saved = face::save_plugin(&plugins, &id, &name, e.style, e.run_ms(), &e.sheet);
        if let Err(why) = saved {
            e.failed = Some(why);
            cx.notify();
            return;
        }
        self.market.face = None;
        *draft() = None;
        self.plugins_changed(cx);
        let picked = mooring_core::effect::Pick::Plugin(id);
        self.text_error = mooring_core::effect::pick(Some(picked)).err();
        self.market.done = Some(Ok(t!("「{name}」画好了，已经换成它", name = name.trim())));
        cx.notify();
    }

    /// A row of small buttons that set something on the frame.
    fn face_chips(
        &self,
        key: &str,
        title: &str,
        choices: &[&'static str],
        cx: &Context<Self>,
        set: fn(&mut Frame, &str),
    ) -> gpui::Div {
        let mut row = div().flex().flex_wrap().items_center().gap(px(T::S6));
        row = row.child(face_label(title));
        for (i, choice) in choices.iter().copied().enumerate() {
            row = row.child(
                secondary(format!("face-{key}-{i}"), choice)
                    .font(T::mono(cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.face_change(cx, |e| {
                            if let Some(f) = e.sheet.frames.get_mut(e.selected) {
                                set(f, choice);
                            }
                        });
                    })),
            );
        }
        row
    }

    /// 完成动画's first row: the editor, or what opens it.
    pub(super) fn face_card(&self, cx: &Context<Self>) -> gpui::AnyElement {
        if let Some(e) = &self.market.face {
            return self.face_editor(e, cx);
        }
        let (text, note) = if has_draft() {
            (t!("接着画"), t!("上次试播的那张还留着"))
        } else {
            (t!("自己画一个"), t!("一帧一帧改眼睛和嘴，存成插件"))
        };
        let draw = secondary("complete-draw", text)
            .on_click(cx.listener(|this, _, _, cx| this.face_open(None, cx)));
        self.row(true, text, note, draw).into_any_element()
    }
    /// 改一改 on a face animation's row: one's own is changed in place, a shipped one copied.
    pub(super) fn face_edit_button(
        &self,
        choice: &mooring_core::effect::Pick,
        title: &str,
        complete: Complete,
        cx: &Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let mooring_core::effect::Pick::Plugin(id) = choice else {
            return None;
        };
        if !Style::FACES.contains(&complete.style) {
            return None;
        }
        let own = id.starts_with("my-face-").then(|| id.clone());
        let start = (
            own,
            title.to_owned(),
            complete.style,
            complete.drawn_sheet(),
        );
        Some(
            secondary(format!("complete-edit-{id}"), t!("改一改")).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.face_open(Some(start.clone()), cx);
                },
            )),
        )
    }

    /// The editor, as one row of the 完成动画 card.
    fn face_editor(&self, e: &FaceEditor, cx: &Context<Self>) -> gpui::AnyElement {
        let script = e.script();
        let run = e.run_ms() as f32;
        // While playing the preview runs the clock; else it shows the frame being edited.
        let played = e.playing.zip(script.as_ref().ok()).map(|(from, s)| {
            let t = (from.elapsed().as_secs_f32() * 1000. / run).min(1.);
            (s.at(t).to_string(), s.frame_at(t))
        });
        let shown = played
            .as_ref()
            .map(|(text, _)| text.clone())
            .unwrap_or_else(|| {
                e.sheet
                    .frames
                    .get(e.selected)
                    .map_or_else(String::new, |f| e.sheet.text(f))
            });
        let current = played.as_ref().map_or(e.selected, |(_, i)| *i);

        let preview = div()
            .h(px(PREVIEW_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(T::CONTROL_RADIUS))
            .bg(T::color(T::popover()))
            .font(T::mono(cx))
            .text_size(px(PREVIEW_SIZE))
            .text_color(T::color(T::primary()))
            .child(shown);

        let mut scenes = div().flex().flex_wrap().items_center().gap(px(T::S6));
        scenes = scenes.child(face_label(t!("场景")));
        for style in Style::FACES {
            let on = style == e.style;
            scenes = scenes.child(
                secondary(format!("face-scene-{}", style.name()), style.label())
                    .when(on, |b| b.border_color(T::color(T::primary())))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.face_change(cx, |e| {
                            // Frames still the scene's own follow it; drawn ones stay.
                            if e.sheet == face::builtin_sheet(e.style) {
                                e.sheet = face::builtin_sheet(style);
                                e.selected = 0;
                            }
                            e.style = style;
                        });
                    })),
            );
        }

        // Each frame's share of the run, so the rhythm shows.
        let held = e.sheet.held_ms().max(1) as f32;
        let total = run.max(held);
        let mut strip = div()
            .h(px(STRIP_HEIGHT))
            .w_full()
            .flex()
            .gap(px(T::BORDER_WIDTH))
            .rounded(px(T::CONTROL_RADIUS))
            .overflow_hidden()
            .bg(T::color(T::control()));
        let mut chips = div().flex().flex_wrap().gap(px(T::S6));
        let last = e.sheet.frames.len().saturating_sub(1);
        for (i, f) in e.sheet.frames.iter().enumerate() {
            let on = i == current;
            let ms = if i == last { total - held } else { f.ms as f32 };
            strip = strip.child(
                div()
                    .h_full()
                    .flex_shrink_0()
                    .w(relative((ms / total).clamp(0., 1.)))
                    .bg(T::color(if on { T::primary() } else { T::secondary() })),
            );
            let hold = if i == last {
                t!("停住").to_owned()
            } else {
                format!("{}", f.ms)
            };
            chips = chips.child(
                div()
                    .id(("face-frame", i))
                    .role(gpui::accesskit::Role::Button)
                    .aria_label(t!("第 {n} 帧", n = i + 1))
                    .px(px(T::S8))
                    .py(px(T::S4))
                    .flex()
                    .flex_col()
                    .items_center()
                    .border(px(T::BORDER_WIDTH))
                    .border_color(T::color(if on { T::primary() } else { T::control() }))
                    .rounded(px(T::CONTROL_RADIUS))
                    .cursor_pointer()
                    .child(
                        div()
                            .font(T::mono(cx))
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .child(e.sheet.text(f)),
                    )
                    .child(label(if f.scramble {
                        t!("{hold} · 乱码", hold)
                    } else {
                        hold
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.face_change(cx, |e| e.selected = i);
                    })),
            );
        }

        let on_scramble = e.sheet.frames.get(e.selected).is_some_and(|f| f.scramble);
        let box_of = |width: f32, field: &Entity<Field>| div().w(px(width)).child(field.clone());
        let parts = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(T::S8))
            .child(face_label(t!("这帧")))
            .child(label(t!("左眼").to_owned()))
            .child(box_of(PART_FIELD, &e.left))
            .child(label(t!("嘴").to_owned()))
            .child(box_of(PART_FIELD, &e.mouth))
            .child(label(t!("右眼").to_owned()))
            .child(box_of(PART_FIELD, &e.right))
            .child(label(t!("毫秒").to_owned()))
            .child(box_of(HOLD_FIELD, &e.hold))
            .child(label(t!("乱码进场").to_owned()))
            .child(
                switch("face-scramble", on_scramble).on_click(cx.listener(|this, _, _, cx| {
                    this.face_change(cx, |e| {
                        if let Some(f) = e.sheet.frames.get_mut(e.selected) {
                            f.scramble = !f.scramble;
                        }
                    });
                })),
            );
        let shell = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(T::S8))
            .child(face_label(t!("外框")))
            .child(box_of(SHELL_FIELD, &e.open))
            .child(label(t!("脸").to_owned()))
            .child(box_of(SHELL_FIELD, &e.close))
            .child(label(t!("名字").to_owned()))
            .child(box_of(NAME_FIELD, &e.name));

        let edit = |key: &str, text: &str| secondary(format!("face-{key}"), text.to_owned());
        let frames_buttons = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(T::S6))
            .child(face_label(t!("帧")))
            .child(
                edit("add", t!("复制这帧")).on_click(cx.listener(|this, _, _, cx| {
                    this.face_change(cx, |e| {
                        if e.sheet.frames.len() < face::MAX_FRAMES
                            && let Some(f) = e.sheet.frames.get(e.selected).cloned()
                        {
                            e.sheet.frames.insert(e.selected + 1, f);
                            e.selected += 1;
                        }
                    });
                })),
            )
            .child(
                edit("remove", t!("删掉")).on_click(cx.listener(|this, _, _, cx| {
                    this.face_change(cx, |e| {
                        if e.sheet.frames.len() > 1 {
                            e.sheet.frames.remove(e.selected);
                        }
                    });
                })),
            )
            .child(
                edit("earlier", t!("往前")).on_click(cx.listener(|this, _, _, cx| {
                    this.face_change(cx, |e| {
                        if e.selected > 0 {
                            e.sheet.frames.swap(e.selected, e.selected - 1);
                            e.selected -= 1;
                        }
                    });
                })),
            )
            .child(
                edit("later", t!("往后")).on_click(cx.listener(|this, _, _, cx| {
                    this.face_change(cx, |e| {
                        if e.selected + 1 < e.sheet.frames.len() {
                            e.sheet.frames.swap(e.selected, e.selected + 1);
                            e.selected += 1;
                        }
                    });
                })),
            );

        let problem = e.failed.clone().or_else(|| script.as_ref().err().cloned());
        let most = (run * face::MAX_SHARE) as u32;
        let note = div()
            .text_size(px(T::SMALL))
            .text_color(if problem.is_some() {
                Tone::Danger.ink()
            } else {
                T::color(T::secondary())
            })
            .child(problem.unwrap_or_else(|| {
                t!(
                    "最后一帧之前共 {held_ms} 毫秒，最多 {most}；最后一帧停到脸离开，它第一次出现时开始开心",
                    held_ms = e.sheet.held_ms(),
                    most
                )
            }));
        let actions = div()
            .flex()
            .flex_wrap()
            .gap(px(T::S8))
            .child(
                edit("play", t!("▶ 播放"))
                    .on_click(cx.listener(|this, _, _, cx| this.face_play(cx))),
            )
            .child(
                edit("try", t!("在窗格上试播"))
                    .on_click(cx.listener(|this, _, _, cx| this.face_try(cx))),
            )
            .child(
                edit("save", t!("保存并用它"))
                    .on_click(cx.listener(|this, _, _, cx| this.face_save(cx))),
            )
            .child(
                edit("cancel", t!("取消")).on_click(cx.listener(|this, _, _, cx| {
                    this.market.face = None;
                    *draft() = None;
                    cx.notify();
                })),
            );

        div()
            .px(px(T::S16))
            .py(px(T::S12))
            .flex()
            .flex_col()
            .gap(px(T::S10))
            .child(preview)
            .child(strip)
            .child(chips)
            .child(parts)
            .child(
                self.face_chips("eyes", t!("眼睛"), &face::EYES, cx, |f, c| {
                    f.left = c.to_owned();
                    f.right = c.to_owned();
                }),
            )
            .child(
                self.face_chips("mouth", t!("嘴"), &face::MOUTHS, cx, |f, c| {
                    f.mouth = c.to_owned();
                }),
            )
            .child(frames_buttons)
            .child(shell)
            .child(scenes)
            .child(note)
            .child(actions)
            .into_any_element()
    }
}

fn face_label(text: &str) -> gpui::Div {
    div().w(px(LABEL_COLUMN)).child(label(text.to_owned()))
}
