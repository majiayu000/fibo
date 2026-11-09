//! `mooring run`: starting an Agent beside the caller and handing it a task.
use super::open::open;
use super::*;

/// `split`: a new shell beside a terminal, in its
/// tab, in the directory that shell is in now. New panes only go right or down, so the
/// pane the user started in keeps the top-left corner. A tab that already has four panes,
/// or a split that would leave a half too small, sends the pane to a side tab named after
/// that terminal instead (`--here` refuses then; `--tab` always goes there). The
/// workbench shows it as soon as it next reads the host and never moves the keyboard or
/// the shown tab for it.
/// `mooring run <agent> <任务|->`: starts an Agent for the terminal calling, hands it the
/// task once its input is ready, and has the host report each turn it ends back into the
/// caller's input. The new terminal goes to the caller's side tab unless `--split`.
pub(super) fn run_agent(a: Args) -> io::Result<()> {
    use mooring_core::agent::Agent;
    let key = a.arg(0, "Agent")?.to_lowercase();
    let agent = Agent::all()
        .into_iter()
        .find(|g| agent_label(*g) == key || g.executable() == key)
        .ok_or_else(|| {
            let known: Vec<String> = Agent::all().into_iter().map(agent_label).collect();
            io::Error::other(format!("不认识的 Agent：{key}；可选：{}", known.join("、")))
        })?;
    let task = a.text(1)?;
    if task.trim().is_empty() {
        return Err(io::Error::other("任务是空的"));
    }
    let from = match a.value("from") {
        Some(from) => from.to_owned(),
        None => std::env::var("MOORING_TERMINAL").map_err(|_| {
            io::Error::other("不在 Fibo 终端里：用 --from <终端> 说要开在谁旁边、汇报给谁")
        })?,
    };
    let parent = resolve(&workspace(&a.socket)?, &from)?;
    let opened = open(&a, &parent, &[agent.command()], !a.has("split"))?;
    let report = !a.has("no-report");
    if report {
        expect_ok(rpc(
            &a.socket,
            &Request::ReportTo {
                child: opened.id.clone(),
                parent: parent.clone(),
            },
        )?)?;
    }
    let timeout = Duration::from_secs(a.number("timeout", 90)?);
    if let Err(e) = hand_over(&a.socket, agent, &opened.id, &task, timeout) {
        return Err(io::Error::other(format!(
            "{e}（终端 {} 留着，fibo read {} 看画面）",
            opened.id, opened.id
        )));
    }
    if a.has("json") {
        println!(
            "{}",
            serde_json::json!({
                "id": opened.id,
                "agent": agent_label(agent),
                "tab": opened.tab,
                "tab_title": opened.tab_title,
                "beside": opened.beside,
                "report_to": report.then_some(&parent),
            })
        );
    } else {
        println!("{}", opened.id);
        let place = if opened.beside.is_some() {
            "开在旁边".to_owned()
        } else {
            format!("开在「{}」标签（不切换用户的画面）", opened.tab_title)
        };
        let back = if report {
            "；它每结束一轮，结果会以 <mooring from=…> 自动发回你的输入框，不用轮询"
        } else {
            ""
        };
        eprintln!("{} 已经收到任务，{place}{back}", agent.name());
    }
    Ok(())
}
/// Types `text` into the Agent starting in `id` once its input box shows and the screen
/// has held still, as the workbench does with a todo's text.
fn hand_over(
    socket: &Path,
    agent: mooring_core::agent::Agent,
    id: &str,
    text: &str,
    timeout: Duration,
) -> io::Result<()> {
    use mooring_core::terminal::command_missing;
    let started = Instant::now();
    let mut last = String::new();
    let mut still = 0;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let tail = match rpc(
            socket,
            &Request::ReadScreen {
                id: id.into(),
                rows: 40,
            },
        )? {
            Reply::Text(text) => text.trim_end().to_owned(),
            other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
        };
        if command_missing(&tail) {
            return Err(io::Error::other("找不到这个 Agent 的命令，没有交任务"));
        }
        if tail == last {
            still += 1;
        } else {
            last = tail;
            still = 0;
        }
        if still >= 2 && agent.input_ready(&last) {
            return expect_ok(rpc(
                socket,
                &Request::SendText {
                    session: None,
                    id: id.into(),
                    text: text.into(),
                    submit: true,
                },
            )?);
        }
        if started.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Agent 一直没有出现输入框（可能停在登录、信任目录或更新提示上），没有交任务",
            ));
        }
    }
}
