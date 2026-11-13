//! The machine's memory pressure for the status bar. The system announces each change, so
//! nothing polls: the bar shows a cell only while pressure is above normal, and a click opens
//! 本机诊断 with the memory row first.
use super::*;
use fibo_core::diagnostics::Pressure;
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
};
#[cfg(feature = "visual-test")]
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "visual-test")]
static FORCED: AtomicU8 = AtomicU8::new(0);
static WAKE: Mutex<Vec<UnboundedSender<()>>> = Mutex::new(Vec::new());
static STARTED: OnceLock<()> = OnceLock::new();

fn changed() {
    if let Ok(mut senders) = WAKE.lock() {
        senders.retain(|tx| tx.unbounded_send(()).is_ok());
    }
}

/// Listens for the system's announcements on first use and redraws `cx`'s view on each change.
pub(super) fn watch(cx: &mut Context<Workbench>) {
    STARTED.get_or_init(|| fibo_platform::system::watch_pressure(changed));
    let (tx, mut rx) = unbounded();
    if let Ok(mut senders) = WAKE.lock() {
        senders.push(tx);
    }
    cx.spawn(async move |this, cx| {
        while rx.next().await.is_some() {
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Pressure above normal, with swap in use, or None while memory is fine.
pub(super) fn tight() -> Option<(Pressure, u64)> {
    #[cfg(feature = "visual-test")]
    let forced = FORCED.load(Ordering::Acquire);
    #[cfg(not(feature = "visual-test"))]
    let forced = 0u8;
    let level = if forced != 0 {
        i32::from(forced)
    } else {
        fibo_platform::system::announced_pressure()?
    };
    let pressure = Pressure::from_level(level).filter(|p| *p != Pressure::Normal)?;
    Some((pressure, fibo_platform::system::swap_used()))
}

#[cfg(feature = "visual-test")]
pub(super) fn force(level: u8) {
    FORCED.store(level, Ordering::Release);
}
