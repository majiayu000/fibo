//! The round trip to another machine (`machines::latency`), beside its name: what typing
//! there waits for. Under 100 ms reads as here; past 300 ms it is felt on every key.
use super::*;

/// `23 ms` in the color of how it feels, while `host` is connected and answers pings.
pub(super) fn latency_tag(host: &str) -> Option<gpui::Div> {
    let rtt = crate::machines::latency(host)?;
    let ms = rtt.as_millis();
    let tone = match ms {
        0..100 => Tone::Ok,
        100..300 => Tone::Warn,
        _ => Tone::Danger,
    };
    Some(
        div()
            .flex_shrink_0()
            .text_color(tone.ink())
            .child(match ms {
                0 => t!("<1 ms").to_owned(),
                ms => t!("{ms} ms", ms = ms.to_string()),
            }),
    )
}
