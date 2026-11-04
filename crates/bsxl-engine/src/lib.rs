//! BoilerSteamXL engine.
//!
//! Pipeline: scan folders → apply exclusions → score executables → detect duplicates →
//! review queue → explicit user approval → final confirmation → import → verify.
//!
//! Nothing in this crate writes to Steam except [`import::import_confirmed`], and that
//! function only writes entries the user approved and then confirmed with the exact plan
//! they were shown.
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

pub mod artwork;
pub mod diagnostics;
pub mod import;
pub mod library;
pub mod log;
pub mod portable;
pub mod prefs;
pub mod rules;
pub mod scan;
pub mod schedule;
pub mod steam;
pub mod title;
pub mod vdf;

use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub const APP_NAME: &str = "BoilerSteamXL";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Folder holding every BoilerSteamXL file (config, library, logs, backups).
pub fn data_dir() -> PathBuf {
    boilr_core::config::get_config_folder()
}

/// Windows paths compare case-insensitively and with either separator, so every
/// comparison goes through this: forward slashes, lower case, no quotes, no trailing slash.
pub fn norm_path(path: &str) -> String {
    let trimmed = path.trim().trim_matches('"');
    let mut out = trimmed.replace('\\', "/").to_lowercase();
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

/// Letters and digits only, lower case: "A Way Out" and "AWayOut" both become "awayout".
pub fn norm_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Two game titles name the same game when they differ only in case, punctuation, spacing or
/// Roman against Arabic numerals: "Baldur's Gate III" and "Baldur's Gate 3", "Hades II" and
/// "Hades 2". ("X" is left alone: "Mega Man X" is not "Mega Man 10".)
pub fn same_title(a: &str, b: &str) -> bool {
    let key = |s: &str| -> String {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| {
                let t = t.to_lowercase();
                let n = match t.as_str() {
                    "ii" => "2",
                    "iii" => "3",
                    "iv" => "4",
                    "v" => "5",
                    "vi" => "6",
                    "vii" => "7",
                    "viii" => "8",
                    "ix" => "9",
                    _ => return t,
                };
                n.to_string()
            })
            .collect()
    };
    key(a) == key(b)
}

/// Stable 64-bit FNV-1a, used for shortcut identity tags and plan tokens. Not for security.
pub fn fnv64(data: &str) -> u64 {
    fnv64_bytes(data.as_bytes())
}

pub fn fnv64_bytes(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// What a scan found, before it is merged into the library.
pub struct Discovery {
    pub scan: scan::ScanResult,
}

/// The slow half of a refresh: walk the configured folders and read Steam. It does not
/// touch the library, so it runs without the operation lock. Never writes to Steam.
pub fn discover(
    prefs: &prefs::Preferences,
    progress: &dyn Fn(scan::ScanProgress),
    cancel: &std::sync::atomic::AtomicBool,
) -> Discovery {
    let cache_file = data_dir().join("scan-cache.json");
    let mut cache: scan::PeCache = read_json(&cache_file).unwrap_or_default();
    let result = scan::scan(&prefs.scan_roots, &prefs.rules, &mut cache, progress, cancel);
    if let Ok(json) = serde_json::to_vec(&cache) {
        let _ = atomic_write(&cache_file, &json);
    }
    Discovery { scan: result }
}

/// The quick half: merge a discovery into the library. Call it under the operation lock
/// with the library freshly loaded, so decisions made meanwhile are not lost. Steam is read
/// here, under the same lock, so an import that finished during the scan is seen.
pub fn merge(
    lib: &mut library::Library,
    discovery: Discovery,
    launchers: Option<&[library::LauncherGame]>,
    prefs: &prefs::Preferences,
) -> library::ScanSummary {
    let snapshot = steam::snapshot(&steam::detect_steam_path(prefs.steam_path.as_deref()));
    let mut summary = lib.apply_scan(&discovery.scan, launchers, &snapshot);
    summary.orphan_excluded = discovery.scan.orphan_excluded;
    lib.last_scan = Some(summary.clone());
    log::info(
        "scan",
        &format!(
            "{} folders, {} games, {} new, {} ignored executables{}",
            summary.folders_scanned,
            summary.games_found,
            summary.new_games,
            summary.exes_excluded,
            if summary.cancelled { " (cancelled)" } else { "" }
        ),
    );
    summary
}

/// Both halves at once (tests and simple callers).
pub fn refresh(
    prefs: &prefs::Preferences,
    lib: &mut library::Library,
    launchers: Option<&[library::LauncherGame]>,
    progress: &dyn Fn(scan::ScanProgress),
    cancel: &std::sync::atomic::AtomicBool,
) -> library::ScanSummary {
    merge(lib, discover(prefs, progress, cancel), launchers, prefs)
}

pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Writes through a temporary file and renames it over the target, so a crash leaves
/// either the old file or the new one, never half of each. The previous version is kept
/// as `<name>.bak`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("bsxl-tmp-{}-{n}", std::process::id()));
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("bak"));
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Reads JSON written by [`atomic_write`], falling back to the `.bak` copy. A file that
/// cannot be parsed is kept aside (never deleted) and `None` is returned.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let parse = |p: &Path| -> Option<T> {
        let text = std::fs::read_to_string(p).ok()?;
        serde_json::from_str(&text).ok()
    };
    if !path.exists() {
        return None;
    }
    if let Some(value) = parse(path) {
        return Some(value);
    }
    let stamp = time::OffsetDateTime::now_utc().unix_timestamp();
    let _ = std::fs::copy(path, path.with_extension(format!("corrupt-{stamp}")));
    log::warn("state", &format!("{} could not be read; using backup", path.display()));
    parse(&path.with_extension("bak"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_windows_paths() {
        assert_eq!(
            norm_path(r#""D:\Games\PLUG&PLAY\A Way Out\""#),
            "d:/games/plug&play/a way out"
        );
        assert_eq!(norm_path("D:/Games/X.EXE"), norm_path(r"d:\games\x.exe"));
    }

    #[test]
    fn normalizes_names_with_unicode() {
        assert_eq!(norm_name("A Way Out"), "awayout");
        assert_eq!(norm_name("Ōkami HD"), "ōkamihd");
    }

    #[test]
    fn titles_match_across_roman_numerals_and_punctuation() {
        assert!(same_title("Baldur's Gate 3", "Baldur's Gate III"));
        assert!(same_title("Hades II", "Hades 2"));
        assert!(same_title("Grand Theft Auto V", "grand theft auto 5"));
        assert!(same_title("ELDEN RING NIGHTREIGN", "Elden Ring Nightreign"));
        assert!(!same_title("Halo", "Halo: Spartan Assault"));
        assert!(!same_title("Baldur's Gate 3", "Baldur's Gate 3 Toolkit"));
        assert!(!same_title("Mega Man X", "Mega Man 10"));
    }
}
