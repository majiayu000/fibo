//! Pictures for a terminal on another computer. Ctrl+V would have its Agent read that
//! computer's clipboard, and a dropped path names a file only here, so each picture goes
//! to that host first ([`Request::SaveImage`]) and the path it answers is pasted, as a
//! phone's picture is. The Agent there turns that path into an attachment.
use super::Pane;
use base64::Engine;
use fibo_core::protocol::{Request, rpc};
use gpui::{Context, prelude::*};
use std::path::PathBuf;

/// Base64 grows a picture by a third, and one request frame holds 16 MB.
const MAX_BYTES: usize = 11 * 1024 * 1024;

pub(super) enum Picture {
    /// Off the clipboard.
    Bytes(Vec<u8>),
    /// A file here, dropped on the pane.
    File(PathBuf),
}

impl Pane {
    /// Sends the pictures to this pane's host, then pastes their paths there; nothing is
    /// pasted when one of them can't go.
    pub(super) fn send_pictures(&mut self, pictures: Vec<Picture>, cx: &mut Context<Self>) {
        let socket = self.socket.clone();
        self.local_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_spawn(async move {
                    (pictures.into_iter())
                        .map(|p| upload(&socket, p))
                        .collect::<Result<Vec<_>, String>>()
                })
                .await;
            this.update(cx, |pane, cx| match sent {
                Ok(paths) => {
                    pane.prefill(&crate::terminal::terminal_input::dropped_paths(&paths), cx);
                }
                Err(e) => {
                    pane.local_error = Some(e);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

fn upload(socket: &std::path::Path, picture: Picture) -> Result<PathBuf, String> {
    let bytes = match picture {
        Picture::Bytes(bytes) => bytes,
        Picture::File(path) => {
            std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?
        }
    };
    let bytes = as_sent(bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(t!("图片超过 11 MB，传不到另一台电脑").into());
    }
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    match rpc(socket, &Request::SaveImage { data }) {
        Ok(fibo_core::protocol::Reply::Text(path)) => Ok(PathBuf::from(path)),
        Ok(other) => Err(format!("unexpected reply: {other:?}")),
        Err(e) => Err(refusal(&e.to_string())),
    }
}

/// A host built before `SaveImage` can't read the request at all.
fn refusal(error: &str) -> String {
    if error.starts_with("unsupported or invalid request") {
        t!("那台电脑上的 Fibo 太旧，还收不了图片；更新它之后再试").into()
    } else {
        error.to_owned()
    }
}

/// PNG, JPEG, GIF and WebP go as they are (the host keeps only those); another picture,
/// such as the TIFF some apps copy, goes as PNG. Anything else isn't sent.
fn as_sent(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    use image::ImageFormat as F;
    let not_a_picture =
        || t!("只有图片能传到另一台电脑；其他文件请先上传，再输入那台电脑上的路径").to_owned();
    match image::guess_format(&bytes) {
        Ok(F::Png | F::Jpeg | F::Gif | F::WebP) => Ok(bytes),
        Ok(_) => {
            let decoded = image::load_from_memory(&bytes).map_err(|_| not_a_picture())?;
            let mut png = std::io::Cursor::new(Vec::new());
            decoded
                .write_to(&mut png, F::Png)
                .map_err(|e| e.to_string())?;
            Ok(png.into_inner())
        }
        Err(_) => Err(not_a_picture()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_the_host_keeps_go_as_they_are_and_others_as_png() {
        let png = b"\x89PNG\r\n\x1a\n rest".to_vec();
        assert_eq!(as_sent(png.clone()).unwrap(), png);
        let mut tiff = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(2, 2)
            .write_to(&mut tiff, image::ImageFormat::Tiff)
            .unwrap();
        let sent = as_sent(tiff.into_inner()).unwrap();
        assert!(sent.starts_with(b"\x89PNG"));
        assert!(as_sent(b"%PDF-1.7".to_vec()).is_err());
        assert!(as_sent(b"fn main() {}".to_vec()).is_err());
    }

    #[test]
    fn an_old_host_is_named_as_too_old() {
        assert_ne!(
            refusal("unsupported or invalid request: unknown variant `SaveImage`"),
            "unsupported or invalid request: unknown variant `SaveImage`"
        );
        assert_eq!(refusal("disk full"), "disk full");
    }
}
