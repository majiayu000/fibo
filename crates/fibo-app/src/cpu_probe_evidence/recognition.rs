//! Which recognition updates draw the workbench and which stay in the store.
use super::*;

pub(super) fn title_check(
    handle: WindowHandle<workbench::Workbench>,
    cx: &mut gpui::AsyncApp,
) -> String {
    use workbench::Workbench;

    const BATCH: usize = 30;
    let frame = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    let sidebar = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_sidebar_drawn(cx))
            .unwrap()
            .expect("sidebar must be on screen")
    };
    let image = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, _| w.render_to_image().unwrap())
            .unwrap()
    };
    let (original, terminal) = handle
        .update(cx, |v, _, _| {
            (v.visual_recognition().0, v.visual_shown_terminal())
        })
        .unwrap();
    let mut status = original.clone();
    let mut update = |title: &str, cx: &mut gpui::AsyncApp| {
        status
            .terminal_titles
            .insert(terminal.clone(), title.into());
        handle
            .update(cx, |v, _, cx| v.receive_recognition(status.clone(), cx))
            .unwrap();
        frame(cx);
    };
    // The probe starts in a new, automatically named shell tab. Let its strip
    // reach its settled position before sending the title changes.
    frame(cx);
    update("First probe title", cx);
    frame(cx);
    let before = image(cx);
    let mut last = sidebar(cx);
    let mut draws = 0;
    let strip = Workbench::visual_strip_draws();
    let (cpu, ins) = (cpu_secs(), instructions());
    for i in 0..BATCH {
        update(
            if i % 2 == 0 {
                "Next probe title"
            } else {
                "Last probe title"
            },
            cx,
        );
        let now = sidebar(cx);
        draws += usize::from(now != last);
        last = now;
    }
    let used = (cpu_secs() - cpu) * 1000. / BATCH as f64;
    let ins = (instructions() - ins) as f64 / BATCH as f64 / 1e6;
    let strip = Workbench::visual_strip_draws() - strip;
    let title = handle
        .update(cx, |v, _, cx| v.visual_shown_title(cx))
        .unwrap();
    let pixels = before
        .pixels()
        .zip(image(cx).pixels())
        .filter(|(a, b)| a != b)
        .count();
    handle
        .update(cx, |v, _, cx| v.receive_recognition(original, cx))
        .unwrap();
    format!(
        "title-check: {draws} sidebar draws, {strip} strip draws for {BATCH} title changes; {used:.2} ms CPU, {ins:.2} M instructions per update and draw; title arrived {}, {pixels} px changed",
        title == "Last probe title"
    )
}

pub(super) fn recognition_check(
    handle: WindowHandle<workbench::Workbench>,
    cx: &mut gpui::AsyncApp,
) -> String {
    use fibo_core::modes::TerminalModes;
    use fibo_core::protocol::{RecognitionPhase, TrustPrompt};

    const BATCH: usize = 30;
    let frame = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, a| w.draw(a).clear(a))
            .unwrap();
    };
    let sidebar = |cx: &mut gpui::AsyncApp| {
        handle
            .update(cx, |v, _, cx| v.visual_sidebar_drawn(cx))
            .unwrap()
            .expect("sidebar must be on screen")
    };
    let snapshot =
        |cx: &mut gpui::AsyncApp| handle.update(cx, |v, _, _| v.visual_recognition()).unwrap();
    frame(cx);
    let (original, _) = snapshot(cx);
    let terminal = handle
        .update(cx, |v, _, _| v.visual_shown_terminal())
        .unwrap();
    let mut status = original.clone();
    let trust = |i: usize| TrustPrompt {
        agent: Default::default(),
        question: "Trust this folder?".into(),
        folder: format!("/probe-{i}"),
    };
    // A prompt appearing is drawn (its terminal waits on you); only its words changing is
    // the bookkeeping measured here.
    status.terminal_trust.insert(terminal.clone(), trust(0));
    handle
        .update(cx, |v, _, cx| v.receive_recognition(status.clone(), cx))
        .unwrap();
    frame(cx);
    let mut last = sidebar(cx);
    let mut draws = 0;
    let (cpu, ins) = (cpu_secs(), instructions());
    for i in 1..=BATCH {
        status.metrics.ps_spawns += 1;
        status.terminal_modes.insert(
            terminal.clone(),
            TerminalModes {
                model: Some((format!("Probe model {i}"), "Logged model".into())),
                ..Default::default()
            },
        );
        status.terminal_trust.insert(terminal.clone(), trust(i));
        handle
            .update(cx, |v, _, cx| v.receive_recognition(status.clone(), cx))
            .unwrap();
        frame(cx);
        let now = sidebar(cx);
        draws += usize::from(now != last);
        last = now;
    }
    let used = (cpu_secs() - cpu) * 1000. / BATCH as f64;
    let ins = (instructions() - ins) as f64 / BATCH as f64 / 1e6;
    let (mut current, displayed_revision) = snapshot(cx);
    // The gate's existing arrival check also covers both phone metadata fields.
    let metrics_arrived = current.metrics.ps_spawns == status.metrics.ps_spawns
        && current.terminal_modes == status.terminal_modes
        && current.terminal_trust == status.terminal_trust;
    // The host announced a revision. UI work here is synchronous, so the fresh workspace
    // can't land until after this frame. There is nothing new to paint in this frame.
    current.workspace_revision = displayed_revision.max(current.workspace_revision) + 1;
    handle
        .update(cx, |v, _, cx| v.receive_recognition(current.clone(), cx))
        .unwrap();
    frame(cx);
    let revision_painted = sidebar(cx) != last;
    last = sidebar(cx);
    let image = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, _| w.render_to_image().unwrap())
            .unwrap()
    };
    let before = image(cx);
    current.phase = if current.phase == RecognitionPhase::Interrupted {
        RecognitionPhase::Healthy
    } else {
        RecognitionPhase::Interrupted
    };
    handle
        .update(cx, |v, _, cx| v.receive_recognition(current, cx))
        .unwrap();
    frame(cx);
    let painted = sidebar(cx) != last;
    let pixels = before
        .pixels()
        .zip(image(cx).pixels())
        .filter(|(a, b)| a != b)
        .count();
    // Leave the next phase the same host status it started with.
    handle
        .update(cx, |v, _, cx| v.receive_recognition(original, cx))
        .unwrap();
    format!(
        "recognition-check: {draws} sidebar draws for {BATCH} diagnostics updates; {used:.2} ms CPU, {ins:.2} M instructions per update and draw; metrics arrived {metrics_arrived}; revision alone painted {revision_painted}; phase painted {painted}, {pixels} px changed"
    )
}
