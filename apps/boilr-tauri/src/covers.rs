//! Artwork for every game in the library, shown in the app before anything is imported.
//!
//! Sources, best first: the artwork Steam already has for an imported or linked shortcut,
//! BoilerSteamXL's SteamGridDB cache (`bsxl_engine::artwork`), Steam's own library cover for
//! an official game, and the icon inside the game's executable. Downloads only write into
//! BoilerSteamXL's data folder; Steam gets the images when the game is imported.

use std::{collections::HashMap, path::Path};

use boilr_core::{
    settings::Settings,
    steamgriddb::{get_image_extension, get_query_type, ImageType},
};
use bsxl_engine::{
    artwork::{self, Meta},
    library::{Entry, Library, Status},
    prefs::Preferences,
    steam,
};
use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Default, Serialize)]
pub struct GameArt {
    pub cover: Option<String>,
    pub hero: Option<String>,
    pub logo: Option<String>,
    pub icon: Option<String>,
    /// "steam", "steamgriddb", "steam library" or "executable".
    pub source: Option<String>,
    pub match_name: Option<String>,
    pub match_chosen: bool,
    /// Cover file's modification time: a re-downloaded image gets a new URL, not the old cached one.
    pub version: Option<u64>,
}

fn path_string(p: impl AsRef<Path>) -> String {
    p.as_ref().to_string_lossy().to_string()
}

fn mtime(path: &str) -> Option<u64> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    modified.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

fn steam_grid_image(folder: &Path, app_id: u32, kind: ImageType) -> Option<String> {
    ["png", "jpg", "jpeg", "webp", "ico"]
        .iter()
        .map(|ext| folder.join(kind.file_name(app_id, ext)))
        .find(|p| p.is_file())
        .map(path_string)
}

/// The best artwork for every game. Opens the image folders it uses to the web view
/// (read-only asset access) and creates nothing in Steam's folders.
#[tauri::command(async)]
pub fn game_art<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> HashMap<String, GameArt> {
    let prefs = Preferences::load();
    let steam_path = steam::detect_steam_path(prefs.steam_path.as_deref());
    let snap = steam::snapshot(&steam_path);
    let lib = Library::load();
    let scope = app.asset_protocol_scope();
    let root = artwork::cache_root();
    let _ = std::fs::create_dir_all(&root);
    let _ = scope.allow_directory(&root, true);
    let library_cache = Path::new(&steam_path).join("appcache").join("librarycache");
    if library_cache.is_dir() {
        let _ = scope.allow_directory(&library_cache, true);
    }
    let grids: Vec<(String, std::path::PathBuf)> = snap
        .users
        .iter()
        .map(|u| (u.user_id.clone(), Path::new(&u.folder).join("config").join("grid")))
        .filter(|(_, g)| g.is_dir())
        .collect();
    for (_, grid) in &grids {
        let _ = scope.allow_directory(grid, false);
    }

    let mut out = HashMap::new();
    for entry in lib.entries.values() {
        let mut art = GameArt::default();
        // 1. Steam's artwork for the shortcut this game has (imported or linked).
        let shortcut_id = entry.import.as_ref().map(|r| r.app_id).or_else(|| {
            entry.linked.as_ref().filter(|l| l.kind == "shortcut").and_then(|l| l.app_id)
        });
        if let Some(app_id) = shortcut_id {
            for (_, grid) in &grids {
                art.cover = art.cover.or_else(|| steam_grid_image(grid, app_id, ImageType::Grid));
                art.hero = art.hero.or_else(|| steam_grid_image(grid, app_id, ImageType::Hero));
                art.logo = art.logo.or_else(|| steam_grid_image(grid, app_id, ImageType::Logo));
                art.icon = art.icon.or_else(|| steam_grid_image(grid, app_id, ImageType::Icon));
            }
            if art.cover.is_some() {
                art.source = Some("steam".into());
            }
        }
        // 2. BoilerSteamXL's SteamGridDB cache.
        let cached = |kind: &str| artwork::cached(&entry.id, kind).map(path_string);
        if art.cover.is_none() {
            if let Some(cover) = cached("grid") {
                art.cover = Some(cover);
                art.source = Some("steamgriddb".into());
            }
        }
        art.hero = art.hero.or_else(|| cached("hero"));
        art.logo = art.logo.or_else(|| cached("logo"));
        art.icon = art.icon.or_else(|| cached("icon"));
        // 3. Steam's library cover for an official game it is linked to.
        if art.cover.is_none() {
            if let Some(official) = entry.linked.as_ref().filter(|l| l.kind == "official").and_then(|l| l.app_id) {
                if let Some(cover) = artwork::steam_library_cover(&steam_path, official) {
                    art.cover = Some(path_string(cover));
                    art.source = Some("steam library".into());
                }
            }
        }
        // 4. The executable's own icon (read only, never run).
        if art.icon.is_none() && !entry.exe.is_empty() && Path::new(&entry.exe).is_file() {
            art.icon = artwork::exe_icon(&entry.id, &entry.exe).map(path_string);
        }
        if let Some(meta) = artwork::read_meta(&entry.id) {
            art.match_name = meta.game_name;
            art.match_chosen = meta.chosen;
        }
        art.version = art.cover.as_deref().and_then(mtime);
        out.insert(entry.id.clone(), art);
    }
    // Steam shortcuts no library game owns (made by hand, BoilR or another tool), keyed "steam:<key>".
    for user in &snap.users {
        let grid = Path::new(&user.folder).join("config").join("grid");
        for s in user.shortcuts.iter().filter(|s| lib.entry_for_shortcut(s).is_none()) {
            let mut art = GameArt::default();
            if grid.is_dir() {
                art.cover = steam_grid_image(&grid, s.app_id, ImageType::Grid);
                art.hero = steam_grid_image(&grid, s.app_id, ImageType::Hero);
                art.logo = steam_grid_image(&grid, s.app_id, ImageType::Logo);
                art.icon = steam_grid_image(&grid, s.app_id, ImageType::Icon);
                art.source = art.cover.as_ref().map(|_| "steam".into());
            }
            if art.icon.is_none() && s.exe_exists {
                art.icon = artwork::exe_icon(&format!("steam:{}:{}", s.user_id, s.app_id), &s.exe_path()).map(path_string);
            }
            out.insert(format!("steam:{}", s.key()), art);
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtProgress {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FetchSummary {
    pub checked: usize,
    pub downloaded: usize,
    pub not_found: usize,
    pub failed: usize,
}

fn client() -> Result<(Settings, steamgriddb_api::Client), String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;
    let key = settings
        .steamgrid_db
        .auth_key
        .clone()
        .filter(|k| !k.trim().is_empty())
        .ok_or("Add your free SteamGridDB API key in Settings to download artwork.")?;
    Ok((settings, steamgriddb_api::Client::new(key)))
}

/// Games whose artwork is worth fetching: not ignored/rejected, with a title.
fn wants_art(entry: &Entry) -> bool {
    !matches!(entry.status, Status::Ignored | Status::Rejected) && !entry.display_title().trim().is_empty()
}

/// One HTTP client per run, with a timeout so a stalled download cannot hang the run.
pub(crate) fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| e.to_string())
}

/// Downloads an image into memory: a failed or empty download is an error, never a file.
pub(crate) async fn fetch_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let bytes = http.get(url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("empty image".into());
    }
    Ok(bytes.to_vec())
}

async fn download(http: &reqwest::Client, url: &str, path: &Path) -> Result<(), String> {
    let bytes = fetch_bytes(http, url).await?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, &bytes).map_err(|e| e.to_string())
}

/// Downloads the cover, wide capsule, hero, logo and icon for one game into the cache.
/// Returns how many it has, and whether a request or download failed (worth retrying).
async fn fetch_one(settings: &Settings, client: &steamgriddb_api::Client, http: &reqwest::Client, entry: &Entry, game_id: usize) -> (usize, bool) {
    let dir = artwork::cache_dir(&entry.id);
    let mut got = 0;
    let mut had_error = false;
    let wanted: [(&str, ImageType); 5] = [
        ("grid", ImageType::Grid),
        ("wide_grid", ImageType::WideGrid),
        ("hero", ImageType::Hero),
        ("logo", ImageType::Logo),
        ("icon", ImageType::Icon),
    ];
    for (kind, image_type) in wanted {
        if artwork::cached(&entry.id, kind).is_some() {
            got += 1;
            continue;
        }
        let query = get_query_type(settings.steamgrid_db.prefer_animated, &image_type, settings.steamgrid_db.allow_nsfw);
        let Ok(images) = client.get_images_for_id(game_id, &query).await else {
            had_error = true;
            continue;
        };
        // Grids come portrait and landscape: covers are portrait, wide capsules landscape.
        let pick = images.iter().find(|i| match kind {
            "grid" => i.height > i.width,
            "wide_grid" => i.width > i.height,
            _ => true,
        });
        if let Some(image) = pick {
            let path = dir.join(format!("{kind}.{}", get_image_extension(&image.mime)));
            match download(http, &image.url, &path).await {
                Ok(()) => got += 1,
                Err(_) => had_error = true,
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    }
    (got, had_error)
}

/// One game's artwork at a time across every run (background, rename, Refresh, a picked match),
/// so two runs never write the same game's files at once.
static ENTRY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Fetches missing artwork from SteamGridDB, one game at a time (gentle on its rate limits),
/// with progress events. `ids` limits it to some games; `force` refetches them.
#[tauri::command]
pub async fn fetch_artwork<R: tauri::Runtime>(app: tauri::AppHandle<R>, ids: Option<Vec<String>>, force: bool) -> Result<FetchSummary, String> {
    let (settings, client) = client()?;
    let http = http()?;
    let started = bsxl_engine::now_rfc3339();
    let lib = Library::load();
    let todo: Vec<Entry> = lib
        .entries
        .values()
        .filter(|e| wants_art(e) && ids.as_ref().is_none_or(|ids| ids.contains(&e.id)))
        .filter(|e| {
            if force {
                return true;
            }
            match artwork::read_meta(&e.id) {
                // Already looked up for this title: only retry if images are missing.
                Some(meta) if meta.query == e.display_title() => {
                    meta.game_id.is_some() && artwork::cached(&e.id, "grid").is_none() && meta.fetched_at.is_empty()
                }
                _ => true,
            }
        })
        .cloned()
        .collect();
    let mut summary = FetchSummary::default();
    let total = todo.len();
    let mut search_failures = 0;
    for (n, queued) in todo.iter().enumerate() {
        let _guard = ENTRY_LOCK.lock().await;
        // The game as it is now: it may have been renamed or removed since this run started.
        let Some(entry) = Library::load().get(&queued.id).cloned().filter(wants_art) else { continue };
        let title = entry.display_title().to_string();
        let _ = app.emit("artwork-progress", ArtProgress { done: n, total, current: title.clone() });
        let previous = artwork::read_meta(&entry.id);
        // Another run looked it up, for this name, after this one started: nothing left to do.
        if !force && previous.as_ref().is_some_and(|m| m.query == title && m.fetched_at > started) {
            continue;
        }
        summary.checked += 1;
        // A match the user picked holds until they rename the game; a new name means a new search.
        let renamed = previous.as_ref().is_some_and(|m| m.query != title);
        let chosen = previous.as_ref().filter(|m| m.chosen && !renamed).and_then(|m| m.game_id.map(|id| (id, m.game_name.clone())));
        let matched = match chosen {
            Some((id, name)) => Some((id, name)),
            // `.ok()` first: the error type is not Send and must not live across the sleep below.
            None => match client.search(&title).await.ok() {
                Some(results) => results.into_iter().next().map(|r| (r.id, Some(r.name))),
                None => {
                    summary.failed += 1;
                    search_failures += 1;
                    if search_failures >= 3 {
                        return Err("SteamGridDB is not answering or refused the key. Try again later.".into());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
            },
        };
        search_failures = 0;
        // A refetch or a renamed game: drop the old match's images.
        if force || renamed {
            artwork::clear_downloaded(&entry.id);
        }
        let mut meta = Meta {
            game_id: matched.as_ref().map(|m| m.0),
            game_name: matched.as_ref().and_then(|m| m.1.clone()),
            query: title.clone(),
            chosen: !renamed && previous.is_some_and(|m| m.chosen),
            fetched_at: String::new(),
        };
        let mut had_error = false;
        match matched {
            Some((game_id, _)) => {
                let (got, error) = fetch_one(&settings, &client, &http, &entry, game_id).await;
                had_error = error;
                if got > 0 {
                    summary.downloaded += 1;
                }
            }
            None => summary.not_found += 1,
        }
        // Left empty after a failed request or download, so the next run tries again.
        if !had_error {
            meta.fetched_at = bsxl_engine::now_rfc3339();
        }
        let _ = artwork::write_meta(&entry.id, &meta);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    let _ = app.emit("artwork-progress", ArtProgress { done: total, total, current: String::new() });
    bsxl_engine::log::info("artwork", &format!("{} checked, {} with artwork, {} not found", summary.checked, summary.downloaded, summary.not_found));
    Ok(summary)
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtMatch {
    pub id: usize,
    pub name: String,
    pub year: Option<i32>,
}

/// SteamGridDB games for a search, to choose a different artwork match.
#[tauri::command]
pub async fn search_artwork(query: String) -> Result<Vec<ArtMatch>, String> {
    let (_, client) = client()?;
    let results = client.search(&query).await.map_err(|e| e.to_string())?;
    Ok(results
        .into_iter()
        .take(10)
        .map(|r| ArtMatch {
            id: r.id,
            name: r.name,
            year: r.release_date.and_then(|t| i64::try_from(t).ok()).and_then(|t| time::OffsetDateTime::from_unix_timestamp(t).ok()).map(|d| d.year()),
        })
        .collect())
}

/// A small cover image (portrait if there is one) for a SteamGridDB game, to show search results.
#[tauri::command]
pub async fn artwork_thumb(game_id: usize) -> Result<Option<String>, String> {
    let (settings, client) = client()?;
    let query = get_query_type(false, &ImageType::Grid, settings.steamgrid_db.allow_nsfw);
    let images = client.get_images_for_id(game_id, &query).await.map_err(|e| e.to_string())?;
    let pick = images.iter().find(|i| i.height > i.width).or(images.first());
    Ok(pick.map(|i| i.thumb.clone()))
}

/// Asks SteamGridDB whether a key works (a wrong key is refused).
#[tauri::command]
pub async fn check_steamgriddb_key(key: String) -> Result<(), String> {
    steamgriddb_api::Client::new(key.trim())
        .search("Portal")
        .await
        .map(|_| ())
        .map_err(|_| "SteamGridDB did not accept this key (or there is no internet). Copy it again from your SteamGridDB API page.".to_string())
}

/// Uses a chosen SteamGridDB game for one library game and downloads its artwork.
#[tauri::command]
pub async fn set_artwork_match<R: tauri::Runtime>(app: tauri::AppHandle<R>, entry_id: String, game_id: usize, game_name: String) -> Result<FetchSummary, String> {
    let lib = Library::load();
    let entry = lib.get(&entry_id).cloned().ok_or("Unknown game")?;
    {
        let _guard = ENTRY_LOCK.lock().await;
        artwork::clear_downloaded(&entry_id);
        let meta = Meta { game_id: Some(game_id), game_name: Some(game_name), query: entry.display_title().to_string(), chosen: true, fetched_at: String::new() };
        artwork::write_meta(&entry_id, &meta).map_err(|e| e.to_string())?;
    }
    fetch_artwork(app, Some(vec![entry_id]), false).await
}
