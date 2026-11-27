//! The round trip to another machine (`machines::latency`), beside its name: what typing
//! there waits for. Under 100 ms reads as here; past 300 ms it is felt on every key.
//! It says which way it is (`ssh 17 ms`, `中继 139 ms`), and its tip how the other way
//! stands (docs/multi-machine/routes.md).
use super::*;

/// `ssh 23 ms`: the way taken to `host` now and its round trip, while it answers pings.
pub(super) fn round_trip_words(host: &str) -> Option<String> {
    let rtt = crate::machines::latency(host)?;
    let ms = match rtt.as_millis() {
        0 => t!("<1 ms").to_owned(),
        ms => t!("{ms} ms", ms = ms.to_string()),
    };
    Some(match crate::attach::route(host) {
        Some(route) => format!("{} {ms}", route.via.word()),
        None => ms,
    })
}

/// How the way not taken stands: `中继 139 ms`, `中继：没配对`; None with only one way.
pub(super) fn other_way(host: &str) -> Option<String> {
    let (via, state) = crate::attach::route(host)?.other?;
    Some(match state {
        Ok(rtt) => t!(
            "另一条：{way} {ms} ms",
            way = via.word(),
            ms = rtt.as_millis().to_string()
        ),
        Err(why) => t!("另一条：{way}：{why}", way = via.word(), why),
    })
}

/// [`round_trip_words`] in the color of how it feels, the other way in its tip.
pub(super) fn latency_tag(host: &str) -> Option<gpui::Stateful<gpui::Div>> {
    let words = round_trip_words(host)?;
    let ms = crate::machines::latency(host)?.as_millis();
    let tone = match ms {
        0..100 => Tone::Ok,
        100..300 => Tone::Warn,
        _ => Tone::Danger,
    };
    let tag = div()
        .id(gpui::ElementId::Name(format!("latency-{host}").into()))
        .flex_shrink_0()
        .text_color(tone.ink())
        .child(words);
    Some(match other_way(host) {
        Some(other) => tag.tooltip(super::diagnostics::tip(vec![other])),
        None => tag,
    })
}
