//! Read-only view of the local Steam installation: accounts, non-Steam shortcuts and
//! officially installed games. Nothing here writes to Steam.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use boilr_core::steam::{get_shortcuts_for_user, get_shortcuts_paths, SteamSettings};
use serde::{Deserialize, Serialize};
use steam_shortcuts_util::shortcut::ShortcutOwned;

use crate::{fnv64, norm_path};

/// Prefix of `DevkitGameID` on shortcuts BoilerSteamXL created; the rest is a hash of the
/// library entry id, so each shortcut can be traced back to its entry.
pub const IDENTITY_PREFIX: &str = "bsxl-";
pub const MANAGED_TAG: &str = "BoilerSteamXL";

pub fn identity_for(entry_id: &str) -> String {
    format!("{IDENTITY_PREFIX}{:016x}", fnv64(entry_id))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SteamShortcut {
    pub user_id: String,
    /// Position in `shortcuts.vdf`; with `app_id` it addresses one record even when
    /// duplicates share an app id.
    pub index: usize,
    pub app_id: u32,
    pub name: String,
    pub exe: String,
    pub start_dir: String,
    pub launch_options: String,
    pub icon: String,
    pub tags: Vec<String>,
    pub identity: String,
    pub managed: bool,
    pub boilr: bool,
    pub exe_exists: bool,
}

impl SteamShortcut {
    pub fn key(&self) -> String {
        format!("{}#{}#{}", self.user_id, self.app_id, self.index)
    }

    pub fn exe_path(&self) -> String {
        self.exe.trim().trim_matches('"').to_string()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SteamUser {
    pub user_id: String,
    pub persona: Option<String>,
    pub most_recent: bool,
    pub folder: String,
    pub shortcuts_file: Option<String>,
    pub shortcuts: Vec<SteamShortcut>,
    /// Set when `shortcuts.vdf` could not be parsed; such a file is never written.
    pub error: Option<String>,
    /// Size + content hash of `shortcuts.vdf`, so a plan can tell if Steam changed it.
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialGame {
    pub app_id: u32,
    pub name: String,
    pub install_dir: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SteamSnapshot {
    pub steam_path: String,
    pub valid: bool,
    pub error: Option<String>,
    pub users: Vec<SteamUser>,
    pub official: Vec<OfficialGame>,
    pub library_folders: Vec<String>,
}

impl SteamSnapshot {
    pub fn all_shortcuts(&self) -> impl Iterator<Item = &SteamShortcut> {
        self.users.iter().flat_map(|u| u.shortcuts.iter())
    }

    pub fn user(&self, user_id: &str) -> Option<&SteamUser> {
        self.users.iter().find(|u| u.user_id == user_id)
    }

    /// The accounts a write goes to: the chosen ones, or every readable account.
    pub fn target_users(&self, chosen: &[String]) -> Vec<&SteamUser> {
        self.users
            .iter()
            .filter(|u| chosen.is_empty() || chosen.contains(&u.user_id))
            .collect()
    }
}

/// The Steam folder: the configured one, then the registry, then Program Files.
pub fn detect_steam_path(configured: Option<&str>) -> String {
    if let Some(path) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        return path.to_string();
    }
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
            if let Ok(path) = key.get_value::<String, _>("SteamPath") {
                let path = path.replace('/', "\\");
                if Path::new(&path).join("steam.exe").is_file() {
                    return path;
                }
            }
        }
    }
    boilr_core::steam::get_default_location().unwrap_or_default()
}

/// Checks a folder really is a Steam installation (not a library or a game folder).
pub fn validate_steam_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("{path} does not exist."));
    }
    let has_client = p.join("steam.exe").is_file() || p.join("steam.sh").is_file();
    if !p.join("userdata").is_dir() {
        if p.join("steamapps").is_dir() && !has_client {
            return Err(format!(
                "{path} looks like a Steam *library* folder, not the Steam installation. The installation is usually C:\\Program Files (x86)\\Steam."
            ));
        }
        return Err(format!("{path} has no userdata folder, so it is not a Steam installation."));
    }
    Ok(())
}

fn steam_settings(path: &str) -> SteamSettings {
    SteamSettings {
        location: Some(path.to_string()),
        ..SteamSettings::default()
    }
}

/// Size and hash of the raw bytes: any change to the file changes it.
pub fn file_fingerprint(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => format!("{}:{:016x}", bytes.len(), crate::fnv64_bytes(&bytes)),
        Err(_) => "absent".into(),
    }
}

pub fn to_view(user_id: &str, index: usize, s: &ShortcutOwned) -> SteamShortcut {
    let exe = s.exe.trim().trim_matches('"').to_string();
    SteamShortcut {
        user_id: user_id.to_string(),
        index,
        app_id: s.app_id,
        name: s.app_name.clone(),
        exe_exists: !exe.is_empty() && Path::new(&exe).exists(),
        exe,
        start_dir: s.start_dir.trim_matches('"').to_string(),
        launch_options: s.launch_options.clone(),
        icon: s.icon.clone(),
        tags: s.tags.clone(),
        identity: s.dev_kit_game_id.clone(),
        managed: s.dev_kit_game_id.starts_with(IDENTITY_PREFIX) || s.tags.iter().any(|t| t == MANAGED_TAG),
        boilr: s.dev_kit_game_id.starts_with("boilr") || s.tags.iter().any(|t| t == "boilr"),
    }
}

pub fn snapshot(steam_path: &str) -> SteamSnapshot {
    let mut snap = SteamSnapshot {
        steam_path: steam_path.to_string(),
        ..SteamSnapshot::default()
    };
    if let Err(e) = validate_steam_path(steam_path) {
        snap.error = Some(e);
        return snap;
    }
    let personas = read_login_users(Path::new(steam_path));
    match get_shortcuts_paths(&steam_settings(steam_path)) {
        Ok(users) => {
            for user in users {
                let mut entry = SteamUser {
                    user_id: user.user_id.clone(),
                    folder: user.steam_user_data_folder.clone(),
                    shortcuts_file: user.shortcut_path.clone(),
                    ..SteamUser::default()
                };
                if let Some((persona, recent)) = personas.iter().find(|p| p.0 == user.user_id).map(|p| (p.1.clone(), p.2)) {
                    entry.persona = Some(persona);
                    entry.most_recent = recent;
                }
                let file = Path::new(&user.steam_user_data_folder).join("config").join("shortcuts.vdf");
                entry.fingerprint = file_fingerprint(&file);
                if user.shortcut_path.is_some() {
                    match get_shortcuts_for_user(&user) {
                        Ok(info) => {
                            // The library misreads app ids whose lowest byte is 0x01; take them
                            // from the lossless reader when both agree on the record count.
                            let raw = std::fs::read(&file).ok().and_then(|b| crate::vdf::parse(&b).ok()).unwrap_or_default();
                            entry.shortcuts = info
                                .shortcuts
                                .iter()
                                .enumerate()
                                .map(|(i, s)| {
                                    let mut view = to_view(&user.user_id, i, s);
                                    if raw.len() == info.shortcuts.len() {
                                        if let Some(id) = raw.get(i).and_then(|r| r.app_id()) {
                                            view.app_id = id;
                                        }
                                    }
                                    view
                                })
                                .collect();
                        }
                        Err(e) => entry.error = Some(e.to_string()),
                    }
                }
                snap.users.push(entry);
            }
            snap.users.sort_by(|a, b| b.most_recent.cmp(&a.most_recent).then(a.user_id.cmp(&b.user_id)));
            snap.valid = true;
        }
        Err(e) => snap.error = Some(e.to_string()),
    }
    let (folders, official) = read_official_games(Path::new(steam_path));
    snap.library_folders = folders;
    snap.official = official;
    snap
}

/// `"key"  "value"` pairs from a VDF/ACF line, with `\\` unescaped.
fn vdf_pair(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split('"').collect();
    // ["\t", key, "\t\t", value, ""]
    if parts.len() >= 5 {
        Some((parts.get(1)?.to_string(), parts.get(3)?.replace("\\\\", "\\")))
    } else {
        None
    }
}

fn read_official_games(steam: &Path) -> (Vec<String>, Vec<OfficialGame>) {
    let mut libraries = vec![steam.to_path_buf()];
    if let Ok(text) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        for (key, value) in text.lines().filter_map(vdf_pair) {
            if key.eq_ignore_ascii_case("path") {
                let path = PathBuf::from(value);
                if !libraries.iter().any(|l| norm_path(&l.to_string_lossy()) == norm_path(&path.to_string_lossy())) {
                    libraries.push(path);
                }
            }
        }
    }
    let mut games = Vec::new();
    for library in &libraries {
        let apps = library.join("steamapps");
        let Ok(entries) = std::fs::read_dir(&apps) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let (mut app_id, mut title, mut dir) = (None, None, None);
            for (key, value) in text.lines().filter_map(vdf_pair) {
                match key.to_lowercase().as_str() {
                    "appid" if app_id.is_none() => app_id = value.parse().ok(),
                    "name" if title.is_none() => title = Some(value),
                    "installdir" if dir.is_none() => dir = Some(value),
                    _ => {}
                }
            }
            if let (Some(app_id), Some(name), Some(dir)) = (app_id, title, dir) {
                games.push(OfficialGame {
                    app_id,
                    name,
                    install_dir: apps.join("common").join(dir).to_string_lossy().to_string(),
                });
            }
        }
    }
    (
        libraries.iter().map(|l| l.to_string_lossy().to_string()).collect(),
        games,
    )
}

/// `(account id, persona name, most recent)` from `config/loginusers.vdf`.
fn read_login_users(steam: &Path) -> Vec<(String, String, bool)> {
    let Ok(text) = std::fs::read_to_string(steam.join("config").join("loginusers.vdf")) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String, bool)> = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        let quoted: Vec<&str> = trimmed.split('"').filter(|s| !s.trim().is_empty()).collect();
        if quoted.len() == 1 {
            if let Some(Ok(id64)) = quoted.first().map(|s| s.parse::<u64>()) {
                let account = id64.saturating_sub(76_561_197_960_265_728);
                current = Some(account.to_string());
                out.push((account.to_string(), String::new(), false));
            }
        } else if let (Some(_), Some((key, value))) = (&current, vdf_pair(trimmed)) {
            if let Some(last) = out.last_mut() {
                match key.to_lowercase().as_str() {
                    "personaname" => last.1 = value,
                    "mostrecent" => last.2 = value == "1",
                    _ => {}
                }
            }
        }
    }
    out
}

pub fn is_steam_running() -> bool {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};
    let system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );
    let name = if cfg!(windows) { "steam.exe" } else { "steam" };
    let running = system.processes_by_exact_name(OsStr::new(name)).next().is_some();
    running
}

/// Asks Steam to close itself (`steam.exe -shutdown`) and waits. Never kills Steam:
/// if it is still running after `timeout`, the caller must ask the user to close it.
pub fn close_steam_gracefully(steam_path: &str, timeout: Duration) -> bool {
    if !is_steam_running() {
        return true;
    }
    let exe = Path::new(steam_path).join(if cfg!(windows) { "steam.exe" } else { "steam" });
    if std::process::Command::new(exe).arg("-shutdown").spawn().is_err() {
        return false;
    }
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        if !is_steam_running() {
            // Steam writes its files on the way out; give it a moment to finish.
            std::thread::sleep(Duration::from_secs(2));
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_stable() {
        assert_eq!(identity_for("fs:d:/games/a way out"), identity_for("fs:d:/games/a way out"));
        assert!(identity_for("x").starts_with(IDENTITY_PREFIX));
    }

    #[test]
    fn parses_vdf_pairs_with_escaped_paths() {
        assert_eq!(
            vdf_pair("\t\t\"path\"\t\t\"D:\\\\SteamLibrary\""),
            Some(("path".into(), "D:\\SteamLibrary".into()))
        );
    }

    #[test]
    fn library_folder_is_not_a_steam_install() {
        let dir = std::env::temp_dir().join(format!("bsxl-lib-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("steamapps")).ok();
        let err = validate_steam_path(&dir.to_string_lossy()).err().unwrap_or_default();
        assert!(err.contains("library"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
