//! 主题编辑器's material: how see-through the window is, and a picture behind terminals.
use super::theme_editor::Editor;
use super::*;
use crate::ui::appearance;
use crate::ui::backdrop::Backdrop;
use crate::ui::themes;
use mooring_core::theme::{BackgroundImage, DEFAULT_IMAGE_STRENGTH, ImageMode};

const OPACITIES: [(u8, &str); 5] = [
    (100, "不透明"),
    (95, "95%"),
    (90, "90%"),
    (80, "80%"),
    (65, "65%"),
];
const STRENGTHS: [(u8, &str); 5] = [
    (10, "10%"),
    (20, "20%"),
    (35, "35%"),
    (50, "50%"),
    (80, "80%"),
];
fn mode_label(mode: ImageMode) -> &'static str {
    match mode {
        ImageMode::Fill => "铺满",
        ImageMode::Fit => "完整显示",
        ImageMode::Stretch => "拉伸",
        ImageMode::Tile => "平铺",
    }
}
impl SettingsView {
    fn change_material(
        &mut self,
        change: impl FnOnce(&mut mooring_core::theme::Material),
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        change(&mut editor.draft.material);
        appearance::preview(Some(&editor.draft), cx);
        cx.notify();
    }
    /// The picked picture goes into the themes folder and the theme names it there, so the
    /// folder alone carries a theme to another machine.
    fn pick_picture(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("用作背景".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.first() else { return };
            let path = path.clone();
            let copied = cx
                .background_executor()
                .spawn(async move { themes::keep_picture(&path) })
                .await;
            let _ = this.update(cx, |this, cx| match copied {
                Ok(path) => this.change_material(
                    |material| {
                        material.image = Some(BackgroundImage {
                            path,
                            mode: ImageMode::default(),
                            strength: DEFAULT_IMAGE_STRENGTH,
                        });
                    },
                    cx,
                ),
                Err(error) => {
                    if let Some(editor) = this.editor.as_mut() {
                        editor.error = Some(error);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn material_rows(&self, editor: &Editor, cx: &Context<Self>) -> gpui::Div {
        let material = &editor.draft.material;
        let mut opacity = segments();
        for (value, name) in OPACITIES {
            opacity = opacity.child(
                segment(format!("opacity-{value}"), name, material.opacity == value).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.change_material(|m| m.opacity = value, cx);
                    }),
                ),
            );
        }
        let mut rows = div()
            .flex()
            .flex_col()
            .rounded(px(T::CARD_RADIUS))
            .border(px(T::BORDER_WIDTH))
            .border_color(T::color(T::card_border()))
            .child(self.row(
                true,
                "窗口不透明度",
                "终端文字始终不透明，只有窗口的底变透；程序自己涂的底色不变",
                opacity,
            ));
        if material.opacity != 100 {
            let blur = material.blur;
            let mut choices = segments();
            for (value, name) in [(false, "不模糊"), (true, "模糊")] {
                choices = choices.child(
                    segment(format!("blur-{value}"), name, blur == value).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.change_material(|m| m.blur = value, cx);
                        },
                    )),
                );
            }
            rows = rows.child(self.row(false, "背后模糊", "透过窗口看到的内容是否模糊", choices));
        }
        let mut actions = div().flex().items_center().gap(px(T::S8)).child(
            secondary(
                "picture-pick",
                if material.image.is_some() {
                    "换一张"
                } else {
                    "选择图片"
                },
            )
            .on_click(cx.listener(|this, _, _, cx| this.pick_picture(cx))),
        );
        if material.image.is_some() {
            actions = actions.child(secondary("picture-remove", "移除").on_click(
                cx.listener(|this, _, _, cx| this.change_material(|m| m.image = None, cx)),
            ));
        }
        let note = material
            .image
            .as_ref()
            .and_then(|image| Backdrop::of(image).picture.error())
            .or_else(|| material.image.as_ref().map(|image| image.path.clone()))
            .unwrap_or_else(|| "PNG、JPEG 或 WebP，放在每个终端后面".to_owned());
        rows = rows.child(self.row(false, "背景图", &note, actions));
        if let Some(image) = &material.image {
            let (mode, strength) = (image.mode, image.strength);
            let mut modes = segments();
            for (i, value) in ImageMode::ALL.into_iter().enumerate() {
                modes = modes.child(
                    segment(format!("image-mode-{i}"), mode_label(value), mode == value).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.change_material(
                                |m| {
                                    if let Some(image) = m.image.as_mut() {
                                        image.mode = value;
                                    }
                                },
                                cx,
                            );
                        }),
                    ),
                );
            }
            rows = rows.child(self.row(false, "缩放", "图片怎样填满终端", modes));
            let mut strengths = segments();
            for (value, name) in STRENGTHS {
                strengths = strengths.child(
                    segment(format!("image-strength-{value}"), name, strength == value).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.change_material(
                                |m| {
                                    if let Some(image) = m.image.as_mut() {
                                        image.strength = value;
                                    }
                                },
                                cx,
                            );
                        }),
                    ),
                );
            }
            rows = rows.child(self.row(false, "图片强度", "越高图片越明显，文字越难读", strengths));
        }
        rows
    }
}
#[cfg(feature = "visual-test")]
impl Workbench {
    pub(crate) fn visual_slider_point(
        &self,
        channel: usize,
        to: f32,
        cx: &gpui::App,
    ) -> Option<gpui::Point<gpui::Pixels>> {
        let view = self.settings.as_ref()?.read(cx);
        view.visual_slider_point(channel, to)
    }
}
