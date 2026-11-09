//! 主题编辑器: every color a theme can set, the window following as you change them.
//! Colors you leave alone keep following 背景 and 前景. A draft shows everywhere and is
//! written only by 保存; 取消, or closing 设置, puts the saved theme back.
use super::*;
use crate::ui::appearance;
use crate::ui::palette::UI_TOKENS;
use crate::ui::theme::{Status, StatusStyle};
use crate::ui::themes;
use crate::ui::tone::Tone;
use gpui::{MouseDownEvent, MouseMoveEvent, MouseUpEvent, Subscription, canvas};
use mooring_core::theme::{ANSI_NAMES, ThemeFile, hex, parse_hex};

const SWATCH: f32 = 18.;
const SLIDER: f32 = 14.;
const SLIDER_STEPS: usize = 36;
const ROW_COLUMN: f32 = 310.;
const LIST_HEIGHT: f32 = 400.;
const THUMB: f32 = T::S4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Base,
    Terminal,
    Ansi,
    Window,
    Sidebar,
    Meaning,
}
impl Tab {
    const ALL: [Self; 6] = [
        Self::Base,
        Self::Terminal,
        Self::Ansi,
        Self::Window,
        Self::Sidebar,
        Self::Meaning,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Base => t!("基础"),
            Self::Terminal => t!("终端"),
            Self::Ansi => "ANSI",
            Self::Window => t!("界面"),
            Self::Sidebar => t!("侧栏"),
            Self::Meaning => t!("状态"),
        }
    }
    fn items(self) -> Vec<Item> {
        let item = |section, key, label| Item {
            section,
            key,
            label,
        };
        match self {
            Self::Base => vec![item("", "bg", t!("背景")), item("", "fg", t!("前景"))],
            Self::Terminal => vec![
                item("terminal", "cursor", t!("光标")),
                item("terminal", "cursor_text", t!("光标上的文字")),
                item("terminal", "selection", t!("选区")),
                item("terminal", "selected_text", t!("选中的文字")),
                item("terminal", "cursor_guide", t!("光标行")),
                item("terminal", "bold", t!("粗体")),
                item("terminal", "link", t!("链接")),
                item("terminal", "underline", t!("下划线")),
            ],
            Self::Ansi => ANSI_NAMES.iter().map(|n| item("ansi", n, n)).collect(),
            Self::Window => UI_TOKENS
                .iter()
                .filter(|(key, _)| !key.starts_with("sidebar"))
                .map(|(key, label)| item("ui", key, label))
                .collect(),
            Self::Sidebar => UI_TOKENS
                .iter()
                .filter(|(key, _)| key.starts_with("sidebar"))
                .map(|(key, label)| item("ui", key, label))
                .collect(),
            Self::Meaning => Status::ALL
                .iter()
                .map(|s| item("status", s.key(), s.label()))
                .chain([
                    item("tone", "ok", t!("语气：正常")),
                    item("tone", "warn", t!("语气：警告")),
                    item("tone", "danger", t!("语气：危险")),
                    item("seen", "title", t!("看过的会话标题")),
                    item("seen", "status", t!("看过的会话状态")),
                ])
                .collect(),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Item {
    section: &'static str,
    key: &'static str,
    label: &'static str,
}
impl Item {
    /// What the window paints it with now, set or derived.
    fn value(self, draft: &ThemeFile) -> u32 {
        let p = T::palette();
        match (self.section, self.key) {
            ("", "bg") => draft.bg,
            ("", "fg") => draft.fg,
            ("terminal", "cursor") => p.term.cursor,
            ("terminal", "cursor_text") => p.term_cursor_text.unwrap_or(p.term.bg),
            ("terminal", "selection") => p.term_selection.unwrap_or(p.focus),
            ("terminal", "selected_text") => p.term_selected_text.unwrap_or(p.term.fg),
            ("terminal", "cursor_guide") => p.term_cursor_guide.unwrap_or(p.term.cursor),
            ("terminal", "bold") => p.term_bold.unwrap_or(p.term.fg),
            ("terminal", "link") => p.term_link.unwrap_or(p.term.fg),
            ("terminal", "underline") => p.term_underline.unwrap_or(p.term.fg),
            ("ansi", key) => {
                let i = ANSI_NAMES.iter().position(|n| *n == key).unwrap_or(0);
                p.term.ansi()[i].unwrap_or(mooring_core::terminal::builtin_ansi()[i])
            }
            ("status", key) => Status::ALL
                .iter()
                .find(|s| s.key() == key)
                .map_or(p.muted, |s| s.hue()),
            ("seen", "title") => crate::ui::status_colors::seen_title(),
            ("seen", "status") => crate::ui::status_colors::seen_status(),
            ("tone", key) => Tone::ALL
                .iter()
                .find(|t| t.key() == key)
                .map_or(p.muted, |t| t.hue()),
            (_, key) => p.ui(key).unwrap_or(p.muted),
        }
    }
    fn is_set(self, draft: &ThemeFile) -> bool {
        draft.color(self.section, self.key).is_some()
    }
}
pub(super) struct Editor {
    pub(super) draft: ThemeFile,
    /// The theme's own file; a built-in gets a new one on saving.
    over: Option<PathBuf>,
    /// It opened on what the current space wears, so saving changes that space alone.
    for_space: bool,
    /// Where each slider was last drawn, for the native acceptance scene to press.
    slider_bounds: std::rc::Rc<std::cell::Cell<[Option<gpui::Bounds<gpui::Pixels>>; 3]>>,
    tab: Tab,
    picked: Item,
    /// Kept apart from the color so a gray does not forget its hue while dragging.
    hsv: [f32; 3],
    dragging: Option<usize>,
    name: Entity<Field>,
    hex: Entity<Field>,
    _hex_changed: Subscription,
    pub(super) error: Option<String>,
}
impl SettingsView {
    pub(super) fn open_editor(&mut self, cx: &mut Context<Self>) {
        let Some(shown) = appearance::shown(cx) else {
            return;
        };
        let Some(entry) = shown.entry else { return };
        let (draft, over, for_space) = (
            entry.as_file(),
            entry.own_file(),
            shown.source == appearance::Source::Space,
        );
        let picked = Tab::Base.items()[0];
        let start = picked.value(&draft);
        let name = cx.new(|cx| Field::new(t!("主题名称"), &draft.name, cx).compact());
        let hex_field = cx.new(|cx| Field::new(t!("颜色"), &hex(start), cx).compact());
        let watch = cx.observe(&hex_field, |this, field, cx| {
            let typed = parse_hex(&field.read(cx).text);
            let Some(color) = typed else { return };
            let current = this.editor.as_ref().map(|e| e.picked.value(&e.draft));
            if current != Some(color) {
                this.set_color(color, false, cx);
            }
        });
        appearance::preview(Some(&draft), cx);
        self.editor = Some(Editor {
            hsv: to_hsv(start),
            draft,
            over,
            for_space,
            slider_bounds: Default::default(),
            tab: Tab::Base,
            picked,
            dragging: None,
            name,
            hex: hex_field,
            _hex_changed: watch,
            error: None,
        });
        cx.notify();
    }
    /// Leave the editor; the saved theme shows again.
    pub(super) fn close_editor(&mut self, cx: &mut Context<Self>) {
        self.editor = None;
        appearance::preview(None, cx);
        cx.notify();
    }
    /// `from_slider` keeps the hue and saturation the drag is on and refills the hex.
    fn set_color(&mut self, color: u32, from_slider: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let item = editor.picked;
        match (item.section, item.key) {
            ("", "bg") => editor.draft.bg = color,
            ("", "fg") => editor.draft.fg = color,
            (section, key) => editor.draft.set(section, key, Some(color)),
        }
        if from_slider {
            let text = hex(color);
            editor.hex.update(cx, |field, _| field.set_text(&text));
        } else {
            editor.hsv = to_hsv(color);
        }
        appearance::preview(Some(&editor.draft), cx);
        cx.notify();
    }
    /// What the row shows after anything changes which color is being edited.
    fn pick(&mut self, item: Item, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.picked = item;
        let value = item.value(&editor.draft);
        editor.hsv = to_hsv(value);
        editor.dragging = None;
        editor
            .hex
            .update(cx, |field, _| field.set_text(&hex(value)));
        cx.notify();
    }
    fn reset_color(&mut self, item: Item, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.draft.set(item.section, item.key, None);
        appearance::preview(Some(&editor.draft), cx);
        self.pick(item, cx);
    }
    fn slide(&mut self, channel: usize, to: f32, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.hsv[channel] = to.clamp(0., 1.);
        let color = from_hsv(editor.hsv);
        self.set_color(color, true, cx);
    }
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let name = editor.name.read(cx).text.trim().to_owned();
        if name.is_empty() {
            editor.error = Some(t!("给主题起个名字").to_owned());
            cx.notify();
            return;
        }
        editor.draft.name = name;
        let (light, for_space) = (editor.draft.term_colors().light, editor.for_space);
        match themes::save(&editor.draft, editor.over.as_deref()) {
            Ok(id) => {
                self.editor = None;
                appearance::preview(None, cx);
                themes::reload(cx);
                self.text_error = if for_space {
                    crate::ui::space_themes::saved_as(&id, cx).err()
                } else {
                    appearance::set_theme(&id, light, true, cx).err()
                };
                crate::set_menus(cx);
                cx.notify();
            }
            Err(error) => {
                editor.error = Some(error);
                cx.notify();
            }
        }
    }
    fn delete_theme(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.editor.as_ref().and_then(|e| e.over.clone()) else {
            return;
        };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.editor = None;
                appearance::preview(None, cx);
                themes::reload(cx);
                cx.notify();
            }
            Err(error) => {
                if let Some(editor) = self.editor.as_mut() {
                    let path = path.display();
                    editor.error = Some(t!("无法删除 {path}：{error}", path, error));
                }
                cx.notify();
            }
        }
    }
    pub(super) fn editor_page(&self, cx: &Context<Self>) -> gpui::Div {
        let Some(editor) = self.editor.as_ref() else {
            return div();
        };
        let mut tabs = segments();
        for (i, tab) in Tab::ALL.into_iter().enumerate() {
            tabs = tabs.child(
                segment(format!("editor-tab-{i}"), tab.label(), tab == editor.tab).on_click(
                    cx.listener(move |this, _, _, cx| {
                        if let Some(editor) = this.editor.as_mut() {
                            editor.tab = tab;
                        }
                        this.pick(tab.items()[0], cx);
                    }),
                ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(T::S12))
            .child(self.editor_header(editor, cx))
            .child(tabs)
            .when(editor.tab == Tab::Base, |d| {
                d.child(self.material_rows(editor, cx))
            })
            .child(
                div()
                    .flex()
                    .gap(px(T::S16))
                    .child(self.editor_rows(editor, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(T::S12))
                            .child(self.picker(editor, cx))
                            .child(preview_window())
                            .child(super::theme_material::contrast_note(editor)),
                    ),
            )
    }
    fn editor_header(&self, editor: &Editor, cx: &Context<Self>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(T::S6))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(div().flex_1().min_w_0().child(editor.name.clone()))
                    .when(editor.over.is_some(), |d| {
                        d.child(
                            secondary("editor-delete", t!("删除"))
                                .on_click(cx.listener(|this, _, _, cx| this.delete_theme(cx))),
                        )
                    })
                    .child(
                        secondary("editor-cancel", t!("取消"))
                            .on_click(cx.listener(|this, _, _, cx| this.close_editor(cx))),
                    )
                    .child(
                        secondary("editor-save", t!("保存"))
                            .bg(T::color(T::accent()))
                            .text_color(T::color(T::on_accent()))
                            .on_click(cx.listener(|this, _, _, cx| this.save_draft(cx))),
                    ),
            )
            .when_some(editor.error.clone(), |d, error| {
                d.child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(Tone::Danger.ink())
                        .child(error),
                )
            })
            .child(label(if editor.over.is_some() {
                t!("改动马上显示，保存后写回这个主题")
            } else {
                t!("内置主题不会被改：保存时存成一个新主题")
            }))
    }
    fn editor_rows(&self, editor: &Editor, cx: &Context<Self>) -> impl IntoElement {
        let rows = editor.tab.items().into_iter().map(|item| {
            let (on, set) = (item == editor.picked, item.is_set(&editor.draft));
            div()
                .id(format!("editor-row-{}-{}", item.section, item.key))
                .flex()
                .items_center()
                .gap(px(T::S8))
                .px(px(T::S8))
                .py(px(T::S6))
                .rounded(px(T::CONTROL_RADIUS))
                .cursor_pointer()
                .when(on, |d| d.bg(T::color(T::selection())))
                .on_click(cx.listener(move |this, _, _, cx| this.pick(item, cx)))
                .child(
                    div()
                        .size(px(SWATCH))
                        .flex_shrink_0()
                        .rounded(px(T::S4))
                        .border(px(T::BORDER_WIDTH))
                        .border_color(T::color(T::control()))
                        .bg(T::color(item.value(&editor.draft))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(T::primary()))
                        .child(item.label),
                )
                .when(item.section.is_empty(), |d| d.child(label(t!("基础"))))
                .when(!item.section.is_empty() && !set, |d| {
                    d.child(label(t!("推导")))
                })
                .when(set, |d| {
                    d.child(
                        secondary(
                            gpui::SharedString::from(format!(
                                "editor-reset-{}-{}",
                                item.section, item.key
                            )),
                            t!("重置"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.reset_color(item, cx);
                        })),
                    )
                })
        });
        div()
            .id("editor-rows")
            .w(px(ROW_COLUMN))
            .flex_shrink_0()
            .h(px(LIST_HEIGHT))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(T::S2))
            .children(rows)
    }
    fn picker(&self, editor: &Editor, cx: &Context<Self>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(T::S8))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(T::S8))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(T::BODY_SIZE))
                            .text_color(T::color(T::primary()))
                            .child(editor.picked.label),
                    )
                    .child(div().w(px(T::S20 * 4.5)).child(editor.hex.clone())),
            )
            .child(self.slider(0, t!("色相"), editor.hsv, cx))
            .child(self.slider(1, t!("饱和"), editor.hsv, cx))
            .child(self.slider(2, t!("明度"), editor.hsv, cx))
    }
    /// One channel, drawn as the colors dragging it would give. Only exists while the
    /// editor is open.
    fn slider(
        &self,
        channel: usize,
        name: &'static str,
        hsv: [f32; 3],
        cx: &Context<Self>,
    ) -> gpui::Div {
        let view = cx.entity();
        let seen = self.editor.as_ref().map(|e| e.slider_bounds.clone());
        let paint = move |bounds: gpui::Bounds<gpui::Pixels>,
                          (): (),
                          window: &mut Window,
                          _: &mut gpui::App| {
            if let Some(seen) = &seen {
                let mut all = seen.get();
                all[channel] = Some(bounds);
                seen.set(all);
            }
            let step = bounds.size.width / SLIDER_STEPS as f32;
            for i in 0..SLIDER_STEPS {
                let mut at = hsv;
                at[channel] = i as f32 / (SLIDER_STEPS - 1) as f32;
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(
                        gpui::point(bounds.left() + step * i as f32, bounds.top()),
                        gpui::size(step + px(T::BORDER_WIDTH), bounds.size.height),
                    ),
                    T::color(from_hsv(at)),
                ));
            }
            let x = bounds.left() + bounds.size.width * hsv[channel] - px(THUMB / 2.);
            window.paint_quad(gpui::outline(
                gpui::Bounds::new(
                    gpui::point(x, bounds.top()),
                    gpui::size(px(THUMB), bounds.size.height),
                ),
                T::color(T::primary()),
                gpui::BorderStyle::Solid,
            ));
            let at = move |x: gpui::Pixels| (x - bounds.left()) / bounds.size.width;
            let (down, moved, up) = (view.clone(), view.clone(), view.clone());
            window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                if phase.bubble() && bounds.contains(&event.position) {
                    down.update(cx, |this, cx| {
                        if let Some(editor) = this.editor.as_mut() {
                            editor.dragging = Some(channel);
                        }
                        this.slide(channel, at(event.position.x), cx);
                    });
                }
            });
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase.bubble() && event.dragging() {
                    moved.update(cx, |this, cx| {
                        if this
                            .editor
                            .as_ref()
                            .is_some_and(|e| e.dragging == Some(channel))
                        {
                            this.slide(channel, at(event.position.x), cx);
                        }
                    });
                }
            });
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase.bubble() {
                    up.update(cx, |this, _| {
                        if let Some(editor) = this.editor.as_mut() {
                            editor.dragging = None;
                        }
                    });
                }
            });
        };
        div()
            .flex()
            .items_center()
            .gap(px(T::S8))
            .child(div().w(px(T::S20 + T::S8)).child(label(name)))
            .child(
                div()
                    .flex_1()
                    .h(px(SLIDER))
                    .rounded(px(T::S4))
                    .overflow_hidden()
                    .child(canvas(|_, _, _| (), paint).size_full()),
            )
    }
}
/// The window behind 设置 is the real preview; this shows its parts side by side.
fn preview_window() -> gpui::Div {
    let p = T::palette();
    let state = |status: Status| {
        div()
            .flex()
            .items_center()
            .gap(px(T::S6))
            .text_size(px(T::SMALL))
            .text_color(T::color(p.sidebar_title))
            .child(div().size(px(T::S8)).rounded_full().bg(status.colors().0))
            .child(status.label())
    };
    let ansi = |i: usize, text: &'static str| {
        let color = p.term.ansi()[i].unwrap_or(mooring_core::terminal::builtin_ansi()[i]);
        div().text_color(T::color(color)).child(text)
    };
    div()
        .h(px(T::S20 * 8.))
        .flex()
        .rounded(px(T::CONTROL_RADIUS))
        .overflow_hidden()
        .border(px(T::BORDER_WIDTH))
        .border_color(T::color(p.card_border))
        .bg(T::color(p.sidebar_bg))
        .child(
            div()
                .w(px(T::S20 * 6.))
                .flex_shrink_0()
                .p(px(T::S10))
                .flex()
                .flex_col()
                .gap(px(T::S8))
                .child(state(Status::Waiting))
                .child(state(Status::Review))
                .child(state(Status::Running))
                .child(
                    div()
                        .text_size(px(T::SMALL))
                        .text_color(T::color(p.sidebar_third))
                        .child(t!("弱文字")),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .m(px(T::S4))
                .ml_0()
                .p(px(T::S8))
                .flex()
                .flex_col()
                .gap(px(T::S2))
                .rounded(px(T::S4))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(p.border))
                .bg(T::color(p.term.bg))
                .font_family("Menlo")
                .text_size(px(T::SMALL))
                .text_color(T::color(p.term.fg))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(T::color(p.term_bold.unwrap_or(p.term.fg)))
                        .child("$ cargo test"),
                )
                .child(ansi(2, "ok 214 passed"))
                .child(ansi(1, "1 failed"))
                .child(ansi(3, "2 warnings"))
                .child(ansi(4, "src/theme.rs"))
                .child(
                    div()
                        .flex()
                        .child("$ ")
                        .child(div().w(px(T::S8)).h(px(T::S14)).bg(T::color(p.term.cursor))),
                ),
        )
}
#[cfg(feature = "visual-test")]
impl Workbench {
    fn visual_editor(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut SettingsView, &mut Context<SettingsView>),
    ) {
        let view = self.settings.clone().expect("settings open");
        view.update(cx, f);
    }
    pub(crate) fn visual_editor_open(&mut self, cx: &mut Context<Self>) {
        self.visual_editor(cx, |v, cx| v.open_editor(cx));
    }
    pub(crate) fn visual_editor_tab(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.visual_editor(cx, |v, cx| {
            let tab = Tab::ALL[tab];
            v.editor.as_mut().unwrap().tab = tab;
            v.pick(tab.items()[0], cx);
        });
    }
    /// Pick a row of the open tab by key, then give it a color.
    pub(crate) fn visual_editor_set(&mut self, key: &str, color: u32, cx: &mut Context<Self>) {
        self.visual_editor(cx, |v, cx| {
            let tab = v.editor.as_ref().unwrap().tab;
            let item = tab.items().into_iter().find(|i| i.key == key).unwrap();
            v.pick(item, cx);
            v.set_color(color, false, cx);
        });
    }
    pub(crate) fn visual_editor_reset(&mut self, key: &str, cx: &mut Context<Self>) {
        self.visual_editor(cx, |v, cx| {
            let tab = v.editor.as_ref().unwrap().tab;
            let item = tab.items().into_iter().find(|i| i.key == key).unwrap();
            v.reset_color(item, cx);
        });
    }
    pub(crate) fn visual_editor_hsv(&self, cx: &gpui::App) -> [f32; 3] {
        self.settings
            .as_ref()
            .unwrap()
            .read(cx)
            .editor
            .as_ref()
            .unwrap()
            .hsv
    }
    pub(crate) fn visual_editor_cancel(&mut self, cx: &mut Context<Self>) {
        self.visual_editor(cx, |v, cx| v.close_editor(cx));
    }
    pub(crate) fn visual_editor_save(&mut self, name: &str, cx: &mut Context<Self>) {
        let name = name.to_owned();
        self.visual_editor(cx, |v, cx| {
            let field = v.editor.as_ref().unwrap().name.clone();
            field.update(cx, |f, _| f.set_text(&name));
            v.save_draft(cx);
        });
    }
    pub(crate) fn visual_editor_delete(&mut self, cx: &mut Context<Self>) {
        self.visual_editor(cx, |v, cx| v.delete_theme(cx));
    }
}
/// Hue, saturation and value, each 0-1.
fn to_hsv(color: u32) -> [f32; 3] {
    let channel = |shift: u32| ((color >> shift) & 255) as f32 / 255.;
    let (r, g, b) = (channel(16), channel(8), channel(0));
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let hue = if delta == 0. {
        0.
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        (b - r) / delta + 2.
    } else {
        (r - g) / delta + 4.
    };
    [hue / 6., if max == 0. { 0. } else { delta / max }, max]
}
fn from_hsv([h, s, v]: [f32; 3]) -> u32 {
    let sector = (h.clamp(0., 1.) * 6.).min(5.999_999);
    let chroma = v * s;
    let middle = chroma * (1. - (sector.rem_euclid(2.) - 1.).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, middle, 0.),
        1 => (middle, chroma, 0.),
        2 => (0., chroma, middle),
        3 => (0., middle, chroma),
        4 => (middle, 0., chroma),
        _ => (chroma, 0., middle),
    };
    let byte = |c: f32| (((c + v - chroma) * 255.).round() as u32).min(255);
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

impl SettingsView {
    /// Where to press a slider for `to` of its range.
    #[cfg(feature = "visual-test")]
    pub(crate) fn visual_slider_point(
        &self,
        channel: usize,
        to: f32,
    ) -> Option<gpui::Point<gpui::Pixels>> {
        let bounds = self.editor.as_ref()?.slider_bounds.get()[channel]?;
        Some(gpui::point(
            bounds.left() + bounds.size.width * to,
            bounds.center().y,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_survive_the_trip_through_hsv() {
        for text in [
            "#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#0f1c2e", "#e8732c",
        ] {
            let color = parse_hex(text).unwrap();
            assert_eq!(from_hsv(to_hsv(color)), color, "{text}");
        }
    }
    #[test]
    fn every_tab_lists_something_and_every_token_is_on_a_tab() {
        let count: usize = Tab::ALL.iter().map(|t| t.items().len()).sum();
        assert!(Tab::ALL.iter().all(|t| !t.items().is_empty()));
        assert_eq!(
            count,
            2 + mooring_core::theme::TERMINAL_NAMES.len()
                + ANSI_NAMES.len()
                + UI_TOKENS.len()
                + Status::ALL.len()
                + Tone::ALL.len()
                + mooring_core::theme::SEEN_NAMES.len()
        );
    }
}
