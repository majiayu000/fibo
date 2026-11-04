//! The app's looks, the mooring ring, ink ring, mascot and anime portrait: the title-bar image and the Dock tile.
//! The choice belongs to this machine's client, not to the workspace.
use gpui::{Global, Image, ImageFormat};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppIcon {
    Ring,
    Ink,
    Mascot,
    Anime,
}

static RING: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-ring.png")));
static INK: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-ink.png")));
static ANIME: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-anime.png")));
static MASCOT: LazyLock<Arc<Image>> =
    LazyLock::new(|| png(include_bytes!("../../assets/title-mascot.png")));

fn png(bytes: &[u8]) -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec()))
}

impl AppIcon {
    pub const ALL: [Self; 4] = [Self::Ring, Self::Ink, Self::Mascot, Self::Anime];
    pub fn label(self) -> &'static str {
        match self {
            Self::Ring => "系泊环",
            Self::Ink => "水墨",
            Self::Mascot => "吉祥物",
            Self::Anime => "二次元",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Ring => "ring",
            Self::Ink => "ink",
            Self::Mascot => "mascot",
            Self::Anime => "anime",
        }
    }
    /// The small image beside the traffic lights, decoded once.
    pub fn title_image(self) -> Arc<Image> {
        match self {
            Self::Ring => RING.clone(),
            Self::Ink => INK.clone(),
            Self::Mascot => MASCOT.clone(),
            Self::Anime => ANIME.clone(),
        }
    }
    /// The Dock tile, drawn on Apple's icon grid so the system does not frame it.
    pub fn dock_png(self) -> &'static [u8] {
        match self {
            Self::Ring => include_bytes!("../../assets/dock-ring.png"),
            Self::Ink => include_bytes!("../../assets/dock-ink.png"),
            Self::Mascot => include_bytes!("../../assets/dock-mascot.png"),
            Self::Anime => include_bytes!("../../assets/dock-anime.png"),
        }
    }
    /// The saved choice; the ring when none was made or the file is unreadable.
    pub fn load(file: &Path) -> Self {
        let saved = std::fs::read_to_string(file).unwrap_or_default();
        Self::ALL
            .into_iter()
            .find(|icon| icon.key() == saved.trim())
            .unwrap_or(Self::Ring)
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
    fn the_choice_survives_a_restart_and_unknown_files_fall_back_to_the_ring() {
        let dir = std::env::temp_dir().join(format!("mooring-app-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app-icon");
        assert_eq!(AppIcon::load(&file), AppIcon::Ring);
        AppIcon::Mascot.save(&file).unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Mascot);
        std::fs::write(&file, "something else").unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Ring);
        AppIcon::Ink.save(&file).unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Ink);
        AppIcon::Anime.save(&file).unwrap();
        assert_eq!(AppIcon::load(&file), AppIcon::Anime);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
