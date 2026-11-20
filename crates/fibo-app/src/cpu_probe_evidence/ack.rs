//! Measures real pane notifications when the PTY accepts input without sending output.
use super::*;
use crate::{terminal::pane::Pane, workbench::Workbench};
use std::{cell::Cell, rc::Rc};

const QUIET_KEYS: usize = 120;
const ECHO_KEYS: usize = 20;

pub(super) async fn run(
    handle: WindowHandle<Workbench>,
    pane: &gpui::Entity<Pane>,
    cx: &mut gpui::AsyncApp,
) -> String {
    let blink = crate::ui::terminal_text::visual_cursor_blink(false);
    let executor = cx.background_executor().clone();
    let draw = |cx: &mut gpui::AsyncApp| {
        cx.update_window(handle.into(), |_, w, app| w.draw(app).clear(app))
            .unwrap();
    };
    let send = |text: &str, cx: &mut gpui::AsyncApp| {
        pane.update(cx, |p, cx| p.visual_type(text, cx));
    };
    draw(cx);
    // The ready marker is produced only after disabling terminal echo. The program
    // acknowledges the entire quiet burst after a separate key ends the measurement.
    // Each subsequent key gets an individually identifiable response from the PTY.
    let command = format!(
        r#"stty -echo -icanon min 1 time 0 && perl -e '
            sub key {{ sysread(STDIN, my $b, 1) == 1 or exit 1; return $b; }}
            $| = 1;
            printf "\nFIBO_ACK_%s\n", "READY";
            for (1..{QUIET_KEYS}) {{ key() eq "x" or exit 2; }}
            key() eq "!" or exit 3;
            printf "\nFIBO_ACK_%s_%d\n", "DONE", {QUIET_KEYS};
            for my $i (1..{ECHO_KEYS}) {{
                my $b = key();
                printf "\nFIBO_ACK_%s_%02d=%s\n", "ECHO", $i, $b;
            }}
        '; stty sane"#
    )
    .replace('\n', " ")
        + "\r";
    send(&command, cx);
    wait_for_line(pane, "FIBO_ACK_READY", &draw, cx).await;
    // A new pane schedules loading notes through its third second, even when ready.
    executor.timer(Duration::from_millis(3300)).await;
    draw(cx);
    executor.timer(Duration::from_millis(80)).await;
    let initial = pane.read_with(cx, |p, _| p.visual_text());
    let wakes = Rc::new(Cell::new(0));
    let seen = wakes.clone();
    let observe = handle
        .update(cx, |_, _, cx| {
            cx.observe(pane, move |_, _, _| seen.set(seen.get() + 1))
        })
        .unwrap();
    let drawn_through = Cell::new(0);
    let draws = Cell::new(0);
    // This window has no display link. Paint on observed notifications, not every
    // loop iteration, so a fake wake or the probe itself cannot account for a frame.
    let draw_if_notified = |cx: &mut gpui::AsyncApp| {
        let pending = wakes.get();
        if pending != drawn_through.replace(pending) {
            draw(cx);
            draws.set(draws.get() + 1);
        }
    };

    let (cpu, ins) = (cpu_secs(), instructions());
    for _ in 0..QUIET_KEYS {
        send("x", cx);
        executor.timer(Duration::from_micros(8_333)).await;
        draw_if_notified(cx);
    }
    executor.timer(Duration::from_millis(80)).await;
    draw_if_notified(cx);
    let cpu_ms = (cpu_secs() - cpu) * 1000.;
    let million = (instructions() - ins) as f64 / 1e6;
    let (quiet_wakes, quiet_draws) = (wakes.get(), draws.get());
    assert_eq!(
        pane.read_with(cx, |p, _| p.visual_text()),
        initial,
        "quiet input unexpectedly changed the terminal"
    );

    send("!", cx);
    wait_for_line(
        pane,
        &format!("FIBO_ACK_DONE_{QUIET_KEYS}"),
        &draw_if_notified,
        cx,
    )
    .await;
    let before_echo = wakes.get();
    let mut echoes = Vec::with_capacity(ECHO_KEYS);
    for i in 1..=ECHO_KEYS {
        let began = Instant::now();
        send("y", cx);
        wait_for_line(
            pane,
            &format!("FIBO_ACK_ECHO_{i:02}=y"),
            &draw_if_notified,
            cx,
        )
        .await;
        echoes.push(began.elapsed().as_secs_f64() * 1000.);
    }
    let echo_wakes = wakes.get() - before_echo;
    assert!(
        echo_wakes >= ECHO_KEYS,
        "the echoes never notified the pane"
    );
    echoes.sort_by(f64::total_cmp);
    let echo_p95 = echoes[(ECHO_KEYS * 95).div_ceil(100) - 1];
    drop(observe);
    crate::ui::terminal_text::visual_cursor_blink(blink);
    format!(
        "quiet-input: {QUIET_KEYS} input bytes; quiet pane wakes {quiet_wakes}; quiet draws {quiet_draws}; {million:.2} M instructions, {cpu_ms:.2} ms CPU; echo output {ECHO_KEYS}/{ECHO_KEYS}; echo pane wakes {echo_wakes}; echo p95 {echo_p95:.2} ms"
    )
}

async fn wait_for_line(
    pane: &gpui::Entity<Pane>,
    marker: &str,
    draw: &impl Fn(&mut gpui::AsyncApp),
    cx: &mut gpui::AsyncApp,
) {
    let executor = cx.background_executor().clone();
    let started = Instant::now();
    while !pane.read_with(cx, |p, _| {
        p.visual_text_shown()
            .lines()
            .any(|line| line.trim() == marker)
    }) {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the pane did not paint {marker}"
        );
        executor.timer(Duration::from_millis(4)).await;
        draw(cx);
    }
}
