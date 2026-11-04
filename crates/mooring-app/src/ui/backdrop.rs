//! A theme's picture and how the window's material follows the theme. A picture is decoded
//! once off the window's thread, however many themes or strengths use it; drawing reads the
//! finished image, or paints plain ground until there is one.
use super::{status_colors::config_dir, theme::Theme};
use gpui::{App, Bounds, Hsla, Pixels, RenderImage, WindowBackgroundAppearance, point, px, size};
use image::imageops::FilterType;
use mooring_core::theme::{BackgroundImage, ImageMode};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// Past this on its long side a picture is scaled down: a texture larger than a 5K window
/// only costs memory.
const LONGEST_SIDE: u32 = 2880;

pub struct Picture {
    pub path: PathBuf,
    decoded: OnceLock<Result<Arc<RenderImage>, String>>,
    started: AtomicBool,
}
impl Picture {
    /// Kept for the life of the process, one per path, as palettes are.
    fn of(path: PathBuf) -> &'static Self {
        static KEPT: Mutex<Vec<&'static Picture>> = Mutex::new(Vec::new());
        let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = kept.iter().find(|p| p.path == path) {
            return found;
        }
        let picture: &'static Self = Box::leak(Box::new(Self {
            path,
            decoded: OnceLock::new(),
            started: AtomicBool::new(false),
        }));
        kept.push(picture);
        picture
    }
    pub fn image(&self) -> Option<&Arc<RenderImage>> {
        self.decoded.get()?.as_ref().ok()
    }
    /// Why it cannot be shown, once that is known.
    pub fn error(&self) -> Option<&str> {
        self.decoded.get()?.as_ref().err().map(String::as_str)
    }
}
#[derive(Clone, Copy)]
pub struct Backdrop {
    pub picture: &'static Picture,
    pub mode: ImageMode,
    pub strength: u8,
}
impl PartialEq for Backdrop {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.picture, other.picture)
            && self.mode == other.mode
            && self.strength == other.strength
    }
}
impl Eq for Backdrop {}
impl std::fmt::Debug for Backdrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Backdrop({}, {:?}, {})",
            self.picture.path.display(),
            self.mode,
            self.strength
        )
    }
}
impl Backdrop {
    pub fn of(image: &BackgroundImage) -> Self {
        Self {
            picture: Picture::of(resolve(&image.path)),
            mode: image.mode,
            strength: image.strength,
        }
    }
    /// How opaque the terminal's ground is over the picture.
    pub fn ground(self) -> f32 {
        1. - f32::from(self.strength) / 100.
    }
}
/// A relative path is in the themes folder, so a theme and its picture move together.
pub fn resolve(path: &str) -> PathBuf {
    let path = Path::new(path);
    match config_dir() {
        Some(config) if path.is_relative() => config.join("themes").join(path),
        _ => path.to_owned(),
    }
}
fn decode(path: &Path) -> Result<Arc<RenderImage>, String> {
    let picture = image::ImageReader::open(path)
        .map_err(|e| format!("读不了 {}：{e}", path.display()))?
        .with_guessed_format()
        .map_err(|e| format!("读不了 {}：{e}", path.display()))?
        .decode()
        .map_err(|e| format!("{} 不是能显示的图片：{e}", path.display()))?;
    let picture = if picture.width().max(picture.height()) > LONGEST_SIDE {
        picture.resize(LONGEST_SIDE, LONGEST_SIDE, FilterType::Triangle)
    } else {
        picture
    };
    let mut pixels = picture.into_rgba8();
    // GPUI's textures are BGRA.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}
/// Decode the current theme's picture if nothing has yet, then repaint.
fn load(cx: &mut App) {
    let Some(backdrop) = Theme::palette().backdrop else {
        return;
    };
    let picture = backdrop.picture;
    if picture.started.swap(true, Ordering::Relaxed) {
        return;
    }
    cx.spawn(async move |cx| {
        let done = cx
            .background_executor()
            .spawn(async move { decode(&picture.path) })
            .await;
        if let Err(error) = &done {
            eprintln!("mooring: {error}");
        }
        let _ = picture.decoded.set(done);
        cx.update(|cx| cx.refresh_windows());
    })
    .detach();
}
impl Theme {
    /// The window's ground. Under a see-through theme this one layer carries the tint and
    /// everything above it (`surface`) stays clear, so layers never add up to opaque.
    pub fn base() -> Hsla {
        let p = Self::palette();
        match p.opacity {
            100 => Self::color(p.app),
            percent => Self::color(p.term.bg).opacity(f32::from(percent) / 100.),
        }
    }
    /// A large surface on the window's ground: its color, or nothing under a see-through theme.
    pub fn surface(color: u32) -> Hsla {
        if Self::palette().opacity == 100 {
            Self::color(color)
        } else {
            gpui::transparent_black()
        }
    }
}
/// What the platform window should be for the current theme.
pub fn window_background() -> WindowBackgroundAppearance {
    let p = Theme::palette();
    match (p.opacity, p.blur) {
        (100, _) => WindowBackgroundAppearance::Opaque,
        (_, true) => WindowBackgroundAppearance::Blurred,
        (_, false) => WindowBackgroundAppearance::Transparent,
    }
}
/// Bring every window and the picture in line with a theme just installed.
pub fn sync(cx: &mut App) {
    load(cx);
    let wanted = window_background();
    for window in cx.windows() {
        let _ = window.update(cx, |_, window, _| window.set_background_appearance(wanted));
    }
}
/// Where the picture goes for a terminal, in the terminal's own coordinates; `natural` is
/// its size at one image pixel per device pixel. Clipped to the pane by the caller.
pub fn placements(
    mode: ImageMode,
    natural: gpui::Size<Pixels>,
    pane: Bounds<Pixels>,
) -> Vec<Bounds<Pixels>> {
    let (w, h) = (f32::from(natural.width), f32::from(natural.height));
    let (pw, ph) = (f32::from(pane.size.width), f32::from(pane.size.height));
    if w <= 0. || h <= 0. || pw <= 0. || ph <= 0. {
        return vec![];
    }
    let scaled = |scale: f32| {
        let (sw, sh) = (w * scale, h * scale);
        let origin = pane.origin + point(px((pw - sw) / 2.), px((ph - sh) / 2.));
        vec![Bounds::new(origin, size(px(sw), px(sh)))]
    };
    match mode {
        ImageMode::Fill => scaled((pw / w).max(ph / h)),
        ImageMode::Fit => scaled((pw / w).min(ph / h)),
        ImageMode::Stretch => vec![pane],
        ImageMode::Tile => {
            let (cols, rows) = ((pw / w).ceil() as usize, (ph / h).ceil() as usize);
            (0..rows)
                .flat_map(|row| {
                    (0..cols).map(move |col| {
                        Bounds::new(
                            pane.origin + point(px(w * col as f32), px(h * row as f32)),
                            natural,
                        )
                    })
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pane() -> Bounds<Pixels> {
        Bounds::new(point(px(10.), px(20.)), size(px(400.), px(200.)))
    }
    #[test]
    fn each_mode_places_the_picture() {
        let natural = size(px(100.), px(100.));
        let fill = placements(ImageMode::Fill, natural, pane());
        assert_eq!(fill[0].size, size(px(400.), px(400.)));
        assert_eq!(fill[0].origin, point(px(10.), px(-80.)));
        let fit = placements(ImageMode::Fit, natural, pane());
        assert_eq!(fit[0].size, size(px(200.), px(200.)));
        assert_eq!(fit[0].origin, point(px(110.), px(20.)));
        assert_eq!(
            placements(ImageMode::Stretch, natural, pane()),
            vec![pane()]
        );
        let tile = placements(ImageMode::Tile, natural, pane());
        assert_eq!(tile.len(), 8);
        assert_eq!(tile[7].origin, point(px(310.), px(120.)));
    }
    #[test]
    fn nothing_to_place_is_not_a_panic() {
        let none = size(px(0.), px(0.));
        assert!(placements(ImageMode::Tile, none, pane()).is_empty());
    }
    #[test]
    fn one_path_is_decoded_once_whatever_its_strength() {
        let image = |strength| BackgroundImage {
            path: "/nowhere/sea.png".into(),
            mode: ImageMode::Fill,
            strength,
        };
        let (a, b) = (Backdrop::of(&image(20)), Backdrop::of(&image(40)));
        assert!(std::ptr::eq(a.picture, b.picture));
        assert_ne!(a, b);
        assert_eq!(a, Backdrop::of(&image(20)));
    }
    #[test]
    fn a_picture_comes_out_bgra_and_bounded() {
        let dir = std::env::temp_dir().join(format!("mooring-backdrop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wide.png");
        image::RgbaImage::from_pixel(4000, 100, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .unwrap();
        let decoded = decode(&path).unwrap();
        let size = decoded.size(0);
        assert_eq!(size.width.0, LONGEST_SIDE as i32);
        assert_eq!(&decoded.as_bytes(0).unwrap()[..4], &[30, 20, 10, 255]);
        std::fs::write(dir.join("bad.png"), "not a picture").unwrap();
        assert!(decode(&dir.join("bad.png")).is_err());
        assert!(decode(&dir.join("gone.png")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
