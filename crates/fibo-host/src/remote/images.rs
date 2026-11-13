//! Pictures from a phone. `image` keeps one in the runtime's `phone-images` and answers its
//! path; `send` with `images` pastes each path on its own before the text. Claude Code, Codex
//! and OpenCode each turn a pasted path that is nothing but an image file into an attachment
//! (`[Image #1]`); a path pasted together with words stays words.
use super::{named, text};
use base64::Engine;
use fibo_core::protocol::*;
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Kept this long, then removed by the next upload: an Agent reads it when the message goes
/// in, and a conversation picked up later may look again.
const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
/// At most this many in one message.
pub(super) const MAX_IMAGES: usize = 6;

pub(super) fn folder(socket: &Path) -> PathBuf {
    socket.with_file_name("phone-images")
}

/// The file type its first bytes say, whatever the phone called it.
fn kind(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        _ => None,
    }
}

/// `image`: `data`, the picture as base64 (PNG, JPEG, GIF or WebP): `{path}`.
pub(super) fn upload(socket: &Path, request: &Value) -> io::Result<Value> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text(request, "data")?)
        .map_err(|e| io::Error::other(format!("data 不是 base64：{e}")))?;
    let ext = kind(&bytes).ok_or_else(|| io::Error::other("只收 PNG、JPEG、GIF 或 WebP 图片"))?;
    let dir = folder(socket);
    // Its own, or refused: a link or a folder others may enter is not where a picture goes.
    fibo_platform::fs::create_private_dir(&dir)?;
    fibo_platform::fs::secure_dir(&dir)?;
    prune(&dir, SystemTime::now());
    let path = dir.join(format!("{}.{ext}", uuid::Uuid::new_v4()));
    fibo_platform::fs::write_private(&path, &bytes)?;
    Ok(json!({ "path": path.to_string_lossy() }))
}

fn prune(dir: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = (entry.metadata().and_then(|m| m.modified()))
            .is_ok_and(|t| now.duration_since(t).is_ok_and(|age| age > KEEP));
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// The `images` of a `send`: each one a path `upload` answered and still there. Anything
/// else is refused before a key goes in, so a message never arrives half.
pub(super) fn wanted(socket: &Path, request: &Value) -> io::Result<Vec<String>> {
    let Some(list) = request.get("images") else {
        return Ok(vec![]);
    };
    let list = list
        .as_array()
        .ok_or_else(|| io::Error::other("images 需要字符串数组"))?;
    if list.len() > MAX_IMAGES {
        return Err(io::Error::other(format!("一次最多 {MAX_IMAGES} 张图片")));
    }
    let dir = folder(socket);
    list.iter()
        .map(|p| {
            let p = p
                .as_str()
                .ok_or_else(|| io::Error::other("images 需要字符串数组"))?;
            let path = Path::new(p);
            let ours = path.parent() == Some(dir.as_path())
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| ["png", "jpg", "gif", "webp"].contains(&e));
            if !ours {
                return Err(io::Error::other("images 只能是 image 回的路径"));
            }
            if !path.is_file() {
                return Err(io::Error::other("图片已经不在电脑上了，重新选一次"));
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kl-images-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_first_bytes_name_the_type_not_the_phone() {
        assert_eq!(kind(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(kind(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(kind(b"#!/bin/sh\n"), None);
        assert_eq!(kind(b""), None);
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

    #[test]
    fn a_week_old_picture_goes_with_the_next_upload() {
        let root = temp();
        let file = root.join("a.png");
        fs::write(&file, b"x").unwrap();
        prune(&root, SystemTime::now() + Duration::from_secs(3600));
        assert!(file.exists());
        prune(&root, SystemTime::now() + KEEP + Duration::from_secs(60));
        assert!(!file.exists());
        let _ = fs::remove_dir_all(root);
    }
}
