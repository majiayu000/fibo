//! `mooring todo`: what an Agent writes down and writes back. There is no way to mark one
//! done here on purpose: completing a todo is the user's call, made in the window.
use super::*;
use mooring_core::model::short_id;
use mooring_core::status::task_state;
use mooring_core::time;

pub(super) fn todo(a: Args) -> io::Result<()> {
    match a.arg(0, "子命令（list、show、add、note）")? {
        "list" => list(a),
        "show" => show(a),
        "add" => add(a),
        "note" => note(a),
        "done" | "complete" | "close" => Err(io::Error::other(
            "待办只由用户在窗口里标记完成，Agent 不能标；把结果写成 fibo todo note，等用户看",
        )),
        other => Err(io::Error::other(format!(
            "没有 fibo todo {other}\n用法：\n{}",
            a.usage
        ))),
    }
}

/// The conversation whose CLI says it ran this command. Its terminal's variable may be
/// missing or another terminal's when the command comes from a shared daemon.
fn calling_session(w: &Workspace) -> Option<&AgentSession> {
    session_named(w, &mooring_host::calling_conversations())
}
fn session_named<'a>(w: &'a Workspace, ids: &[(Agent, String)]) -> Option<&'a AgentSession> {
    w.agent_sessions.iter().find(|s| {
        ids.iter()
            .any(|(a, id)| *a == s.runtime && *id == s.cli_session_id)
    })
}
fn own_terminal(w: &Workspace) -> Option<String> {
    match calling_session(w) {
        Some(session) => session.terminal.clone(),
        None => std::env::var("MOORING_TERMINAL").ok(),
    }
}
/// The conversation running the caller, if there is one.
fn own_session(w: &Workspace) -> Option<String> {
    if let Some(session) = calling_session(w) {
        return Some(session.id.clone());
    }
    w.current_agent(&own_terminal(w)?).map(|s| s.id.clone())
}
/// `--task`: a todo's id, the start of it, or its exact title. Without it, the todo the
/// calling terminal works for.
fn pick<'a>(w: &'a Workspace, a: &Args) -> io::Result<&'a Task> {
    let Some(needle) = a.value("task") else {
        return own_terminal(w)
            .and_then(|t| w.terminal_task(&t))
            .ok_or_else(|| {
                io::Error::other(
                    "这个终端没有关联待办：用 --task <待办> 指定，fibo todo list 能看编号",
                )
            });
    };
    let found: Vec<&Task> = w
        .tasks
        .iter()
        .filter(|t| t.id.starts_with(needle) || t.title == needle)
        .collect();
    match found.as_slice() {
        [task] => Ok(task),
        [] => Err(io::Error::other(format!("没有这个待办：{needle}"))),
        many => Err(io::Error::other(format!(
            "有 {} 个待办对得上「{needle}」，多写几位编号",
            many.len()
        ))),
    }
}
fn space_name<'a>(w: &'a Workspace, id: &'a str) -> &'a str {
    w.spaces
        .iter()
        .find(|s| s.id == id)
        .map_or(id, |s| s.name.as_str())
}
/// The space whose folder holds `cwd`, the deepest when they nest.
fn space_at<'a>(w: &'a Workspace, cwd: &Path) -> Option<&'a Space> {
    w.spaces
        .iter()
        .filter(|s| !s.cwd.is_empty() && cwd.starts_with(&s.cwd))
        .max_by_key(|s| s.cwd.len())
}
/// `--space`, else the calling terminal's space, else the one this folder is in, else the
/// only space there is.
fn pick_space(w: &Workspace, a: &Args) -> io::Result<String> {
    if let Some(needle) = a.value("space") {
        return w
            .spaces
            .iter()
            .find(|s| s.id == needle || s.name == needle)
            .map(|s| s.id.clone())
            .ok_or_else(|| io::Error::other(format!("没有这个空间：{needle}")));
    }
    if let Some(tab) = own_terminal(w).and_then(|t| w.terminal_tab(&t).cloned()) {
        return Ok(tab.space);
    }
    let here = std::env::current_dir().ok();
    if let Some(space) = here.and_then(|cwd| space_at(w, &cwd)) {
        return Ok(space.id.clone());
    }
    match w.spaces.as_slice() {
        [only] => Ok(only.id.clone()),
        spaces => {
            let names: Vec<&str> = spaces.iter().map(|s| s.name.as_str()).collect();
            Err(io::Error::other(format!(
                "不在 Fibo 终端里，用 --space <空间> 说记到哪：{}",
                names.join("、")
            )))
        }
    }
}
fn deliver_edit(a: &Args, edit: Edit) -> io::Result<()> {
    let request = Request::Edit {
        request: Some(RequestId::fresh()),
        edit,
    };
    deliver(&a.socket, &request, PATIENCE)
        .into_result()
        .map(|_| ())
}
fn print_json(value: &serde_json::Value) -> io::Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(io::Error::other)?
    );
    Ok(())
}

fn task_json(w: &Workspace, task: &Task, now: u64) -> serde_json::Value {
    let state = task_state(w, task, now);
    let log = w.task_logs.get(&task.id).cloned().unwrap_or_default();
    serde_json::json!({
        "id": task.id,
        "title": task.title,
        "said": task.description,
        "space": space_name(w, &task.space),
        "state": state.label(),
        "closed": state.closed(),
        "created": log.created,
        "closed_at": log.closed,
        "source": log.source,
        "notes": log.notes,
    })
}

fn list(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let now = time::now();
    let shown: Vec<&Task> = w
        .tasks
        .iter()
        .filter(|t| a.has("all") || !task_state(&w, t, now).closed())
        .collect();
    if a.has("json") {
        let rows: Vec<_> = shown.iter().map(|t| task_json(&w, t, now)).collect();
        return print_json(&serde_json::Value::Array(rows));
    }
    if shown.is_empty() {
        println!("没有待办");
        return Ok(());
    }
    let lines = shown
        .into_iter()
        .map(|task| super::todo_text::Line {
            id: short_id(&w, &task.id).to_owned(),
            state: task_state(&w, task, now),
            space: space_name(&w, &task.space).to_owned(),
            title: task.title.clone(),
            note: w
                .task_logs
                .get(&task.id)
                .and_then(|log| log.notes.last())
                .map_or(String::new(), |n| n.text.clone()),
        })
        .collect();
    let color = {
        use std::io::IsTerminal;
        io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
    };
    print!("{}", super::todo_text::render(lines, color));
    Ok(())
}

fn show(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let now = time::now();
    let task = pick(&w, &a)?;
    if a.has("json") {
        return print_json(&task_json(&w, task, now));
    }
    println!(
        "{}  {}  {}\n{}",
        short_id(&w, &task.id),
        task_state(&w, task, now).label(),
        space_name(&w, &task.space),
        task.title
    );
    if !task.description.trim().is_empty() {
        println!("\n用户写的：\n{}", task.description.trim());
    }
    if !task.acceptance.is_empty() {
        println!("\n验收标准：");
        for c in &task.acceptance {
            println!("- {}", c.description);
        }
    }
    if let Some(log) = w.task_logs.get(&task.id).filter(|l| !l.notes.is_empty()) {
        println!("\n进展：");
        for n in &log.notes {
            println!("- {} {}", time::ago(n.at, now), n.text);
        }
    }
    Ok(())
}

fn add(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let title = a.text(1)?;
    let space = pick_space(&w, &a)?;
    let session = own_session(&w);
    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        space,
        title: title.trim().to_owned(),
        description: a.value("said").unwrap_or_default().trim().to_owned(),
        acceptance: vec![],
        agent: w.settings.default_agent,
    };
    let id = task.id.clone();
    deliver_edit(
        &a,
        Edit::AddTaskFrom {
            task,
            session: session.clone(),
        },
    )?;
    // Only when asked: most things noted mid-conversation are not what it is working on.
    if a.has("work") {
        let session =
            session.ok_or_else(|| io::Error::other("这个终端里没有会话，--work 没有东西可关联"))?;
        deliver_edit(
            &a,
            Edit::AssignTask {
                id: session,
                task: Some(id.clone()),
            },
        )?;
    }
    if a.has("json") {
        return print_json(&serde_json::json!({ "id": id }));
    }
    let w = workspace(&a.socket)?;
    println!("{}", short_id(&w, &id));
    Ok(())
}

/// Who wrote a note: the caller's conversation if it works for this todo or noted it, else
/// the one conversation working for it. A daemon may carry another terminal's variable, so
/// an unrelated conversation is never named.
fn note_session(w: &Workspace, task: &str) -> Option<String> {
    let own = own_session(w).filter(|id| {
        let works = w
            .agent_sessions
            .iter()
            .any(|s| &s.id == id && s.task.as_deref() == Some(task));
        works || w.task_logs.get(task).and_then(|l| l.source.as_ref()) == Some(id)
    });
    own.or_else(|| {
        let mut working = w
            .agent_sessions
            .iter()
            .filter(|s| s.current && s.task.as_deref() == Some(task));
        match (working.next(), working.next()) {
            (Some(only), None) => Some(only.id.clone()),
            _ => None,
        }
    })
}

fn note(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let text = a.text(1)?;
    let task = pick(&w, &a)?.id.clone();
    let session = note_session(&w, &task);
    deliver_edit(
        &a,
        Edit::AddTaskNote {
            task,
            session,
            text,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A CLI names a conversation by its own id, which is not the workbench's.
    #[test]
    fn a_cli_names_a_conversation_by_its_own_id() {
        let mut w = mooring_core::sample::every_state();
        let target = &mut w.agent_sessions[0];
        target.id = "workbench-id".into();
        target.cli_session_id = "cli-id".into();
        let runtime = target.runtime;
        let other = Agent::all().into_iter().find(|a| *a != runtime).unwrap();

        let named =
            |agent, id: &str| session_named(&w, &[(agent, id.to_owned())]).map(|s| s.id.as_str());
        assert_eq!(named(runtime, "cli-id"), Some("workbench-id"));
        assert_eq!(named(runtime, "workbench-id"), None);
        assert_eq!(named(other, "cli-id"), None, "another CLI's id space");
    }
}
