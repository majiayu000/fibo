//! Another computer on the relay (docs/decisions/2026-11-27-computers-over-relay.md): what
//! this machine tells one first, and on the computer that reaches it, its pairing kept and
//! its window's machine written down.
//!
//! - `fibo remote pair --relay <url> --computer --name <它的名字>` here pairs one, with full
//!   control, as `ssh` here would give.
//! - `fibo remote join '<link>' [--name <这台的名字>]` there keeps the link, readable only by
//!   that user, and adds this machine to its `machines.json` reached by the relay.
//! - Its window then runs `fibo remote dial` (`dial.rs`) for this machine's socket.
use fibo_host::devices;
use fibo_remote::link::Link;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::path::{Path, PathBuf};

/// What a computer is told when its session opens: what `ssh` asks a machine before its
/// window forwards to it (attach.rs `remote_socket`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct About {
    pub home: String,
    pub shell: String,
    pub machine: String,
    pub name: String,
}

/// This machine's [`About`], one JSON line.
pub(super) fn about(socket: &Path) -> Vec<u8> {
    let machine = devices::machine(socket).ok();
    let about = About {
        home: fibo_platform::dirs::home_str().unwrap_or_default(),
        shell: fibo_platform::shell::interactive(),
        machine: machine.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
        name: machine.map(|m| m.name).unwrap_or_default(),
    };
    let mut line = serde_json::to_vec(&about).unwrap_or_default();
    line.push(b'\n');
    line
}

/// Where the link to the machine called `host` in machines.json is kept.
pub(super) fn link_file(host: &str) -> io::Result<PathBuf> {
    let dir = fibo_core::agent::config_dir()
        .ok_or_else(|| io::Error::other(t!("找不到配置目录")))?
        .join("relay-links");
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_alphanumeric() || "-_.@".contains(c))
    {
        return Err(io::Error::other(t!(
            "电脑名只能有字母、数字和 - _ . @：{host}",
            host
        )));
    }
    Ok(dir.join(format!("{host}.link")))
}

pub(super) fn load_link(host: &str) -> io::Result<Link> {
    let file = link_file(host)?;
    let text = std::fs::read_to_string(&file).map_err(|e| {
        io::Error::other(t!(
            "没有 {host} 的中继配对（{e}）：先在那台电脑上运行 fibo remote pair --computer，再在这里 fibo remote join",
            host,
            e
        ))
    })?;
    Link::parse(&text).map_err(io::Error::other)
}

/// `fibo remote join <link> [--name <名字>]`: the name it is kept by, and whether it now
/// has two ways (it was already reached by ssh).
pub(super) fn join(text: &str, name: Option<&str>) -> io::Result<(String, bool)> {
    let link = Link::parse(text).map_err(|e| io::Error::other(t!("配对链接不对：{e}", e)))?;
    let host = name
        .map(str::to_owned)
        .unwrap_or_else(|| link.name.clone())
        .trim()
        .replace(' ', "-");
    let file = link_file(&host)?;
    fibo_platform::fs::create_private_dir(file.parent().expect("a folder"))?;
    fibo_platform::fs::write_private(&file, link.text().as_bytes())?;
    let machines = fibo_core::agent::config_dir()
        .ok_or_else(|| io::Error::other(t!("找不到配置目录")))?
        .join("machines.json");
    let both = write_machine(&machines, &host)?;
    Ok((host, both))
}

/// Adds `host` to machines.json reached by the relay, keeping what else it says; a file
/// that cannot be read is left as it is. One already reached by ssh keeps ssh and gets
/// both ways (`auto`, docs/multi-machine/routes.md): true then.
fn write_machine(file: &Path, host: &str) -> io::Result<bool> {
    let mut list: Vec<Value> = match std::fs::read(file) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            io::Error::other(t!("{display} 格式不对：{e}", display = file.display(), e))
        })?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(e),
    };
    let both = match list.iter_mut().find(|m| m["host"] == host) {
        Some(m) if m["via"] != "relay" => {
            m["auto"] = true.into();
            true
        }
        Some(_) => false,
        None => {
            list.push(serde_json::json!({ "host": host, "via": "relay" }));
            false
        }
    };
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    fibo_platform::fs::write_atomic(file, &serde_json::to_vec_pretty(&list)?)?;
    Ok(both)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_adds_the_machine_once_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join(format!("fibo-join-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("machines.json");
        std::fs::write(&file, r#"[{"host":"mac2","quiet":true}]"#).unwrap();
        assert!(!write_machine(&file, "apple").unwrap());
        assert!(!write_machine(&file, "apple").unwrap());
        // One reached by ssh keeps it and gets both ways.
        assert!(write_machine(&file, "mac2").unwrap());
        let list: Vec<Value> = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            list[0]["quiet"], true,
            "the other machine is kept as it was"
        );
        assert_eq!(list[1]["via"], "relay");
        assert_eq!(list[0]["via"], Value::Null, "ssh, as before");
        assert_eq!(list[0]["auto"], true);
        assert_eq!(list[1]["auto"], Value::Null);
        std::fs::write(&file, "[{").unwrap();
        assert!(write_machine(&file, "apple").is_err());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[{",
            "never written over"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_name_cannot_leave_the_links_folder() {
        assert!(link_file("../x").is_err());
        assert!(link_file("").is_err());
        assert!(link_file("apple").is_ok());
    }
}
