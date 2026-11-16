//! Handles the launch request.
use super::*;

pub(super) fn dispatch(shared: &Shared, request: Request) -> io::Result<Reply> {
    match request {
        Request::Launch {
            id,
            space,
            argv,
            cwd,
            cols,
            rows,
            scheme,
            colors,
            start,
        } => {
            let colors = colors.unwrap_or(scheme.colors());
            geometry(cols, rows)?;
            let mut host = lock(shared)?;
            // A folder that is gone here (another machine's window asked for it, and only
            // this host can look) starts in its space's folder, else home, as the window
            // does for its own host.
            // A resend is matched by what was asked, which the folder fallback never changes.
            let asked = cwd.clone();
            let cwd = super::start_dir::start_dir(&host.workspace, &space, cwd);
            if let Some(existing) = host.workspace.terminals.iter().find(|s| s.id == id) {
                let same_dir = existing.cwd == asked || existing.cwd == cwd;
                if existing.argv != argv || !same_dir || existing.space != space {
                    return Err(io::Error::other(
                        "launch ID was already used with different arguments",
                    ));
                }
                if existing.pid.is_none() {
                    return Err(io::Error::other(
                        existing.error.as_deref().unwrap_or(t!("终端启动结果未知")),
                    ));
                }
                return Ok(Reply::Workspace(host.snapshot()));
            }
            if id.is_empty()
                || argv.is_empty()
                || argv[0].is_empty()
                || !Path::new(&cwd).is_absolute()
                || !Path::new(&cwd).is_dir()
            {
                return Err(io::Error::other(
                    "launch needs an ID, argv and an absolute existing working directory",
                ));
            }
            if !host.workspace.spaces.iter().any(|s| s.id == space) {
                return Err(io::Error::other("space not found"));
            }
            let mut vt = Vt::new(cols, rows)?;
            vt.set_colors(colors)?;
            let replies = Arc::new(Mutex::new(Vec::new()));
            let out = Arc::clone(&replies);
            vt.on_reply(move |bytes| {
                if let Ok(mut out) = out.lock() {
                    out.extend_from_slice(bytes);
                }
            })?;
            let cell = CellPixels::default();
            vt.answer_queries(cell.clone())?;
            let mut next = host.workspace.clone();
            next.terminals.push(Terminal {
                id: id.clone(),
                space,
                argv: argv.clone(),
                cwd: cwd.clone(),
                pid: None,
                state: ProcessState::Starting,
                exit_code: None,
                error: None,
            });
            if let Some(start) = start {
                let at = fibo_core::content::now();
                next.agent_starts
                    .insert(id.clone(), AgentStart { at, ..start });
            }
            host.commit(next)?;
            let (mut child, master) =
                match pty::spawn(&argv, Path::new(&cwd), cols, rows, &id, colors.colorfgbg()) {
                    Ok(result) => result,
                    Err(error) => {
                        let mut next = host.workspace.clone();
                        let session = next.terminals.last_mut().unwrap();
                        session.state = ProcessState::Failed;
                        session.error = Some(error.to_string());
                        host.commit(next)?;
                        return Err(error);
                    }
                };
            let reader = match master.reader() {
                Ok(reader) => reader,
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e);
                }
            };
            let pid = child.id();
            let mut next = host.workspace.clone();
            let session = next.terminals.last_mut().unwrap();
            session.pid = Some(pid);
            session.state = ProcessState::Running;
            if let Err(e) = host.commit(next) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
            let started = host
                .workspace
                .agent_starts
                .contains_key(&id)
                .then(|| id.clone());
            run_live(
                &mut host,
                shared,
                id,
                Live {
                    vt,
                    master,
                    cols,
                    rows,
                    pid,
                    owner: None,
                    subscribers: HashMap::new(),
                    watchers: Default::default(),
                    exited: false,
                    drained: false,
                    status: None,
                    exit_code: None,
                    title: String::new(),
                    cell,
                    input_at: None,
                    marks: Default::default(),
                    reports: Default::default(),
                },
                reader,
                replies,
                Reaper::Child(child),
            );
            if let Some(id) = started {
                super::launch_probe::look_for_started(shared, &id);
            }
            Ok(Reply::Workspace(host.snapshot()))
        }
        _ => unreachable!("request routed by client::dispatch"),
    }
}
