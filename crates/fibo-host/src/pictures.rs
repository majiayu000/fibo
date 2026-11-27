//! Pictures sent to this machine's Agents from elsewhere: a phone (`fibo remote`'s `image`)
//! or another computer's window ([`fibo_core::protocol::Request::SaveImage`]). Each is kept
//! beside the host's socket and its path pasted; Claude Code, Codex and OpenCode turn a
//! pasted path that is nothing but an image file into an attachment.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Kept this long, then removed by the next upload: an Agent reads it when the message goes
/// in, and a conversation picked up later may look again.
const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);

/// The folder kept pictures go in. Named for phones, which used it first; paths in it that
/// phones were given still have to be found there.
pub fn folder(socket: &Path) -> PathBuf {
    socket.with_file_name("phone-images")
}

/// The file type its first bytes say, whatever the sender called it.
pub fn kind(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        _ => None,
    }
}

/// Keeps a PNG, JPEG, GIF or WebP and answers its path.
pub fn save(socket: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    let ext =
        kind(bytes).ok_or_else(|| io::Error::other(t!("只收 PNG、JPEG、GIF 或 WebP 图片")))?;
    let dir = folder(socket);
    // Its own, or refused: a link or a folder others may enter is not where a picture goes.
    fibo_platform::fs::create_private_dir(&dir)?;
    fibo_platform::fs::secure_dir(&dir)?;
    prune(&dir, SystemTime::now());
    let path = dir.join(format!("{}.{ext}", uuid::Uuid::new_v4()));
    fibo_platform::fs::write_private(&path, bytes)?;
    Ok(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_bytes_name_the_type_not_the_sender() {
        assert_eq!(kind(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(kind(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(kind(b"#!/bin/sh\n"), None);
        assert_eq!(kind(b""), None);
    }

    #[test]
    fn a_week_old_picture_goes_with_the_next_upload() {
        let root = std::env::temp_dir().join(format!("kl-images-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("a.png");
        fs::write(&file, b"x").unwrap();
        prune(&root, SystemTime::now() + Duration::from_secs(3600));
        assert!(file.exists());
        prune(&root, SystemTime::now() + KEEP + Duration::from_secs(60));
        assert!(!file.exists());
        let _ = fs::remove_dir_all(root);
    }
}
