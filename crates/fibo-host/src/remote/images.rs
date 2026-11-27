//! Pictures from a phone. `image` keeps one in the runtime's `phone-images` and answers its
//! path; `send` with `images` pastes each path on its own before the text. Claude Code, Codex
//! and OpenCode each turn a pasted path that is nothing but an image file into an attachment
//! (`[Image #1]`); a path pasted together with words stays words.
use super::{named, text};
use base64::Engine;
use fibo_core::protocol::*;
use fibo_host::pictures::folder;
use serde_json::{json, Value};
use std::io;
use std::path::Path;

/// At most this many in one message.
pub(super) const MAX_IMAGES: usize = 6;

/// `image`: `data`, the picture as base64 (PNG, JPEG, GIF or WebP): `{path}`.
pub(super) fn upload(socket: &Path, request: &Value) -> io::Result<Value> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text(request, "data")?)
        .map_err(|e| io::Error::other(t!("data 不是 base64：{e}", e)))?;
    let path = fibo_host::pictures::save(socket, &bytes)?;
    Ok(json!({ "path": path.to_string_lossy() }))
}

/// The `images` of a `send`: each one a path `upload` answered and still there. Anything
/// else is refused before a key goes in, so a message never arrives half.
pub(super) fn wanted(socket: &Path, request: &Value) -> io::Result<Vec<String>> {
    let Some(list) = request.get("images") else {
        return Ok(vec![]);
    };
    let list = list
        .as_array()
        .ok_or_else(|| io::Error::other(t!("images 需要字符串数组")))?;
    if list.len() > MAX_IMAGES {
        return Err(io::Error::other(t!(
            "一次最多 {MAX_IMAGES} 张图片",
            MAX_IMAGES
        )));
    }
    let dir = folder(socket);
    list.iter()
        .map(|p| {
            let p = p
                .as_str()
                .ok_or_else(|| io::Error::other(t!("images 需要字符串数组")))?;
            let path = Path::new(p);
            let ours = path.parent() == Some(dir.as_path())
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| ["png", "jpg", "gif", "webp"].contains(&e));
            if !ours {
                return Err(io::Error::other(t!("images 只能是 image 回的路径")));
            }
            if !path.is_file() {
                return Err(io::Error::other(t!("图片已经不在电脑上了，重新选一次")));
            }
            Ok(p.to_owned())
        })
        .collect()
}

/// Pastes each picture's path on its own, as a terminal does a dropped file. With the
/// send's `request` id, each paste gets its own, so a send tried again pastes none twice.
pub(super) fn paste(
    socket: &Path,
    request: &Value,
    client: Option<&str>,
    terminal: &str,
    session: &Option<String>,
    images: &[String],
) -> io::Result<()> {
    if images.is_empty() {
        return Ok(());
    }
    let before = super::image_wait::on_screen(socket, terminal);
    for (i, path) in images.iter().enumerate() {
        let paste = Request::SendText {
            session: session.clone(),
            id: terminal.to_owned(),
            text: path.clone(),
            submit: false,
        };
        let mut each = request.clone();
        if let Some(id) = request["request"].as_str() {
            each["request"] = json!(format!("{id}#image{i}"));
        }
        super::ok_reply(rpc(socket, &named(client, &each, paste))?)?;
    }
    // The words go in once the pictures are attachments, or Enter sends them without.
    if let Some(before) = before {
        if !super::image_wait::until_shown(socket, terminal, before, images.len())? {
            eprintln!("terminal {terminal}: pictures not shown in time; sending the words anyway");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kl-images-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_send_names_only_pictures_this_machine_kept() {
        let root = temp();
        let socket = root.join("s.sock");
        let saved = upload(&socket, &json!({ "data": "iVBORw0KGgo=" })).unwrap();
        let path = saved["path"].as_str().unwrap().to_owned();
        assert!(path.ends_with(".png") && Path::new(&path).is_file());
        assert_eq!(
            wanted(&socket, &json!({ "images": [path] })).unwrap().len(),
            1
        );
        assert!(wanted(&socket, &json!({})).unwrap().is_empty());
        for bad in [
            json!({ "images": ["/etc/passwd"] }),
            json!({ "images": [folder(&socket).join("x.png")] }),
            json!({ "images": [folder(&socket).join("../s.sock")] }),
            json!({ "images": "a.png" }),
        ] {
            assert!(wanted(&socket, &bad).is_err(), "{bad}");
        }
        assert!(upload(&socket, &json!({ "data": "IyEvYmluL3No" })).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
