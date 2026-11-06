//! `mooring explain`: what the screen rules decided for a terminal and from what.
use super::*;

/// `explain`: replays a saved screen, or a live terminal's, through the
/// screen rules and says what they decided and from what.
pub(super) fn explain(a: Args) -> io::Result<()> {
    use mooring_host::screen_rules;
    let (agent, screen, daemon) = match a.value("file") {
        Some(file) => {
            let name = a.value("agent").ok_or_else(|| {
                io::Error::other(format!("--file 要配 --agent\n用法：\n{}", a.usage))
            })?;
            let agent = mooring_core::agent::Agent::all()
                .into_iter()
                .find(|g| agent_label(*g) == name.to_lowercase())
                .ok_or_else(|| io::Error::other(format!("不认识的 Agent：{name}")))?;
            (agent, std::fs::read_to_string(file)?, None)
        }
        None => {
            let w = workspace(&a.socket)?;
            let id = resolve(&w, a.arg(0, "终端或 --file")?)?;
            let s = w
                .current_agent(&id)
                .ok_or_else(|| io::Error::other("这个终端里没有可识别的 Agent 对话"))?;
            // The daemon reads the visible rows; 200 covers the tallest screen it allows.
            let screen = match rpc(&a.socket, &Request::ReadScreen { id, rows: 200 })? {
                Reply::Text(text) => text,
                other => return Err(io::Error::other(format!("unexpected reply: {other:?}"))),
            };
            (s.runtime, screen, w.content.get(&s.id).cloned())
        }
    };
    let e = screen_rules::explain(screen_rules::rules(agent), &screen);
    if a.has("json") {
        let mut json = e.to_json();
        if let Some(c) = &daemon {
            json["daemon"] = serde_json::json!({
                // TODO: this says Running/Waiting/Completed (the content turn) while
                // `list` says working/waiting/idle (`Phase::name`); settle on one wording
                // when `phase` is reworked.
                "phase": format!("{:?}", c.turn),
                "signal": c.signal,
                "signal_detail": c.signal_detail,
                "screen_check": c.screen_check,
                "shells": c.shells,
            });
        }
        let text = serde_json::to_string_pretty(&json).map_err(io::Error::other)?;
        println!("{text}");
        return Ok(());
    }
    println!("状态：{}", e.state());
    match e.rule() {
        Some(r) => println!("规则：{}（{}）", r.id, r.name),
        None => println!("规则：没有命中"),
    }
    let source = match &e.rules.source {
        screen_rules::Source::Bundled => "内置".to_owned(),
        screen_rules::Source::Override(path) => format!("覆盖文件 {}", path.display()),
    };
    println!("规则集：{} {}，{source}", e.rules.id, e.rules.version);
    match &e.found {
        Ok(hit) => {
            println!("问题：{}", hit.question);
            println!("看到的画面：");
            for line in &hit.evidence {
                println!("  {line}");
            }
        }
        Err(misses) => {
            println!("原因：{}", e.fallback());
            println!("各规则：");
            for (m, r) in misses.iter().zip(&e.rules.rules) {
                println!("  {} {}：{}", r.id, r.name, m.describe(r));
            }
        }
    }
    if let Some(c) = daemon {
        println!("守护进程现在：{:?}，依据 {}", c.turn, c.signal);
        for line in c.signal_detail.lines().chain(c.screen_check.lines()) {
            println!("  {line}");
        }
        if !c.shells.is_empty() {
            println!("后台 shell：");
        }
        for t in &c.shells {
            let kind = if t.job { "任务" } else { "服务" };
            println!("  {} {}：{kind}，{}", t.id, t.description, t.why);
        }
    }
    Ok(())
}
