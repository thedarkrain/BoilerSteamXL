//! Artwork for library games, kept in BoilerSteamXL's own cache (never in Steam until a
//! game is imported).
//!
//! `artwork/<entry key>/` holds `grid.*` (600×900 cover), `wide_grid.*`, `hero.*`,
//! `logo.*`, `icon.*` from SteamGridDB, `exe_icon.ico` extracted from the game's executable
//! (offline fallback), and `meta.json` with the SteamGridDB game it was matched to. On
//! import the images are copied into the account's Steam grid folder under the new
//! shortcut's id, without replacing artwork Steam already has.

use std::path::{Path, PathBuf};

use boilr_core::steamgriddb::ImageType;
use serde::{Deserialize, Serialize};

use crate::{data_dir, fnv64};

/// Kinds BoilerSteamXL downloads, with the Steam grid image type each becomes.
pub const KINDS: [(&str, ImageType); 5] = [
    ("grid", ImageType::Grid),
    ("wide_grid", ImageType::WideGrid),
    ("hero", ImageType::Hero),
    ("logo", ImageType::Logo),
    ("icon", ImageType::Icon),
];

const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "ico"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// SteamGridDB game id; `None` when nothing matched.
    pub game_id: Option<usize>,
    pub game_name: Option<String>,
    /// The title that was searched for.
    pub query: String,
    /// The user picked this match; automatic refreshes keep it.
    #[serde(default)]
    pub chosen: bool,
    pub fetched_at: String,
}

pub fn cache_root() -> PathBuf {
    data_dir().join("artwork")
}

pub fn cache_dir(entry_id: &str) -> PathBuf {
    cache_root().join(format!("{:016x}", fnv64(entry_id)))
}

/// The cached image of one kind ("grid", "hero", …, or "exe_icon"), whatever its type.
pub fn cached(entry_id: &str, kind: &str) -> Option<PathBuf> {
    let dir = cache_dir(entry_id);
    EXTENSIONS.iter().map(|ext| dir.join(format!("{kind}.{ext}"))).find(|p| p.is_file())
}

pub fn read_meta(entry_id: &str) -> Option<Meta> {
    let text = std::fs::read_to_string(cache_dir(entry_id).join("meta.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write_meta(entry_id: &str, meta: &Meta) -> std::io::Result<()> {
    let dir = cache_dir(entry_id);
    std::fs::create_dir_all(&dir)?;
    crate::atomic_write(&dir.join("meta.json"), serde_json::to_string_pretty(meta).unwrap_or_default().as_bytes())
}

/// Removes downloaded images (not the executable icon) so a new match can be fetched.
pub fn clear_downloaded(entry_id: &str) {
    for (kind, _) in KINDS {
        while let Some(path) = cached(entry_id, kind) {
            if std::fs::remove_file(&path).is_err() {
                break;
            }
        }
    }
}

/// Extracts the executable's own icon as an .ico file, reading the file only (it is never
/// run). Returns the cached path; a later call reuses it.
pub fn exe_icon(entry_id: &str, exe: &str) -> Option<PathBuf> {
    if let Some(existing) = cached(entry_id, "exe_icon") {
        return Some(existing);
    }
    let map = pelite::FileMap::open(Path::new(exe)).ok()?;
    let file = pelite::PeFile::from_bytes(map.as_ref()).ok()?;
    let resources = file.resources().ok()?;
    let (_, group) = resources.icons().filter_map(Result::ok).next()?;
    let mut bytes = Vec::new();
    group.write(&mut bytes).ok()?;
    if bytes.len() < 32 {
        return None;
    }
    let dir = cache_dir(entry_id);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("exe_icon.ico");
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Where an imported shortcut's icon will be in the account's grid folder.
pub fn grid_icon_path(user_folder: &str, entry_id: &str, app_id: u32) -> Option<String> {
    let cached_icon = cached(entry_id, "icon")?;
    let grid = Path::new(user_folder).join("config").join("grid");
    // copy_to_grid keeps an icon Steam already has for this id (any extension): point at that one.
    if let Some(existing) = EXTENSIONS.iter().map(|e| grid.join(ImageType::Icon.file_name(app_id, e))).find(|p| p.is_file()) {
        return Some(existing.to_string_lossy().to_string());
    }
    let ext = cached_icon.extension()?.to_string_lossy().to_string();
    Some(grid.join(ImageType::Icon.file_name(app_id, ext)).to_string_lossy().to_string())
}

/// Kinds of artwork cached for an entry, for the import preview.
pub fn cached_kinds(entry_id: &str) -> Vec<&'static str> {
    KINDS.iter().filter(|(k, _)| cached(entry_id, k).is_some()).map(|(k, _)| *k).collect()
}

/// Copies an entry's cached artwork into a Steam grid folder for `app_id`. Images Steam
/// already has for that id are kept. Returns how many files were copied.
pub fn copy_to_grid(entry_id: &str, user_folder: &str, app_id: u32) -> usize {
    let grid = Path::new(user_folder).join("config").join("grid");
    let mut copied = 0;
    for (kind, image_type) in KINDS {
        let Some(source) = cached(entry_id, kind) else { continue };
        let ext = source.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "png".into());
        let existing = EXTENSIONS.iter().any(|e| grid.join(image_type.file_name(app_id, e)).is_file());
        if existing {
            continue;
        }
        if std::fs::create_dir_all(&grid).is_ok() && std::fs::copy(&source, grid.join(image_type.file_name(app_id, &ext))).is_ok() {
            copied += 1;
        }
    }
    copied
}

/// Cached kinds Steam's grid folder still has no image of (any extension) for `app_id`.
pub fn missing_in_grid(entry_id: &str, user_folder: &str, app_id: u32) -> Vec<&'static str> {
    let grid = Path::new(user_folder).join("config").join("grid");
    KINDS
        .iter()
        .filter(|(kind, image_type)| cached(entry_id, kind).is_some() && !EXTENSIONS.iter().any(|e| grid.join(image_type.file_name(app_id, e)).is_file()))
        .map(|(kind, _)| *kind)
        .collect()
}

/// Steam's own cover for an official game, from its local library cache.
pub fn steam_library_cover(steam_path: &str, app_id: u32) -> Option<PathBuf> {
    let cache = Path::new(steam_path).join("appcache").join("librarycache");
    [
        cache.join(app_id.to_string()).join("library_600x900.jpg"),
        cache.join(format!("{app_id}_library_600x900.jpg")),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_names_follow_steam() {
        assert_eq!(ImageType::Grid.file_name(7, "png"), "7p.png");
        assert_eq!(ImageType::Icon.file_name(7, "ico"), "7-icon.ico");
    }
}
