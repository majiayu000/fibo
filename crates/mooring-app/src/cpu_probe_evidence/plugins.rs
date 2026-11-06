//! `frames-plugins`: what 设置 › 插件 and the richer themes cost on the probe's large workspace.
//! - `settings-open`: creating the settings view, which reads every plugin's own files.
//! - `frames-cardrest`: the page of animation cards drawn again with none playing. Nothing
//!   asks for one by itself; this is what any other change on that page costs.
//! - `frames-cardplay`: a frame with one card playing, the costliest animation of them all
//!   over its whole run: what the window draws each display frame while you point at it.
//!   Found over a short run of each, then measured over a long run of its own.
//! - `stage-asks`: frames asked for at rest, while pointing, and after leaving.
//! - `frames-themeimage`, `frames-themepaper`, `frames-themeglass`: a terminal's frame under a
//!   theme with a picture, a tiled picture and a see-through window.
//!
//! The window is off screen: what the system's compositor spends on a see-through window
//! is not in these numbers.
use super::*;
use gpui::{Entity, WindowHandle};
use std::time::{Duration, Instant};
use terminal::pane::Pane;
use workbench::Workbench;

const FRAMES: u32 = 300;
/// Moments of each animation's run measured to find the costliest.
const MOMENTS: u32 = 40;
/// Name, plugin, theme and whether the theme is a light one, which it must be set as.
const THEMES: [(&str, &str, &str, bool); 3] = [
    (
        "frames-themeimage",
        "theme-harbor",
        "plugin:theme-harbor/harbor",
        false,
    ),
    (
        "frames-themepaper",
        "theme-paper",
        "plugin:theme-paper/paper",
        true,
    ),
    (
        "frames-themeglass",
        "theme-glass",
        "plugin:theme-glass/glass-dark",
        false,
    ),
];

/// CPU milliseconds and millions of instructions per frame of what `frame` does.
fn cost(frames: u32, mut frame: impl FnMut(u32)) -> (f64, f64) {
    let (cpu, ins) = (cpu_secs(), instructions());
    for n in 0..frames {
        frame(n);
    }
    (
        (cpu_secs() - cpu) / f64::from(frames) * 1000.,
        (instructions() - ins) as f64 / f64::from(frames) / 1e6,
    )
}

fn copy_plugin(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_plugin(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub(super) async fn run(
    handle: WindowHandle<Workbench>,
    pane: &Entity<Pane>,
    directory: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) -> String {
    let executor = cx.background_executor().clone();
    let wait = move |ms: u64| executor.timer(Duration::from_millis(ms));
    let mut out = Vec::new();

    // A full redraw here, so the shares below compare within one process on a machine
    // that counts no instructions.
    let full = {
        let cx = &mut *cx;
        cost(FRAMES, |_| {
            cx.update_window(handle.into(), |_, w, a| {
                w.refresh();
                w.draw(a).clear(a);
            })
            .unwrap();
        })
    };
    out.push(format!(
        "frames-probefull: {:.2} ms CPU, {:.2} M instructions",
        full.0, full.1
    ));
    let plain = {
        let cx = &mut *cx;
        cost(FRAMES, |_| {
            pane.update(cx, |_, cx| cx.notify());
            cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
                .unwrap();
        })
    };
    out.push(format!(
        "frames-probepane: {:.2} ms CPU, {:.2} M instructions",
        plain.0, plain.1
    ));

    let (at, ins) = (Instant::now(), instructions());
    handle
        .update(cx, |v, w, cx| v.show_settings(w, cx))
        .unwrap();
    let open_ms = at.elapsed().as_secs_f64() * 1000.;
    let open_ins = (instructions() - ins) as f64 / 1e6;
    out.push(format!(
        "settings-open: {open_ms:.2} ms wall, {open_ins:.2} M instructions"
    ));
    // Settings on a plain page first: what is the page's and what is settings' own.
    let plain_frame = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_plugins_change(cx))
            .unwrap();
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    handle
        .update(cx, |v, _, a| v.visual_settings_show("关于", a))
        .unwrap();
    wait(300).await;
    let base = {
        let cx = &mut *cx;
        cost(FRAMES, |_| plain_frame(cx))
    };
    out.push(format!(
        "frames-settingsbase: {:.2} ms CPU, {:.2} M instructions",
        base.0, base.1
    ));
    handle
        .update(cx, |v, _, a| v.visual_settings_show("插件", a))
        .unwrap();
    handle
        .update(cx, |v, _, cx| v.visual_plugins_grid(None, cx))
        .unwrap();
    wait(500).await;
    let change = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_plugins_change(cx))
            .unwrap();
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    let frame = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_plugins_frame(cx))
            .unwrap();
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    let rest = {
        let cx = &mut *cx;
        cost(FRAMES, |_| change(cx))
    };
    out.push(format!(
        "frames-cardrest: {:.2} ms CPU, {:.2} M instructions",
        rest.0, rest.1
    ));

    // Every animation over its whole run, each moment held so a run repeats exactly.
    let mut worst = ("", 0., 0.);
    let styles: Vec<&str> = mooring_core::effect::Style::ALL
        .iter()
        .map(|s| s.name())
        .collect();
    let mut played = 0;
    for style in &styles {
        let id = format!("complete-{style}");
        handle
            .update(cx, |v, _, cx| v.visual_plugins_grid(Some(&id), cx))
            .unwrap();
        // Pointing at it draws the page once; the frames after that are what is measured.
        frame(cx);
        let (ms, ins) = {
            let cx = &mut *cx;
            cost(MOMENTS, |n| {
                Workbench::visual_stage_hold(Some(n as f32 / (MOMENTS - 1) as f32));
                frame(cx);
            })
        };
        if Workbench::visual_stage_asked() > 0 {
            played += 1;
            // A virtual machine counts no instructions: then time is all there is to go by.
            if ins > worst.2 || (ins == worst.2 && ms > worst.1) {
                worst = (style, ms, ins);
            }
        }
    }
    // The search keeps the unluckiest of its short runs: on a virtual machine one 40-frame
    // run caught in a stall was reported as the card's cost (0.40 and 0.25 from one commit).
    // So the number comes from a run of its own, as long as the redraw it is compared with.
    let mut measured = (0., 0.);
    if played > 0 {
        let id = format!("complete-{}", worst.0);
        handle
            .update(cx, |v, _, cx| v.visual_plugins_grid(Some(&id), cx))
            .unwrap();
        frame(cx);
        let cx = &mut *cx;
        measured = cost(FRAMES, |n| {
            Workbench::visual_stage_hold(Some((n % MOMENTS) as f32 / (MOMENTS - 1) as f32));
            frame(cx);
        });
    }
    Workbench::visual_stage_hold(None);
    out.push(format!(
        "frames-cardplay: {:.2} ms CPU, {:.2} M instructions; the costliest of {played}: {}, {:.2} ms over {MOMENTS} frames",
        measured.0, measured.1, worst.0, worst.1
    ));

    // Reusing the page hides nothing, and what is measured really moves: with a card held
    // playing, a frame that reuses settings matches one drawn from scratch, pixel for pixel,
    // and two moments of the run differ.
    let picture = |refresh: bool, cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| {
            if refresh {
                w.refresh();
                w.draw(a).clear(a);
            } else {
                w.draw(a).clear(a);
            }
            w.render_to_image().unwrap().into_raw()
        })
        .unwrap()
    };
    handle
        .update(cx, |v, _, cx| {
            v.visual_plugins_grid(Some("complete-tea"), cx)
        })
        .unwrap();
    Workbench::visual_stage_hold(Some(0.6));
    frame(cx);
    frame(cx);
    handle
        .update(cx, |v, _, cx| v.visual_plugins_frame(cx))
        .unwrap();
    let reused = picture(false, cx);
    let fresh = picture(true, cx);
    Workbench::visual_stage_hold(Some(0.15));
    handle
        .update(cx, |v, _, cx| v.visual_plugins_frame(cx))
        .unwrap();
    let earlier = picture(false, cx);
    Workbench::visual_stage_hold(None);
    out.push(format!(
        "player-pixels: {}; moves {}",
        if reused == fresh { "same" } else { "DIFF" },
        if earlier == reused { "NO" } else { "yes" }
    ));

    // A stopped page must not keep the window drawing.
    handle
        .update(cx, |v, _, cx| v.visual_plugins_grid(None, cx))
        .unwrap();
    frame(cx);
    Workbench::visual_stage_asked();
    for _ in 0..30 {
        frame(cx);
    }
    let at_rest = Workbench::visual_stage_asked();
    handle
        .update(cx, |v, _, cx| {
            v.visual_plugins_grid(Some("complete-cat"), cx)
        })
        .unwrap();
    for _ in 0..30 {
        frame(cx);
    }
    let pointing = Workbench::visual_stage_asked();
    handle
        .update(cx, |v, _, cx| v.visual_plugins_grid(None, cx))
        .unwrap();
    frame(cx);
    Workbench::visual_stage_asked();
    for _ in 0..30 {
        frame(cx);
    }
    let after = Workbench::visual_stage_asked();
    out.push(format!(
        "stage-asks: at rest {at_rest}, pointing {pointing}, after leaving {after}"
    ));
    handle
        .update(cx, |v, _, a| v.visual_settings_close(a))
        .unwrap();
    wait(300).await;

    // The themes, installed as a person would have them.
    let catalog = mooring_core::market::bundled_dir().expect("the catalog");
    let plugins = mooring_core::plugin::dir(mooring_core::plugin::config().expect("config"));
    for (_, id, _, _) in THEMES {
        copy_plugin(&catalog.join(id), &plugins.join(id));
    }
    cx.update(|a| {
        mooring_core::plugin::reload();
        ui::themes::reload(a);
    });
    for (name, _, theme, light) in THEMES {
        cx.update(|a| ui::appearance::set_theme(theme, light, true, a))
            .unwrap();
        // A picture decodes off the main thread and the window asks to draw it afterwards.
        wait(1500).await;
        cx.update_window(handle.into(), |_, w, a| {
            w.refresh();
            w.draw(a).clear(a);
        })
        .unwrap();
        let applied = cx.update(|_| {
            let p = theme::Theme::palette();
            match name {
                "frames-themeglass" => p.opacity < 100,
                _ => p
                    .backdrop
                    .as_ref()
                    .is_some_and(|b| b.picture.image().is_some()),
            }
        });
        out.push(format!(
            "theme-applied {name}: {}",
            if applied { "yes" } else { "NO" }
        ));
        let (ms, ins) = {
            let cx = &mut *cx;
            cost(FRAMES, |_| {
                pane.update(cx, |_, cx| cx.notify());
                cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
                    .unwrap();
            })
        };
        out.push(format!("{name}: {ms:.2} ms CPU, {ins:.2} M instructions"));
        cx.update_window(handle.into(), |_, w, a| {
            w.refresh();
            w.draw(a).clear(a);
            w.render_to_image()
                .unwrap()
                .save(directory.join(format!("{name}.png")))
                .unwrap();
        })
        .unwrap();
    }
    out.join("\n")
}
