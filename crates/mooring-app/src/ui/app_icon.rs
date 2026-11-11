//! The app's looks, the Fibo mark (the default since 2026-11-11), the mooring ring, anime portrait and mascot: the title-bar image and the Dock tile.
//! The choice belongs to this machine's client, not to the workspace.
use gpui::{Global, Image, ImageFormat};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppIcon {
    /// The f and its gold dot on the dusk gradient: the brand mark of `docs/site/brand/fibo-icon.svg`.
    Fibo,
    Ring,
    Anime,
    Mascot,
}

static FIBO: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-fibo.png")));
static RING: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-ring.png")));
static ANIME: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-anime.png")));
static MASCOT: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-mascot.png")));

fn png(bytes: &[u8]) -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec()))
}

impl AppIcon {
    pub const ALL: [Self; 4] = [Self::Fibo, Self::Ring, Self::Anime, Self::Mascot];
    pub fn label(self) -> &'static str {
        match self {
            Self::Fibo => t!("Fibo 标志"),
            Self::Ring => t!("系泊环"),
            Self::Mascot => t!("吉祥物"),
            Self::Anime => t!("二次元"),
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Fibo => "fibo",
            Self::Ring => "ring",
            Self::Mascot => "mascot",
            Self::Anime => "anime",
        }
    }
    /// The small image beside the traffic lights, decoded once.
    pub fn title_image(self) -> Arc<Image> {
        match self {
            Self::Fibo => FIBO.clone(),
            Self::Ring => RING.clone(),
            Self::Mascot => MASCOT.clone(),
            Self::Anime => ANIME.clone(),
        }
    }
    /// The Dock tile, drawn on Apple's icon grid so the system does not frame it.
    pub fn dock_png(self) -> &'static [u8] {
        match self {
            Self::Fibo => include_bytes!("../../assets/dock-fibo.png"),
            Self::Ring => include_bytes!("../../assets/dock-ring.png"),
            Self::Mascot => include_bytes!("../../assets/dock-mascot.png"),
            Self::Anime => include_bytes!("../../assets/dock-anime.png"),
        }
    }
    /// The saved choice; the Fibo mark when none was made or the file is unreadable. A saved
    /// ring stays the ring: someone picked it.
    pub fn load(file: &Path) -> Self {
        let saved = std::fs::read_to_string(file).unwrap_or_default();
        Self::ALL
            .into_iter()
            .find(|icon| icon.key() == saved.trim())
            .unwrap_or(Self::Fibo)
    }
    pub fn save(self, file: &Path) -> std::io::Result<()> {
        std::fs::write(file, self.key())
    }
}

/// The chosen look and the file that remembers it.
pub struct Chosen {
    pub icon: AppIcon,
    pub file: PathBuf,
}
impl Global for Chosen {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_survives_a_restart_and_unknown_files_fall_back_to_the_fibo_mark() {
        let dir = std::env::temp_dir().join(format!("mooring-app-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app-icon");
        assert_eq!(AppIcon::load(&file), AppIcon::Fibo);
        AppIcon::Mascot.save(&file).unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Mascot);
        std::fs::write(&file, "something else").unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Fibo);
        // The retired ink look comes back as the default; a saved ring stays the ring.
        std::fs::write(&file, "ink").unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Fibo);
        AppIcon::Ring.save(&file).unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Ring);
        AppIcon::Anime.save(&file).unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Anime);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
