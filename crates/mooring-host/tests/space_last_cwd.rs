//! A space with no folder of its own remembers where its terminals were when one was
//! closed, across a host restart; a folder set on the space still wins.
#[path = "support/tied.rs"]
mod tied;
use mooring_core::{layout::Layout, model::*, protocol::*};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tied::SpawnTied;

struct Host {
    child: tied::Tied,
    socket: PathBuf,
}
impl Host {
    fn start(root: &Path) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "last_cwd_daemon", "--nocapture"])
            .env("MOORING_FIXTURE_RUNTIME", root)
            .env("MOORING_REMOTE_EXE", env!("CARGO_BIN_EXE_mooring"))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn_tied()
            .unwrap();
        let socket = root.join("workbench.sock");
        let until = Instant::now() + Duration::from_secs(30);
        while rpc(&socket, &Request::Workspace).is_err() {
            assert!(Instant::now() < until, "host never listened");
            std::thread::sleep(Duration::from_millis(20));
        }
        Self { child, socket }
    }
    fn edit(&self, edit: Edit) {
        rpc(
            &self.socket,
            &Request::Edit {
                request: None,
                edit,
            },
        )
        .unwrap();
    }
    fn workspace(&self) -> Workspace {
        match rpc(&self.socket, &Request::Workspace).unwrap() {
            Reply::Workspace(w) => w,
            other => panic!("{other:?}"),
        }
    }
    /// A shell in `space` that moved into `dir` after it started in `start`, in a tab.
    fn open(&self, space: &str, id: &str, start: &Path, dir: &Path) {
        let script = format!("cd '{}' && exec sleep 30", dir.display());
        let launch = Request::Launch {
            id: id.into(),
            space: space.into(),
            argv: vec!["/bin/sh".into(), "-c".into(), script],
            cwd: start.display().to_string(),
            cols: 80,
            rows: 24,
            scheme: Default::default(),
            colors: None,
        };
        rpc(&self.socket, &launch).unwrap();
        self.edit(Edit::CreateTab {
            tab: Tab {
                id: format!("tab-{id}"),
                space: space.into(),
                default_task: None,
                title: String::new(),
                layout: Some(Layout::Leaf(id.into())),
            },
        });
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.child.kill_tree();
        let _ = fs::remove_file(&self.socket);
    }
}
fn space(id: &str, cwd: &str) -> Edit {
    Edit::AddSpace(Space {
        id: id.into(),
        name: id.into(),
        cwd: cwd.into(),
    })
}
fn same(a: &str, b: &Path) -> bool {
    fs::canonicalize(a).ok() == fs::canonicalize(b).ok()
}

#[test]
fn closing_a_terminal_remembers_where_an_unpinned_space_was() {
    let root = std::env::temp_dir().join(format!("kl-lc-{}", uuid::Uuid::new_v4().simple()));
    let work = std::env::temp_dir().join(format!("kl-lcw-{}", uuid::Uuid::new_v4().simple()));
    let (proj, sub) = (work.join("proj"), work.join("proj/sub"));
    fs::create_dir_all(&sub).unwrap();
    let h = Host::start(&root);
    h.edit(space("free", ""));
    h.edit(space("pinned", &proj.display().to_string()));
    assert_eq!(
        h.workspace().space_start_dir("free"),
        "",
        "nothing remembered yet"
    );
    h.open("free", "a", &proj, &sub);
    h.open("pinned", "b", &proj, &sub);
    // The shell's cd has to land before the close reads it.
    let until = Instant::now() + Duration::from_secs(10);
    while !matches!(
        rpc(&h.socket, &Request::TerminalCwd { id: "a".into() }),
        Ok(Reply::Cwd(d)) if same(&d, &sub)
    ) {
        assert!(Instant::now() < until, "shell never moved");
        std::thread::sleep(Duration::from_millis(20));
    }
    h.edit(Edit::ClosePane {
        terminal: "a".into(),
    });
    h.edit(Edit::ClosePane {
        terminal: "b".into(),
    });
    let w = h.workspace();
    assert!(
        same(w.space_start_dir("free"), &sub),
        "{:?}",
        w.space_last_cwd
    );
    assert!(
        same(w.space_start_dir("pinned"), &proj),
        "the space's own folder wins"
    );
    drop(h);
    // Remembered across a restart, and gone with the space.
    let h = Host::start(&root);
    assert!(same(h.workspace().space_start_dir("free"), &sub));
    h.edit(Edit::RemoveSpace { id: "free".into() });
    assert!(!h.workspace().space_last_cwd.contains_key("free"));
    drop(h);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&work);
}

#[test]
#[ignore = "subprocess daemon entry for this file's tests"]
fn last_cwd_daemon() {
    let root = PathBuf::from(std::env::var_os("MOORING_FIXTURE_RUNTIME").unwrap());
    mooring_host::serve_with_roots(&root, vec![(Agent::Codex, root.join("logs/codex"))]).unwrap();
}
