//! `mooring explain`: what the screen rules decided for a terminal and from what.
use super::*;

/// `explain`: replays a saved screen, or a live terminal's, through the
/// screen rules and says what they decided and from what.
pub(super) fn explain(a: Args) -> io::Result<()> {
    use mooring_host::screen_rules;
    let (agent, screen, daemon) = match a.value("file") {
        Some(file) => {
            let name = a.value("agent").ok_or_else(|| {
                io::Error::other(t!(
                    "--file 要配 --agent\n用法：\n{usage}",
                    usage = a.usage()
                ))
            })?;
            let agent = mooring_core::agent::Agent::all()
                .into_iter()
                .find(|g| agent_label(*g) == name.to_lowercase())
                .ok_or_else(|| io::Error::other(t!("不认识的 Agent：{name}", name)))?;
            (agent, std::fs::read_to_string(file)?, None)
        }
        None => {
            let w = workspace(&a.socket)?;
            let id = resolve(&w, a.arg(0, t!("终端或 --file"))?)?;
            let s = w
                .current_agent(&id)
                .ok_or_else(|| io::Error::other(t!("这个终端里没有可识别的 Agent 对话")))?;
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
    println!("{}", t!("状态：{state}", state = e.state()));
    match e.rule() {
        Some(r) => println!("{}", t!("规则：{id}（{name}）", id = r.id, name = r.name)),
        None => println!("{}", t!("规则：没有命中")),
    }
    let source = match &e.rules.source {
        screen_rules::Source::Bundled => t!("内置").to_owned(),
        screen_rules::Source::Override(path) => {
            t!("覆盖文件 {path}", path = path.display())
        }
    };
    println!(
        "{}",
        t!(
            "规则集：{id} {version}，{source}",
            id = e.rules.id,
            version = e.rules.version,
            source
        )
    );
    match &e.found {
        Ok(hit) => {
            println!("{}", t!("问题：{q}", q = hit.question));
            println!("{}", t!("看到的画面："));
            for line in &hit.evidence {
                println!("  {line}");
            }
        }
        Err(misses) => {
            println!("{}", t!("原因：{why}", why = e.fallback()));
            println!("{}", t!("各规则："));
            for (m, r) in misses.iter().zip(&e.rules.rules) {
                println!(
                    "  {}",
                    t!(
                        "{id} {name}：{desc}",
                        id = r.id,
                        name = r.name,
                        desc = m.describe(r)
                    )
                );
            }
        }
    }
    if let Some(c) = daemon {
        let turn = format!("{:?}", c.turn);
        println!(
            "{}",
            t!(
                "守护进程现在：{turn}，依据 {signal}",
                turn,
                signal = c.signal
            )
        );
        for line in c.signal_detail.lines().chain(c.screen_check.lines()) {
            println!("  {line}");
        }
        if !c.shells.is_empty() {
            println!("{}", t!("后台 shell："));
        }
        for sh in &c.shells {
            let kind = if sh.job { t!("任务") } else { t!("服务") };
            println!(
                "  {}",
                t!(
                    "{id} {desc}：{kind}，{why}",
                    id = sh.id,
                    desc = sh.description,
                    kind,
                    why = sh.why
                )
            );
        }
    }
    Ok(())
}
