//! `mooring send`, `prompt` and `wait`: typing into a terminal and waiting on its Agent.
use super::*;

pub(super) fn send(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, t!("终端"))?)?;
    let text = a.text(1)?;
    expect_ok(rpc(
        &a.socket,
        &Request::SendText {
            session: None,
            id,
            text,
            submit: a.has("enter"),
        },
    )?)
}
pub(super) fn prompt(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, t!("终端"))?)?;
    let text = a.text(1)?;
    match phase(&w, &id) {
        Phase::None => {
            return Err(io::Error::other(t!(
                "这个终端里没有可识别的 Agent 对话；发给 shell 请用 send"
            )))
        }
        // Typing into an approval or a choice would answer it, so refuse.
        Phase::Waiting if !a.has("force") => {
            return Err(io::Error::other(t!(
                "Agent 正在等你回答（审批或选择题），没有发送；确认要发请加 --force"
            )));
        }
        _ => (),
    }
    let started = Instant::now();
    expect_ok(rpc(
        &a.socket,
        &Request::SendText {
            session: None,
            id: id.clone(),
            text,
            submit: true,
        },
    )?)?;
    if a.has("wait") {
        let timeout = Duration::from_secs(a.number("timeout", 1800)?);
        let done = wait(
            &a.socket,
            &id,
            &[Phase::Idle, Phase::Waiting],
            timeout,
            started,
        )?;
        println!("{}", done.name());
    }
    Ok(())
}
pub(super) fn wait_cmd(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let id = resolve(&w, a.arg(0, t!("终端"))?)?;
    let target = match a.value("until").unwrap_or("idle") {
        "idle" => Phase::Idle,
        "waiting" => Phase::Waiting,
        "working" => Phase::Working,
        other => {
            return Err(io::Error::other(t!(
                "--until 只接受 idle|waiting|working，不是 {other}",
                other
            )))
        }
    };
    let timeout = Duration::from_secs(a.number("timeout", 1800)?);
    let done = wait(&a.socket, &id, &[target], timeout, Instant::now())?;
    println!("{}", done.name());
    Ok(())
}
/// Polls until the Agent reaches one of `targets`. Right after a prompt the state may
/// still read idle from before it, so idle only counts once the turn started or after
/// a grace period with no sign of work.
fn wait(
    socket: &Path,
    id: &str,
    targets: &[Phase],
    timeout: Duration,
    since: Instant,
) -> io::Result<Phase> {
    const GRACE: Duration = Duration::from_secs(20);
    let since_secs = epoch_secs(since);
    let mut started = !targets.contains(&Phase::Idle);
    loop {
        let w = workspace(socket)?;
        if !w
            .terminals
            .iter()
            .any(|t| t.id == id && t.state == ProcessState::Running)
        {
            return Err(io::Error::other(t!("终端已退出")));
        }
        let now = phase(&w, id);
        started |= now == Phase::Working
            || w.current_agent(id).is_some_and(|s| {
                w.content
                    .get(&s.id)
                    .is_some_and(|c| c.updated >= since_secs)
            });
        if targets.contains(&now) && (now != Phase::Idle || started || since.elapsed() >= GRACE) {
            return Ok(now);
        }
        // A shell with no Agent never gets there: say so instead of sitting out the
        // timeout (30 minutes by default). An Agent just started gets the grace to show up.
        if now == Phase::None && since.elapsed() >= GRACE.min(timeout) {
            return Err(io::Error::other(t!(
                "这个终端里没有可识别的 Agent 对话；等 shell 里的命令请用 fibo read 看画面"
            )));
        }
        if since.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                t!("等待超时，当前 {phase}", phase = now.name()),
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
/// `at` in whole seconds since the epoch, the unit content cards record `updated` in.
fn epoch_secs(at: Instant) -> u64 {
    std::time::SystemTime::now()
        .checked_sub(at.elapsed())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}
