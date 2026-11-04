//! Diagnostic report for troubleshooting. Personal details are masked: the Windows user
//! name in paths, Steam account ids, and the SteamGridDB key (never included at all).

use std::fmt::Write;

use crate::{library::Library, prefs::Preferences, steam::SteamSnapshot};

fn mask_path(path: &str) -> String {
    let lower = path.to_lowercase();
    let marker = "\\users\\";
    match lower.find(marker) {
        Some(start) => {
            let rest_start = start + marker.len();
            let end = path.get(rest_start..).and_then(|r| r.find('\\')).map_or(path.len(), |i| rest_start + i);
            format!("{}<user>{}", path.get(..rest_start).unwrap_or_default(), path.get(end..).unwrap_or_default())
        }
        None => path.to_string(),
    }
}

fn mask_id(id: &str) -> String {
    let tail: String = id.chars().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect();
    format!("…{tail}")
}

pub fn report(prefs: &Preferences, lib: &Library, snapshot: &SteamSnapshot, mode: &str, task_installed: bool) -> String {
    let mut r = String::new();
    let _ = writeln!(r, "BoilerSteamXL {} diagnostic report", crate::VERSION);
    let _ = writeln!(r, "Generated: {}", crate::now_rfc3339());
    let _ = writeln!(r, "Edition: {mode}");
    let _ = writeln!(r, "Simulation mode: {}", if prefs.simulation_mode { "on (no Steam writes)" } else { "off" });
    let _ = writeln!(r, "\n[Steam]");
    let _ = writeln!(r, "Path: {}", mask_path(&snapshot.steam_path));
    let _ = writeln!(r, "Valid: {}", snapshot.valid);
    if let Some(e) = &snapshot.error {
        let _ = writeln!(r, "Problem: {}", mask_path(e));
    }
    for u in &snapshot.users {
        let _ = writeln!(
            r,
            "Account {}: {} shortcuts{}",
            mask_id(&u.user_id),
            u.shortcuts.len(),
            u.error.as_ref().map_or(String::new(), |_| " (shortcuts.vdf damaged)".into())
        );
    }
    let _ = writeln!(r, "Official games found: {}", snapshot.official.len());
    let _ = writeln!(r, "\n[Scan folders]");
    for root in &prefs.scan_roots {
        let exists = std::path::Path::new(&root.path).is_dir();
        let _ = writeln!(r, "{} (enabled: {}, depth {}, reachable: {exists})", mask_path(&root.path), root.enabled, root.max_depth);
    }
    let _ = writeln!(r, "\n[Last scan]");
    match &lib.last_scan {
        Some(s) => {
            let _ = writeln!(
                r,
                "{}: {} folders, {} games, {} new, {} need review, {} executables ignored, {} missing, {} ms{}",
                s.at, s.folders_scanned, s.games_found, s.new_games, s.needs_review, s.exes_excluded, s.missing, s.duration_ms,
                if s.cancelled { " (cancelled)" } else { "" }
            );
            for (root, e) in &s.root_errors {
                let _ = writeln!(r, "Folder problem: {} — {e}", mask_path(root));
            }
        }
        None => {
            let _ = writeln!(r, "No scan yet");
        }
    }
    let _ = writeln!(r, "\n[Schedule]");
    let _ = writeln!(r, "Mode: {:?} at {}, task registered: {task_installed}", prefs.schedule.mode, prefs.schedule.time);
    let _ = writeln!(r, "\n[Library]");
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for e in lib.entries.values() {
        *counts.entry(e.status.label()).or_default() += 1;
    }
    for (status, n) in counts {
        let _ = writeln!(r, "{status}: {n}");
    }
    let dups = lib.entries.values().filter(|e| e.duplicate.is_some()).count();
    let _ = writeln!(r, "Entries with duplicate evidence: {dups}");
    let _ = writeln!(r, "\n[Recent problems]");
    for record in crate::log::tail(200).iter().filter(|l| l["level"] != "info").take(15) {
        let _ = writeln!(r, "{} {} {}", record["ts"], record["event"], mask_path(record["message"].as_str().unwrap_or_default()));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_personal_details() {
        assert_eq!(mask_path(r"C:\Users\jabri\AppData\Roaming\BoilerSteamXL"), r"C:\Users\<user>\AppData\Roaming\BoilerSteamXL");
        assert_eq!(mask_id("123456789"), "…789");
    }
}
