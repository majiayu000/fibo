//! How an interrupted turn ends: the second half of the window's 中断这一轮 and a phone's 停下.
use super::*;
use fibo_core::status::Phase;

/// Give Codex time to redraw the footer after its log records the interrupt.
const BACKGROUND_AFTER: Duration = Duration::from_millis(1500);
const STOP_WAIT: Duration = Duration::from_secs(7);

/// Codex can interrupt its turn while leaving a command alive. Its /stop closes every
/// background terminal in the thread, including servers launched in earlier turns.
pub(super) fn await_interrupt(shared: &Shared, id: &str) -> io::Result<Reply> {
    let began = Instant::now();
    loop {
        let w = lock(shared)?.snapshot();
        let Some(session) = w.current_agent(id) else {
            return Err(io::Error::other(t!("Agent 退出了，这个终端里已经没有对话")));
        };
        let state = fibo_core::status::of(&w, session, fibo_core::content::now());
        let ended = state.phase() != Phase::Working;
        let elapsed = began.elapsed();
        let background = crate::agents::adapter(session.runtime).interrupt_background();
        if (ended && (background.is_none() || elapsed >= BACKGROUND_AFTER)) || elapsed >= STOP_WAIT
        {
            if let Some(reports_running) = background {
                let terminal = lock(shared)?
                    .terminals
                    .get(id)
                    .cloned()
                    .ok_or_else(|| io::Error::other("terminal not found"))?;
                let live = lock(&terminal)?;
                let tail = live.vt.tail_text(usize::from(live.rows))?;
                if reports_running(&tail) {
                    return Err(io::Error::other(if ended {
                        t!("回合已中断，Codex 仍有后台命令在运行。请打开终端查看")
                    } else {
                        t!("Codex 仍有后台命令在运行，请打开终端查看")
                    }));
                }
            }
            if ended {
                return Ok(Reply::Interrupted {
                    closed_background: false,
                });
            }
            return Err(io::Error::other(t!(
                "已发送中断，暂时还没确认停下。可以打开终端查看"
            )));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
