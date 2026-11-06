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
            "待办只由用户在窗口里标记完成，Agent 不能标；把结果写成 mooring todo note，等用户看",
        )),
        other => Err(io::Error::other(format!(
            "没有 mooring todo {other}\n用法：\n{}",
            a.usage
        ))),
    }
}

fn own_terminal() -> Option<String> {
    std::env::var("MOORING_TERMINAL").ok()
}
/// The conversation running in the calling terminal, if there is one.
fn own_session(w: &Workspace) -> Option<String> {
    w.current_agent(&own_terminal()?).map(|s| s.id.clone())
}
/// `--task`: a todo's id, the start of it, or its exact title. Without it, the todo the
/// calling terminal works for.
fn pick<'a>(w: &'a Workspace, a: &Args) -> io::Result<&'a Task> {
    let Some(needle) = a.value("task") else {
        return own_terminal()
            .and_then(|t| w.terminal_task(&t))
            .ok_or_else(|| {
                io::Error::other(
                    "这个终端没有关联待办：用 --task <待办> 指定，mooring todo list 能看编号",
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
/// `--space`, else the calling terminal's space, else the only space there is.
fn pick_space(w: &Workspace, a: &Args) -> io::Result<String> {
    if let Some(needle) = a.value("space") {
        return w
            .spaces
            .iter()
            .find(|s| s.id == needle || s.name == needle)
            .map(|s| s.id.clone())
            .ok_or_else(|| io::Error::other(format!("没有这个空间：{needle}")));
    }
    if let Some(tab) = own_terminal().and_then(|t| w.terminal_tab(&t).cloned()) {
        return Ok(tab.space);
    }
    match w.spaces.as_slice() {
        [only] => Ok(only.id.clone()),
        _ => Err(io::Error::other(
            "不在 Mooring 终端里，用 --space <空间> 说记到哪",
        )),
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
    }
    for task in shown {
        let last = w
            .task_logs
            .get(&task.id)
            .and_then(|log| log.notes.last())
            .map_or(String::new(), |n| format!("  {}", n.text));
        println!(
            "{}  {}  {}  {}{last}",
            short_id(&task.id),
            task_state(&w, task, now).label(),
            space_name(&w, &task.space),
            task.title,
        );
    }
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
        short_id(&task.id),
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
    println!("{}", short_id(&id));
    Ok(())
}

fn note(a: Args) -> io::Result<()> {
    let w = workspace(&a.socket)?;
    let text = a.text(1)?;
    let task = pick(&w, &a)?.id.clone();
    let session = own_session(&w);
    deliver_edit(
        &a,
        Edit::AddTaskNote {
            task,
            session,
            text,
        },
    )
}
