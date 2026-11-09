//! Isolated native settings acceptance. Uses GPUI event dispatch and real PNG output.
use super::*;

#[path = "settings_evidence/faces.rs"]
mod faces;
#[path = "settings_evidence/icons.rs"]
mod icons;
#[path = "settings_evidence/plugin_page.rs"]
mod plugin_page;

pub(super) fn start(
    handle: Option<WindowHandle<workbench::Workbench>>,
    evidence: Option<PathBuf>,
    cx: &mut App,
) -> bool {
    let Ok(phase) = std::env::var("MOORING_SETTINGS_EVIDENCE") else {
        return false;
    };
    let (Some(handle), Some(directory)) = (handle, evidence) else {
        return false;
    };
    assert!(
        std::env::var_os("XDG_CONFIG_HOME").is_some(),
        "scratch config required"
    );
    cx.spawn(async move |cx| {
        let executor = cx.background_executor().clone();
        let wait = move |ms| executor.timer(Duration::from_millis(ms));
        wait(2000).await;
        std::fs::create_dir_all(&directory).unwrap();
        handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
        wait(300).await;
        let shot = |name: &str, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh(); w.draw(a).clear(a);
                w.render_to_image().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
            }).unwrap();
        };
        // Coordinates are relative to the visible settings panel, in logical pixels.
        let click = |x: f32, y: f32, cx: &mut gpui::AsyncApp| {
            cx.update_window(handle.into(), |_, w, a| {
                let origin = gpui::point((w.viewport_size().width - px(theme::Theme::SETTINGS_WIDTH)) / 2., (w.viewport_size().height - px(664.)) / 2.);
                native_click(w, a, origin + gpui::point(px(x), px(y)), gpui::MouseButton::Left);
            }).unwrap();
        };
        // The nav's rows, by the page's place: adding a page must not move what a click means.
        let nav = |page: &str, cx: &mut gpui::AsyncApp| {
            let pages = workbench::Workbench::visual_settings_pages();
            let shown = mooring_core::i18n::say(page);
            let row = pages.iter().position(|p| *p == page || *p == shown).expect("a settings page") as f32;
            click(60., 119. + 34. * row, cx);
        };
        let values = |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_settings_values()).unwrap();
        if phase == "icons" {
            icons::run(handle, &directory, cx).await;
            return;
        }
        // Every page in light and dark, for looking at the layout; changes nothing saved.
        if phase == "tour" {
            for (scheme, look) in [("light", crate::ui::appearance::Mode::Light), ("dark", crate::ui::appearance::Mode::Dark)] {
                cx.update(|a| crate::ui::appearance::set_mode(look, a).unwrap());
                for (i, page) in workbench::Workbench::visual_settings_pages().into_iter().enumerate() {
                    handle.update(cx, |v, _, a| v.visual_settings_show(page, a)).unwrap();
                    wait(300).await;
                    shot(&format!("tour-{scheme}-{i}"), cx);
                }
            }
            eprintln!("settings tour PASS");
            cx.update(|a| a.quit());
            return;
        }
        // Every fixed appearance: its 外观 page, then the workbench behind it.
        if phase == "themes" {
            let themes: Vec<_> = cx.update(|a| a.global::<crate::ui::themes::Themes>().entries.iter().map(|e| (e.id.clone(), e.name.clone(), e.light())).collect());
            for (id, name, light) in themes {
                cx.update(|a| crate::ui::appearance::set_theme(&id, light, true, a).unwrap());
                if !handle.update(cx, |v, _, _| v.visual_settings_open()).unwrap() {
                    handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
                }
                handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
                wait(300).await;
                shot(&format!("themes-{name}-look"), cx);
                handle.update(cx, |v, _, a| v.visual_settings_close(a)).unwrap();
                wait(300).await;
                shot(&format!("themes-{name}-workbench"), cx);
            }
            eprintln!("settings themes PASS");
            cx.update(|a| a.quit());
            return;
        }
        // 主题: the two groups, the editor end to end, and files dropped into the folder.
        // Everything 设置 › 终端 added past the type: each one saved, and seen in a real pane.
        if phase == "details" {
            use crate::ui::terminal_text::{self, CursorDefault};
            use crate::terminal::screen::CursorShape;
            let set = |change: &dyn Fn(&mut terminal_text::TerminalText)| {
                let mut text = terminal_text::current();
                change(&mut text);
                terminal_text::update(text).unwrap();
            };
            cx.update_window(handle.into(), |_, w, a| { w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a); }).unwrap();
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            pane.update(cx, |p, a| p.visual_type("clear; printf 'plain text => != === -> 0O\\n\\033[2mfaint words\\033[0m\\n\\033[38;5;236mlowcontrast\\033[0m on dark\\n中文输入与等宽文字 汉字\\n'\r", a));
            wait(800).await;
            let redraw = |cx: &mut gpui::AsyncApp| {
                cx.update_window(handle.into(), |_, w, a| {
                    w.refresh();
                    w.draw(a).clear(a);
                })
                .unwrap();
            };
            shot("details-default", cx);
            // A program's own shape wins, and a reset goes back to the setting.
            set(&|t| t.cursor_shape = CursorDefault::Bar); redraw(cx); wait(400).await;
            let shape = |cx: &mut gpui::AsyncApp, pane: &gpui::Entity<crate::terminal::pane::Pane>| pane.read_with(cx, |p, _| p.visual_cursor_shape());
            assert_eq!(shape(cx, &pane), CursorShape::Bar, "default takes hold in a running terminal");
            shot("details-cursor-bar", cx);
            pane.update(cx, |p, a| p.visual_type("printf '\\033[2 q'\r", a)); wait(500).await; redraw(cx);
            assert_eq!(shape(cx, &pane), CursorShape::Block, "the program asked for a block");
            pane.update(cx, |p, a| p.visual_type("printf '\\033[0 q'\r", a)); wait(500).await; redraw(cx);
            assert_eq!(shape(cx, &pane), CursorShape::Bar, "a reset returns to the setting");
            set(&|t| t.cursor_shape = CursorDefault::Underline); redraw(cx); wait(400).await;
            assert_eq!(shape(cx, &pane), CursorShape::Underline);
            set(&|t| { t.cursor_shape = CursorDefault::Block; t.cursor_guide = true; }); redraw(cx); wait(400).await;
            shot("details-guide", cx);
            set(&|t| t.cursor_guide = false);
            // The contrast floor reaches the text and comes off again.
            let low = pane.read_with(cx, |p, _| p.visual_run_fg("lowcontrast")).unwrap();
            assert_eq!(low, 0x303030);
            set(&|t| t.min_contrast = 7.); redraw(cx); wait(400).await;
            let raised = pane.read_with(cx, |p, _| p.visual_run_fg("lowcontrast")).unwrap();
            assert_ne!(raised, low);
            shot("details-contrast", cx);
            set(&|t| t.min_contrast = 1.); redraw(cx); wait(400).await;
            assert_eq!(pane.read_with(cx, |p, _| p.visual_run_fg("lowcontrast")).unwrap(), low);
            set(&|t| t.faint = 0.9); redraw(cx); wait(300).await;
            shot("details-faint", cx);
            set(&|t| { t.faint = 0.5; t.ligatures = true; }); redraw(cx); wait(400).await;
            shot("details-ligatures", cx);
            set(&|t| { t.ligatures = false; t.non_ascii_family = "Kaiti SC".into(); }); redraw(cx); wait(400).await;
            shot("details-non-ascii", cx);
            set(&|t| t.non_ascii_family = String::new());
            let before = pane.read_with(cx, |p, _| p.visual_bounds()).unwrap();
            set(&|t| t.padding = 16.); redraw(cx); wait(600).await;
            let after = pane.read_with(cx, |p, _| p.visual_bounds()).unwrap();
            assert_eq!(f32::from(after.origin.x - before.origin.x), 16., "cells start past the margin");
            assert_eq!(f32::from(before.size.width - after.size.width), 32.);
            shot("details-padding", cx);
            set(&|t| t.padding = 0.); redraw(cx); wait(400).await;
            // A theme's selected-text color, on ASCII and on Chinese.
            let dir = crate::ui::themes::dir().unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("sel.json"), r##"{"bg":"#101820","fg":"#d0d8e0","terminal":{"selected_text":"#ffcc00","selection":"#205080","cursor_guide":"#00ff88"}}"##).unwrap();
            cx.update(crate::ui::themes::reload);
            cx.update(|a| crate::ui::appearance::set_theme("file:sel", false, true, a).unwrap());
            wait(400).await;
            let from = pane.read_with(cx, |p, _| p.visual_cell(0, 0)).unwrap();
            let to = pane.read_with(cx, |p, _| p.visual_cell(3, 12)).unwrap();
            cx.update_window(handle.into(), |_, w, a| {
                use gpui::InputEvent;
                w.dispatch_event(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position: from, modifiers: Default::default(), click_count: 1, first_mouse: false }.to_platform_input(), a);
                w.dispatch_event(gpui::MouseMoveEvent { position: to, pressed_button: Some(gpui::MouseButton::Left), modifiers: Default::default() }.to_platform_input(), a);
                w.dispatch_event(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position: to, modifiers: Default::default(), click_count: 1 }.to_platform_input(), a);
            }).unwrap();
            wait(400).await;
            shot("details-selected-text", cx);
            set(&|t| t.cursor_guide = true); redraw(cx); wait(300).await;
            shot("details-theme-guide", cx);
            // The page itself.
            set(&|t| t.cursor_guide = false);
            handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_settings_show("终端", a)).unwrap();
            wait(400).await;
            shot("details-page", cx);
            // Each new row, found by the search that also lists it.
            let query = handle.update(cx, |v, _, a| v.visual_settings_query(a)).unwrap();
            for (name, words) in [("cursor", "光标"), ("reading", "对比度"), ("faces", "连字"), ("chinese", "中文"), ("margin", "边距")] {
                query.update(cx, |f, cx| { f.set_text(words); cx.notify(); });
                wait(300).await;
                shot(&format!("details-row-{name}"), cx);
            }
            eprintln!("settings details PASS");
            cx.update(|a| a.quit());
            return;
        }
        // A theme's material: see-through, blurred, a picture in each mode, then back to none.
        if phase == "material" {
            use crate::ui::{appearance, theme::Theme as T, themes};
            use gpui::WindowBackgroundAppearance as W;
            let dir = themes::dir().unwrap();
            std::fs::create_dir_all(dir.join("images")).unwrap();
            let picture = image::RgbaImage::from_fn(600, 400, |x, y| {
                let edge = x < 12 || y < 12 || x >= 588 || y >= 388;
                if edge { image::Rgba([255, 200, 0, 255]) } else { image::Rgba([(x * 255 / 600) as u8, (y * 255 / 400) as u8, 150, 255]) }
            });
            picture.save(dir.join("images/grad.png")).unwrap();
            cx.update_window(handle.into(), |_, w, a| { w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a); }).unwrap();
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            pane.update(cx, |p, a| p.visual_type("clear; printf 'over the picture\\n\\033[41m red cells keep their color \\033[0m\\n中文输入与等宽文字\\n'\r", a));
            wait(800).await;
            let theme = |name: &str, body: &str, cx: &mut gpui::AsyncApp| {
                std::fs::write(dir.join(format!("{name}.json")), format!(r##"{{"bg":"#0f1c2e","fg":"#d5dfef",{body}}}"##)).unwrap();
                cx.update(themes::reload);
                cx.update(|a| appearance::set_theme(&format!("file:{name}"), false, true, a).unwrap());
            };
            let backing = |cx: &mut gpui::AsyncApp| cx.update(|_| crate::ui::backdrop::window_background());
            theme("glass", r##""window":{"opacity":0.7}"##, cx); wait(400).await;
            assert_eq!(backing(cx), W::Transparent);
            assert_eq!(T::palette().opacity, 70);
            shot("material-transparent", cx);
            theme("frost", r##""window":{"opacity":0.7,"blur":true}"##, cx); wait(400).await;
            assert_eq!(backing(cx), W::Blurred);
            shot("material-blur", cx);
            for mode in ["fill", "fit", "stretch", "tile"] {
                theme(&format!("pic-{mode}"), &format!(r##""background":{{"image":"images/grad.png","mode":"{mode}","strength":0.5}}"##), cx);
                wait(700).await;
                assert_eq!(backing(cx), W::Opaque);
                assert!(T::palette().backdrop.unwrap().picture.image().is_some(), "{mode}: decoded");
                shot(&format!("material-{mode}"), cx);
            }
            // A file replaced under its name shows, and old pictures are let go.
            let first = T::palette().backdrop.unwrap().picture;
            image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 255, 255, 255])).save(dir.join("images/grad.png")).unwrap();
            cx.update(themes::reload);
            wait(700).await;
            let second = T::palette().backdrop.unwrap().picture;
            assert!(!std::ptr::eq(first, second), "the replaced file is a new picture");
            assert!(second.image().is_some());
            shot("material-replaced", cx);
            image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 0, 255, 255])).save(dir.join("images/third.png")).unwrap();
            theme("third", r##""background":{"image":"images/third.png","mode":"tile"}"##, cx); wait(700).await;
            image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 0, 255])).save(dir.join("images/fourth.png")).unwrap();
            theme("fourth", r##""background":{"image":"images/fourth.png","mode":"tile"}"##, cx); wait(700).await;
            assert!(second.image().is_none(), "only the two latest stay decoded");
            std::fs::write(dir.join("images/bad.png"), "not a picture").unwrap();
            theme("broken", r##""background":{"image":"images/bad.png"}"##, cx); wait(500).await;
            assert!(T::palette().backdrop.unwrap().picture.error().is_some());
            shot("material-broken", cx);
            theme("gone", r##""background":{"image":"images/nowhere.png"}"##, cx); wait(300).await;
            assert!(cx.update(|a| a.global::<themes::Themes>().problems.iter().any(|(_, why)| why.contains("找不到背景图"))));
            cx.update(|a| appearance::set_theme("dark", false, true, a).unwrap()); wait(300).await;
            assert_eq!(backing(cx), W::Opaque);
            // The editor's rows, on a theme with a picture.
            theme("pic-edit", r##""window":{"opacity":0.8},"background":{"image":"images/grad.png"}"##, cx); wait(500).await;
            handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_editor_open(a)).unwrap();
            wait(400).await;
            shot("material-editor", cx);
            handle.update(cx, |v, _, a| v.visual_editor_cancel(a)).unwrap();
            eprintln!("settings material PASS");
            cx.update(|a| a.quit());
            return;
        }
        // Sharing and spaces: themes from a plugin, and a theme per space.
        if phase == "sharing" {
            use crate::ui::{appearance, space_themes, theme::Theme as T, themes::{self, Themes}};
            use mooring_core::plugin;
            let config = crate::ui::status_colors::config_dir().unwrap();
            cx.update_window(handle.into(), |_, w, a| { w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a); }).unwrap();
            // A plugin's themes come and go with it, and 调整 never writes into it.
            let dir = config.join("plugins/tide-themes");
            std::fs::create_dir_all(dir.join("themes")).unwrap();
            std::fs::write(dir.join("plugin.json"), r#"{"api":1,"id":"tide-themes","name":"潮汐主题","provides":["themes"]}"#).unwrap();
            std::fs::write(dir.join("themes/tide.json"), r##"{"name":"潮汐","bg":"#0b2530","fg":"#d8ecef","ui":{"sidebar":"#071b23"}}"##).unwrap();
            cx.update(|a| { plugin::reload(); themes::reload(a); });
            let id = "plugin:tide-themes/tide";
            let entry = cx.update(|a| a.global::<Themes>().get(id).cloned()).expect("the plugin's theme is offered");
            assert!(entry.read_only && entry.own_file().is_none());
            assert_eq!(entry.as_file().name, "潮汐 副本");
            cx.update(|a| appearance::set_theme(id, false, true, a).unwrap());
            wait(300).await;
            assert_eq!(T::term().bg, 0x0b2530);
            shot("sharing-plugin-theme", cx);
            handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
            wait(300).await;
            shot("sharing-look-page", cx);
            cx.update_window(handle.into(), |_, w, a| { w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a); }).unwrap();
            std::fs::remove_dir_all(&dir).unwrap();
            cx.update(|a| { plugin::reload(); themes::reload(a); });
            wait(300).await;
            assert!(cx.update(|a| a.global::<Themes>().get(id).is_none()));
            assert_eq!(T::term().bg, crate::ui::theme::Scheme::Dark.bg(), "uninstalled: back to the side's default");
            // One space wears a theme; the other follows 外观; the window changes on switching.
            handle.update(cx, |v, _, a| v.visual_add_space("other", "", a)).unwrap();
            wait(600).await;
            let spaces = handle.update(cx, |v, _, _| v.visual_spaces()).unwrap();
            eprintln!("spaces: {spaces:?}");
            let first = "default".to_owned();
            cx.update(|a| space_themes::set("other", Some("pine"), a).unwrap());
            handle.update(cx, |v, _, a| v.visual_pick_space("other", a)).unwrap();
            wait(300).await; shot("sharing-space-other", cx); wait(300).await;
            assert_eq!(T::scheme(), crate::ui::theme::Scheme::Pine, "the space's theme");
            shot("sharing-space-other", cx);
            handle.update(cx, |v, _, a| v.visual_pick_space(&first, a)).unwrap();
            wait(300).await; shot("sharing-space-first", cx); wait(300).await;
            assert_eq!(T::scheme(), crate::ui::theme::Scheme::Dark, "a space without one follows 外观");
            shot("sharing-space-first", cx);
            let saved = std::fs::read_to_string(config.join("space-themes.json")).unwrap();
            assert!(saved.contains("pine"), "{saved}");
            // Under a space's theme: 外观 says why choosing changes nothing here, the editor
            // starts from what shows, and saving a copy changes this space alone.
            handle.update(cx, |v, _, a| v.visual_pick_space("other", a)).unwrap();
            wait(300).await; shot("sharing-space-other", cx); wait(300).await;
            let chosen_before = cx.update(|a| a.global::<appearance::Appearance>().clone());
            handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
            wait(300).await;
            shot("sharing-space-notice", cx);
            assert_eq!(cx.update(|a| appearance::shown(a).unwrap().source), appearance::Source::Space);
            cx.update(|a| appearance::set_theme("slate", false, true, a).unwrap());
            wait(200).await;
            assert_eq!(T::scheme(), crate::ui::theme::Scheme::Pine, "外观's choice waits for other spaces");
            cx.update(|a| appearance::set_theme(&chosen_before.dark, false, true, a).unwrap());
            handle.update(cx, |v, _, a| v.visual_editor_open(a)).unwrap();
            wait(300).await;
            assert_eq!((T::term().bg, T::term().fg), (crate::ui::theme::Scheme::Pine.bg(), crate::ui::theme::Scheme::Pine.fg()));
            handle.update(cx, |v, _, a| v.visual_editor_set("bg", 0x10302a, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_editor_save("深松", a)).unwrap();
            wait(400).await;
            assert_eq!(T::term().bg, 0x10302a, "the space wears the saved copy");
            let after = cx.update(|a| a.global::<appearance::Appearance>().clone());
            assert_eq!(after, chosen_before, "saving under a space leaves 外观 alone");
            let saved = std::fs::read_to_string(config.join("space-themes.json")).unwrap();
            assert!(saved.contains("file:"), "{saved}");
            handle.update(cx, |v, _, a| v.visual_pick_space(&first, a)).unwrap();
            wait(300).await; shot("sharing-space-first", cx); wait(300).await;
            assert_eq!(T::scheme(), crate::ui::theme::Scheme::Dark, "other spaces did not change");
            cx.update_window(handle.into(), |_, w, a| { w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a); }).unwrap();
            // The menu, folded and open, and picking from it with a real click.
            handle.update(cx, |v, _, a| v.visual_open_space_menu("other", a)).unwrap();
            wait(200).await; shot("sharing-menu", cx);
            let row = handle.update(cx, |v, _, _| v.visual_space_menu_point(3)).unwrap();
            cx.update_window(handle.into(), |_, w, a| native_click(w, a, row, gpui::MouseButton::Left)).unwrap();
            wait(200).await; shot("sharing-menu-open", cx);
            let graphite = handle.update(cx, |v, _, _| v.visual_space_menu_point(4 + 1 + 2)).unwrap();
            cx.update_window(handle.into(), |_, w, a| native_click(w, a, graphite, gpui::MouseButton::Left)).unwrap();
            wait(300).await; shot("sharing-picked", cx);
            eprintln!("picked: {:?}", std::fs::read_to_string(config.join("space-themes.json")).unwrap());
            cx.update(|a| space_themes::set("other", None, a).unwrap());
            eprintln!("settings sharing PASS");
            cx.update(|a| a.quit());
            return;
        }
        // A colors.json from before themes: its colors move in once, and still show.
        if phase == "migrate" {
            use crate::ui::{status_colors, theme::{StatusStyle, Status}, tone::Tone};
            let config = status_colors::config_dir().unwrap();
            assert!(!config.join("colors.json").exists(), "moved at start");
            assert!(config.join("colors.json.migrated").is_file(), "kept aside");
            assert_eq!(Status::Running.hue(), 0x112233);
            assert_eq!(Tone::Ok.hue(), 0x223344);
            assert_eq!(status_colors::seen_title(), 0x445566);
            assert_eq!(status_colors::seen_dot(), status_colors::SeenDot::Fade);
            let chosen = cx.update(|a| a.global::<crate::ui::appearance::Appearance>().clone());
            assert!(chosen.dark.starts_with("file:"), "{chosen:?}");
            handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
            wait(300).await;
            shot("migrated", cx);
            eprintln!("settings migrate PASS");
            cx.update(|a| a.quit());
            return;
        }
        if phase == "editor" {
            use crate::ui::{appearance::{self, Mode}, theme::Theme as T, themes::{self, Themes}};
            let term = || T::term();
            handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
            wait(300).await;
            shot("theme-dark-mode", cx);
            cx.update(|a| appearance::set_mode(Mode::Light, a).unwrap());
            wait(300).await;
            assert_eq!(term().bg, 0xffffff);
            shot("theme-light-mode", cx);
            cx.update(|a| appearance::set_mode(Mode::System, a).unwrap());
            wait(300).await;
            shot("theme-system-mode", cx);
            cx.update(|a| appearance::set_mode(Mode::Dark, a).unwrap());
            wait(300).await;
            // A draft shows everywhere and goes away with 取消.
            handle.update(cx, |v, _, a| v.visual_editor_open(a)).unwrap();
            wait(300).await;
            shot("editor-base", cx);
            handle.update(cx, |v, _, a| v.visual_editor_set("bg", 0x0f1c2e, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_editor_set("fg", 0xd5dfef, a)).unwrap();
            wait(300).await;
            assert_eq!((term().bg, term().fg), (0x0f1c2e, 0xd5dfef));
            assert_eq!(T::sidebar(), crate::ui::palette::Palette::new(term(), false).sidebar);
            shot("editor-draft", cx);
            handle.update(cx, |v, _, a| v.visual_editor_cancel(a)).unwrap();
            wait(300).await;
            assert_eq!(term().bg, 0x0e0e11, "取消 puts the saved theme back");
            // Closing 设置 mid-edit does the same.
            handle.update(cx, |v, _, a| v.visual_editor_open(a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_editor_set("bg", 0x123456, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_settings_close(a)).unwrap();
            // The last frame's listeners hold the page until another is drawn.
            shot("editor-closed", cx);
            wait(300).await;
            shot("editor-closed", cx);
            assert_eq!(term().bg, 0x0e0e11, "closing 设置 puts the saved theme back");
            handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
            wait(300).await;
            // A theme from every tab, saved over a copy of the built-in.
            handle.update(cx, |v, _, a| {
                v.visual_editor_open(a);
                v.visual_editor_set("bg", 0x0f1c2e, a);
                v.visual_editor_set("fg", 0xd5dfef, a);
                v.visual_editor_tab(1, a);
                v.visual_editor_set("cursor", 0xff00aa, a);
                v.visual_editor_set("selection", 0x00ff88, a);
                v.visual_editor_set("bold", 0xffcc00, a);
                v.visual_editor_set("underline", 0x00e5ff, a);
                v.visual_editor_tab(2, a);
                v.visual_editor_set("red", 0xff4040, a);
                v.visual_editor_tab(3, a);
                v.visual_editor_set("accent", 0x7cc4ff, a);
                v.visual_editor_set("border", 0x3a5a8c, a);
                v.visual_editor_reset("border", a);
                v.visual_editor_tab(4, a);
                v.visual_editor_set("sidebar", 0x0b1522, a);
                v.visual_editor_tab(5, a);
                v.visual_editor_set("running", 0xb07cff, a);
            }).unwrap();
            wait(300).await;
            shot("editor-status-tab", cx);
            handle.update(cx, |v, _, a| v.visual_editor_tab(3, a)).unwrap();
            wait(300).await;
            shot("editor-ui-tab", cx);
            // The sliders, with the real mouse; coordinates are the panel's.
            let before = term().bg;
            handle.update(cx, |v, _, a| v.visual_editor_tab(0, a)).unwrap();
            wait(300).await;
            let hsv = handle.update(cx, |v, _, a| v.visual_editor_hsv(a)).unwrap();
            let at = handle.update(cx, |v, _, a| v.visual_slider_point(0, 0.6, a)).unwrap().expect("the editor drew its sliders");
            cx.update_window(handle.into(), |_, w, a| native_click(w, a, at, gpui::MouseButton::Left)).unwrap();
            wait(300).await;
            let moved = handle.update(cx, |v, _, a| v.visual_editor_hsv(a)).unwrap();
            assert_ne!(hsv[0], moved[0], "clicking the hue slider moves the hue");
            assert_ne!(term().bg, before, "and the window follows");
            shot("editor-slid", cx);
            handle.update(cx, |v, _, a| v.visual_editor_set("bg", 0x0f1c2e, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_editor_save("深海", a)).unwrap();
            wait(300).await;
            let dir = themes::dir().unwrap();
            let saved = std::fs::read_to_string(dir.join("深海.json")).unwrap();
            eprintln!("{saved}");
            assert!(saved.contains("#ff00aa") && saved.contains("#b07cff") && !saved.contains("#3a5a8c"));
            assert_eq!(term().bg, 0x0f1c2e);
            assert_eq!(term().cursor, 0xff00aa);
            assert_eq!(term().ansi()[1], Some(0xff4040));
            assert_eq!(T::accent(), 0x7cc4ff);
            shot("theme-saved-look", cx);
            handle.update(cx, |v, _, a| v.visual_settings_close(a)).unwrap();
            wait(600).await;
            shot("theme-saved-workbench", cx);
            // Files dropped in: picked up without a restart, bad ones named.
            std::fs::write(dir.join("day.json"), r##"{"name":"白日","bg":"#fdfbf4","fg":"#26231d"}"##).unwrap();
            std::fs::write(dir.join("oops.json"), r##"{"bg":"#000"}"##).unwrap();
            let preset = dir.parent().unwrap().join("Sea.itermcolors");
            let entry = |k: &str, r: f64, g: f64, b: f64| format!("<key>{k}</key><dict><key>Blue Component</key><real>{b}</real><key>Green Component</key><real>{g}</real><key>Red Component</key><real>{r}</real></dict>");
            std::fs::write(&preset, format!("<plist><dict>{}{}{}</dict></plist>", entry("Background Color", 0.05, 0.1, 0.1), entry("Foreground Color", 0.9, 0.95, 0.9), entry("Ansi 1 Color", 1., 0.3, 0.3))).unwrap();
            themes::save(&themes::import(&preset).unwrap(), None).unwrap();
            wait(4000).await;
            let (names, problems) = cx.update(|a| {
                let t = a.global::<Themes>();
                (t.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>(), t.problems.clone())
            });
            assert!(names.iter().any(|n| n == "白日") && names.iter().any(|n| n == "Sea"), "{names:?}");
            assert_eq!(problems.len(), 1, "{problems:?}");
            cx.update(|a| {
                let t = a.global::<Themes>();
                let id = t.entries.iter().find(|e| e.name == "白日").unwrap().id.clone();
                appearance::set_theme(&id, true, true, a).unwrap();
            });
            wait(300).await;
            assert_eq!(term().bg, 0xfdfbf4);
            handle.update(cx, |v, w, a| v.show_settings(w, a)).unwrap();
            handle.update(cx, |v, _, a| v.visual_settings_show("外观", a)).unwrap();
            wait(300).await;
            shot("theme-file-light", cx);
            // Deleting the theme in use falls back to its side's default.
            handle.update(cx, |v, _, a| { v.visual_editor_open(a); }).unwrap();
            handle.update(cx, |v, _, a| v.visual_editor_delete(a)).unwrap();
            wait(300).await;
            assert_eq!(term().bg, 0xffffff);
            assert!(!dir.join("day.json").exists());
            shot("theme-deleted", cx);
            eprintln!("settings editor PASS");
            cx.update(|a| a.quit());
            return;
        }
        // 状态圆圈转动: the row alone under its search, off then on.
        if phase == "running-spin" {
            for (name, on) in [("running-spin-off", false), ("running-spin-on", true)] {
                cx.update(|a| crate::ui::running_spin::choose(on, a).unwrap());
                handle.update(cx, |v, _, a| {
                    let field = v.visual_settings_query(a);
                    field.update(a, |f, cx| { f.text = "转动".into(); cx.notify(); });
                }).unwrap();
                wait(300).await;
                shot(name, cx);
            }
            let saved = || std::fs::read_to_string(crate::ui::status_colors::config_dir().unwrap().join("running-spin")).unwrap();
            assert_eq!(saved(), "on");
            // A click on the switch writes the file.
            click(879., 178., cx);
            wait(300).await;
            shot("running-spin-clicked", cx);
            assert_eq!(saved(), "off", "the click did not turn it off");
            eprintln!("settings running-spin PASS");
            cx.update(|a| a.quit());
            return;
        }
        // 钉住这一轮的提问: the row alone under its search, default (bar only) then all on.
        if phase == "pinned-ask" {
            for (name, choice) in [
                ("pinned-ask-default", crate::ui::pinned_ask::PinnedAsk::default()),
                ("pinned-ask-all", crate::ui::pinned_ask::PinnedAsk { bar: true, float: true, button: true }),
            ] {
                cx.update(|a| crate::ui::pinned_ask::choose(choice, a).unwrap());
                handle.update(cx, |v, _, a| {
                    let field = v.visual_settings_query(a);
                    field.update(a, |f, cx| { f.text = "钉住".into(); cx.notify(); });
                }).unwrap();
                wait(300).await;
                shot(name, cx);
            }
            let saved = std::fs::read_to_string(crate::ui::status_colors::config_dir().unwrap().join("pinned-ask")).unwrap();
            assert_eq!(saved, "bar,float,button");
            eprintln!("settings pinned-ask PASS");
            cx.update(|a| a.quit());
            return;
        }
        // 插件 scrolled to what is installed: settings rows under their plugin, the files it
        // changes and its last error. The scratch config's plugins are installed beforehand.
        if phase == "plugin-options" {
            use gpui::InputEvent as _;
            for (scheme, look) in [("light", crate::ui::appearance::Mode::Light), ("dark", crate::ui::appearance::Mode::Dark)] {
                cx.update(|a| crate::ui::appearance::set_mode(look, a).unwrap());
                handle.update(cx, |v, _, a| v.visual_settings_show("插件", a)).unwrap();
                wait(300).await;
                cx.update_window(handle.into(), |_, w, a| {
                    let center = gpui::point(w.viewport_size().width / 2. + px(200.), w.viewport_size().height / 2.);
                    w.dispatch_event(gpui::ScrollWheelEvent { position: center, delta: gpui::ScrollDelta::Lines(gpui::point(0., -12.)), modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved }.to_platform_input(), a);
                }).unwrap();
                wait(300).await;
                shot(&format!("plugin-options-{scheme}"), cx);
            }
            eprintln!("settings plugin-options PASS");
            cx.update(|a| a.quit());
            return;
        }
        // A plugin whose teardown fails can still be taken away, and one that does not load
        // can be removed. MOORING_PLUGIN_CLICKS: 卸载 卡住的, 仍然卸载, 移除 broken.
        if phase == "plugin-stuck" {
            use mooring_core::plugin;
            let clicks: Vec<(f32, f32)> = std::env::var("MOORING_PLUGIN_CLICKS").unwrap_or_default()
                .split(';').filter_map(|p| p.split_once(',')).map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap())).collect();
            let at = |i: usize| clicks.get(i).copied();
            handle.update(cx, |v, _, a| v.visual_settings_show("插件", a)).unwrap();
            wait(400).await;
            shot("stuck-start", cx);
            let Some(remove) = at(0) else { eprintln!("settings plugin-stuck SHOTS"); cx.update(|a| a.quit()); return; };
            click(remove.0, remove.1, cx); wait(1000).await;
            assert!(plugin::installed("stuck").is_some(), "a failing teardown keeps it");
            shot("stuck-refused", cx);
            let Some(force) = at(1) else { eprintln!("settings plugin-stuck SHOTS"); cx.update(|a| a.quit()); return; };
            click(force.0, force.1, cx); wait(1000).await;
            assert!(plugin::installed("stuck").is_none(), "仍然卸载 takes it away");
            shot("stuck-forced", cx);
            let Some(broken) = at(2) else { eprintln!("settings plugin-stuck SHOTS"); cx.update(|a| a.quit()); return; };
            click(broken.0, broken.1, cx); wait(1000).await;
            assert!(!plugin::report().iter().any(|f| f.result.is_err()), "移除 takes the broken one away");
            shot("stuck-broken-gone", cx);
            eprintln!("settings plugin-stuck PASS");
            cx.update(|a| a.quit());
            return;
        }
        // Every way to install asks before anything of the package runs: an animation plugin
        // that also has a setup, from 完成动画, and an update that adds one, from 已安装.
        // MOORING_PLUGIN_CLICKS: 安装 in 完成动画, 确认安装, 更新, 确认安装. MOORING_GUARD_MARK
        // is the file the catalog's setups touch.
        if phase == "plugin-guard" {
            use mooring_core::plugin;
            let clicks: Vec<(f32, f32)> = std::env::var("MOORING_PLUGIN_CLICKS").unwrap_or_default()
                .split(';').filter_map(|p| p.split_once(',')).map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap())).collect();
            let at = |i: usize| clicks.get(i).copied();
            let mark = std::path::PathBuf::from(std::env::var_os("MOORING_GUARD_MARK").expect("guard mark"));
            handle.update(cx, |v, _, a| v.visual_settings_show("插件", a)).unwrap();
            wait(400).await;
            shot("guard-start", cx);
            let Some(install) = at(0) else { eprintln!("settings plugin-guard SHOTS"); cx.update(|a| a.quit()); return; };
            click(install.0, install.1, cx); wait(1000).await;
            assert!(plugin::installed("anim-setup").is_none(), "完成动画's 安装 asks first");
            assert!(!mark.exists(), "nothing of it ran");
            shot("guard-animation-asks", cx);
            let Some(confirm) = at(1) else { eprintln!("settings plugin-guard SHOTS"); cx.update(|a| a.quit()); return; };
            click(confirm.0, confirm.1, cx); wait(1500).await;
            assert!(plugin::installed("anim-setup").is_some());
            assert!(mark.exists(), "confirmed, its setup ran");
            std::fs::remove_file(&mark).unwrap();
            shot("guard-animation-installed", cx);
            let Some(update) = at(2) else { eprintln!("settings plugin-guard SHOTS"); cx.update(|a| a.quit()); return; };
            click(update.0, update.1, cx); wait(1000).await;
            assert_eq!(plugin::installed("up").unwrap().version.as_deref(), Some("1.0.0"), "更新 asks first");
            assert!(!mark.exists(), "nothing of the update ran");
            shot("guard-update-asks", cx);
            let Some(confirm) = at(3) else { eprintln!("settings plugin-guard SHOTS"); cx.update(|a| a.quit()); return; };
            click(confirm.0, confirm.1, cx); wait(1500).await;
            assert_eq!(plugin::installed("up").unwrap().version.as_deref(), Some("2.0.0"));
            assert!(mark.exists(), "confirmed, the update's setup ran");
            shot("guard-update-done", cx);
            eprintln!("settings plugin-guard PASS");
            cx.update(|a| a.quit());
            return;
        }
        // 插件: install from the catalog with real clicks, confirm one that runs commands,
        // switch one of its actions off, uninstall, then pick a plugin's animation preset.
        // MOORING_PLUGIN_CLICKS="x,y;…" in panel points, in this order: 安装 落印 (in
        // 完成动画), 安装 常用命令, 确认安装, its first action's switch, 卸载 落印, then
        // 用这个 on 慢扫 in 完成动画. Missing points stop after the shots.
        if phase == "plugins" {
            use mooring_core::{action, effect, plugin};
            let clicks: Vec<(f32, f32)> = std::env::var("MOORING_PLUGIN_CLICKS").unwrap_or_default()
                .split(';').filter_map(|p| p.split_once(',')).map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap())).collect();
            let at = |i: usize| clicks.get(i).copied();
            handle.update(cx, |v, _, a| v.visual_settings_show("插件", a)).unwrap();
            wait(400).await;
            shot("plugins-start", cx);
            assert!(plugin::installed("slow-sweep").is_some(), "the scratch plugin loads");
            let Some(stamp) = at(0) else { eprintln!("settings plugins SHOTS"); cx.update(|a| a.quit()); return; };
            click(stamp.0, stamp.1, cx); wait(1500).await;
            assert!(plugin::installed("complete-stamp").is_some(), "安装 installs and reloads");
            assert_eq!(effect::picked(), Some(effect::Pick::Plugin("complete-stamp".into())), "安装 switches to its animation");
            shot("plugins-installed", cx);
            let Some(actions) = at(1) else { eprintln!("settings plugins SHOTS"); cx.update(|a| a.quit()); return; };
            click(actions.0, actions.1, cx); wait(400).await;
            assert!(plugin::installed("common-actions").is_none(), "commands wait for 确认安装");
            shot("plugins-confirm", cx);
            let Some(confirm) = at(2) else { eprintln!("settings plugins SHOTS"); cx.update(|a| a.quit()); return; };
            click(confirm.0, confirm.1, cx); wait(1500).await;
            assert!(plugin::installed("common-actions").is_some());
            assert!(!action::all().is_empty(), "its actions load without a restart");
            shot("plugins-actions", cx);
            let Some(switch) = at(3) else { eprintln!("settings plugins SHOTS"); cx.update(|a| a.quit()); return; };
            click(switch.0, switch.1, cx); wait(400).await;
            assert!(!action::all()[0].enabled(), "the switch turns its action off");
            shot("plugins-action-off", cx);
            let Some(remove) = at(4) else { eprintln!("settings plugins SHOTS"); cx.update(|a| a.quit()); return; };
            click(remove.0, remove.1, cx); wait(600).await;
            assert!(plugin::installed("complete-stamp").is_none(), "卸载 removes it");
            assert_eq!(effect::picked(), Some(effect::Pick::Builtin(effect::Style::Seal)), "卸载 the one playing goes back to the seal");
            shot("plugins-removed", cx);
            let Some(slow) = at(5) else { eprintln!("settings plugins SHOTS"); cx.update(|a| a.quit()); return; };
            click(slow.0, slow.1, cx); wait(400).await;
            assert_eq!(effect::picked(), Some(effect::Pick::Plugin("slow-sweep".into())));
            assert_eq!(effect::complete().duration, Duration::from_millis(3_400));
            shot("plugins-look-picked", cx);
            eprintln!("settings plugins PASS");
            cx.update(|a| a.quit());
            return;
        }
        if phase == "faces" {
            faces::run(handle, &directory, cx).await;
            return;
        }
        if phase == "plugin-page" {
            plugin_page::run(handle, &directory, cx).await;
            return;
        }
        // 标记完成动画: 外观's row shows what plays and 换一个 turns to 插件, where 只淡出 is
        // picked with a real click and 试播 plays it on the shown pane, which must stay
        // open with nothing marked. MOORING_COMPLETE_CLICKS="x,y;x,y;x,y" are the panel
        // points of 换一个 (once the search leaves only that row), then 只淡出's 用这个 and
        // its 试播 on 插件.
        if phase == "complete" {
            use mooring_core::effect;
            click(106., 76., cx);
            cx.update_window(handle.into(), |_, w, a| {
                for key in ["e", "f", "f", "e", "c", "t", "s"] { w.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), a); }
            }).unwrap();
            wait(300).await;
            shot("complete-row", cx);
            let clicks: Vec<(f32, f32)> = std::env::var("MOORING_COMPLETE_CLICKS").unwrap_or_default()
                .split(';').filter_map(|p| p.split_once(',')).map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap())).collect();
            if let Some(&change) = clicks.first() {
                click(change.0, change.1, cx); wait(400).await;
                shot("complete-animations", cx);
            }
            if let [_, fade, preview] = clicks[..] {
                click(fade.0, fade.1, cx); wait(300).await;
                assert_eq!(effect::picked(), Some(effect::Pick::Builtin(effect::Style::Fade)));
                let saved = std::fs::read_to_string(crate::ui::status_colors::config_dir().unwrap().join("complete")).unwrap();
                assert_eq!(saved, "fade");
                shot("complete-picked", cx);
                let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
                pane.update(cx, |p, a| p.visual_type("printf 'preview of the completion animation\\n'\r", a));
                wait(400).await;
                click(preview.0, preview.1, cx); wait(300).await;
                assert!(handle.update(cx, |v, w, a| v.visual_settings(w, a)).unwrap().is_none(), "试播 closes 设置");
                shot("complete-preview-mid", cx);
                wait(3700).await; // Past the longest animation, 3.4 seconds.
                shot("complete-preview-after", cx);
                assert!(handle.update(cx, |v, _, _| v.visual_shown_pane_alive()).unwrap(), "试播 keeps the pane");
            }
            eprintln!("settings complete PASS");
            cx.update(|a| a.quit());
            return;
        }
        if phase == "2" {
            let saved = values(cx);
            assert_eq!(saved.default_agent, mooring_core::model::Agent::Claude);
            assert_eq!(saved.away_minutes, 15);
            assert_eq!(saved.stale_days, 7);
            let text = crate::ui::terminal_text::current();
            assert_eq!(text.family, "Menlo"); assert_eq!(text.size, 14.); assert_eq!(text.line_height, 1.4);
            assert_eq!(crate::ui::theme::Theme::scheme(), crate::ui::theme::Scheme::Dark);
        }
        shot("general", cx);
        if phase == "1" {
            click(578., 148., cx); wait(1000).await;
            assert_eq!(values(cx).default_agent, mooring_core::model::Agent::Claude);
            click(723., 253., cx); wait(1000).await;
            assert_eq!(values(cx).away_minutes, 15);
            click(814., 315., cx); wait(1000).await;
            assert_eq!(values(cx).stale_days, 7);
            shot("general-saved", cx);
        }
        nav("外观", cx); wait(200).await;
        assert_eq!(handle.update(cx, |v, _, a| v.visual_settings_page_name(a)).unwrap(), "外观");
        shot("appearance", cx);
        if phase == "1" {
            click(865., 147., cx); wait(200).await;
            assert_eq!(crate::ui::theme::Theme::scheme(), crate::ui::theme::Scheme::Dark);
            nav("终端", cx); wait(200).await;
            assert_eq!(handle.update(cx, |v, _, a| v.visual_settings_page_name(a)).unwrap(), "终端");
            click(300., 506., cx); wait(200).await;
            assert_eq!(crate::ui::terminal_text::current().family, "Menlo");
            click(874., 314., cx); wait(200).await;
            assert_eq!(crate::ui::terminal_text::size(), 14.);
            click(778., 380., cx); wait(200).await;
            assert_eq!(crate::ui::terminal_text::line_height(), 1.4);
            shot("appearance-dark-saved", cx);
            let blocked = crate::ui::terminal_text::path().unwrap().with_extension("json.tmp");
            std::fs::create_dir(&blocked).unwrap();
            // The taller preview at 14 pt / 1.4× moves the size row down.
            click(874., 344., cx); wait(200).await;
            assert_eq!(crate::ui::terminal_text::size(), 14., "failed save must not apply");
            shot("save-error", cx);
            std::fs::remove_dir(blocked).unwrap();
        }
        nav("快捷键", cx); wait(200).await;
        assert_eq!(handle.update(cx, |v, _, a| v.visual_settings_page_name(a)).unwrap(), "快捷键");
        shot("shortcuts", cx);
        nav("通知", cx); wait(200).await;
        assert_eq!(handle.update(cx, |v, _, a| v.visual_settings_page_name(a)).unwrap(), "通知");
        shot("notifications", cx);
        nav("插件", cx); wait(200).await;
        assert_eq!(handle.update(cx, |v, _, a| v.visual_settings_page_name(a)).unwrap(), "插件");
        shot("plugins", cx);
        nav("关于", cx); wait(200).await;
        assert_eq!(handle.update(cx, |v, _, a| v.visual_settings_page_name(a)).unwrap(), "关于");
        shot("about", cx);
        if phase == "inspect" {
            eprintln!("settings native inspection: seven category clicks and screenshots");
            cx.update(|a| a.quit());
            return;
        }
        // Native search input must receive characters without triggering workbench keys.
        click(106., 76., cx);
        cx.update_window(handle.into(), |_, w, a| {
            for key in ["f", "o", "n", "t"] { w.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), a); }
        }).unwrap();
        wait(200).await;
        let query = handle.update(cx, |v, _, a| v.visual_settings_query(a)).unwrap();
        assert_eq!(cx.read_entity(&query, |f, _| f.text.clone()), "font");
        shot("search", cx);
        // An unmatched search has an explicit empty state, then native Escape closes.
        cx.update_window(handle.into(), |_, w, a| {
            w.dispatch_keystroke(gpui::Keystroke::parse("cmd-a").unwrap(), a);
            for key in ["z", "z", "z", "z"] { w.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), a); }
        }).unwrap();
        wait(200).await;
        shot("search-empty", cx);
        cx.update_window(handle.into(), |_, w, a| { w.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), a); }).unwrap();
        assert!(handle.update(cx, |v, w, a| v.visual_settings(w, a)).unwrap().is_none());
        if phase == "2" {
            let pane = handle.update(cx, |v, _, _| v.visual_shown_pane()).unwrap();
            pane.update(cx, |p, a| p.visual_type("printf '\\033[2J\\033[HSettings typography: Menlo 14pt / 1.4x\\n中文输入与等宽文字 0123456789\\n'\r", a));
            wait(500).await;
            shot("terminal-after-restart", cx);
        }
        eprintln!("settings native PASS phase {phase}: category clicks, real saves/restart, failed write unchanged, search text and Escape");
        cx.update(|a| a.quit());
    }).detach();
    true
}
