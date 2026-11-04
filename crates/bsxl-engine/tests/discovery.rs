//! Scanner, title resolver and exclusion tests on a synthetic games folder.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::AtomicBool;

use bsxl_engine::{
    library::{Library, Status},
    rules::RuleSet,
    scan::{scan, PeCache, ScanRoot},
};
use common::*;

fn scan_games(t: &Temp) -> bsxl_engine::scan::ScanResult {
    let roots = vec![ScanRoot { path: t.s("Games"), enabled: true, max_depth: 6 }];
    scan(&roots, &RuleSet::recommended(), &mut PeCache::new(), &|_| {}, &AtomicBool::new(false))
}

fn game<'a>(r: &'a bsxl_engine::scan::ScanResult, title: &str) -> &'a bsxl_engine::scan::DiscoveredGame {
    r.games
        .iter()
        .find(|g| g.title == title)
        .unwrap_or_else(|| panic!("no game {title}; found {:?}", r.games.iter().map(|g| &g.title).collect::<Vec<_>>()))
}

#[test]
fn generic_exe_gets_its_name_from_the_folder() {
    let t = Temp::new("mvc");
    games_fixture(&t);
    let r = scan_games(&t);
    let mvc = game(&r, "Marvel vs. Capcom");
    assert_eq!(mvc.title_source, "Folder name");
    assert_eq!(mvc.candidates[0].file_name, "game.exe");
    // Engine crash reporter and _Redist are excluded, not separate games.
    assert!(mvc.excluded.iter().any(|e| e.path.contains("CrashReportClient")));
    assert!(mvc.excluded.iter().any(|e| e.path.contains("vcredist_x64")));
}

#[test]
fn container_and_generic_folders_never_become_games() {
    let t = Temp::new("containers");
    games_fixture(&t);
    let r = scan_games(&t);
    for bad in ["PLUG&PLAY", "Games", "Engine", "Binaries", "Win64", "_Redist", "_CommonRedist", "Redist", "MD5"] {
        assert!(!r.games.iter().any(|g| g.title == bad), "{bad} became a game");
    }
    assert_eq!(r.games.len(), 8, "{:?}", r.games.iter().map(|g| &g.title).collect::<Vec<_>>());
}

#[test]
fn quicksfv_and_uninstallers_are_ignored() {
    let t = Temp::new("awo");
    games_fixture(&t);
    let r = scan_games(&t);
    let awo = game(&r, "A Way Out");
    assert_eq!(awo.candidates.len(), 1);
    assert_eq!(awo.candidates[0].file_name, "AWayOut.exe");
    assert!(!awo.uncertain);
    let reasons: Vec<&str> = awo.excluded.iter().map(|e| e.reason.as_str()).collect();
    assert!(reasons.iter().any(|r| r.contains("verification")), "{reasons:?}");
    assert!(reasons.iter().any(|r| r.contains("uninstall")), "{reasons:?}");
    assert!(awo.excluded.iter().any(|e| e.path.ends_with("DXSETUP.exe")));
    assert!(r.exes_excluded >= 4);
}

#[test]
fn language_selector_is_ignored_and_nested_shipping_binary_is_chosen() {
    let t = Temp::new("haze");
    games_fixture(&t);
    let r = scan_games(&t);
    let haze = game(&r, "Haze1");
    assert_eq!(haze.candidates[0].file_name, "Game-Win64-Shipping.exe");
    assert!(haze.excluded.iter().any(|e| e.path.ends_with("Language Selector.exe")));
}

#[test]
fn nested_unreal_layout_is_one_game() {
    let t = Temp::new("nested");
    t.exe("Games/marvel-vs-capcom/Game/Binaries/Win64/MVC-Win64-Shipping.exe", 40 * 1024);
    let r = scan_games(&t);
    assert_eq!(r.games.len(), 1);
    let g = &r.games[0];
    assert_eq!(g.title, "Marvel vs. Capcom");
    assert!(g.game_dir.ends_with("marvel-vs-capcom"));
    assert!(g.candidates[0].reasons.iter().any(|x| x.contains("initials")), "{:?}", g.candidates[0].reasons);
}

#[test]
fn repack_names_and_unicode_are_resolved() {
    let t = Temp::new("repack");
    games_fixture(&t);
    let r = scan_games(&t);
    let hk = game(&r, "Hollow Knight");
    assert_eq!(hk.candidates[0].file_name, "hollow_knight.exe");
    assert!(hk.candidates[0].reasons.iter().any(|x| x.contains("Unity")));
    assert!(hk.excluded.iter().any(|e| e.path.contains("MD5")));
    let okami = game(&r, "Ōkami HD");
    assert_eq!(okami.candidates[0].file_name, "okami.exe");
    assert!(game(&r, "Call of Duty WaW").candidates[0].file_name == "CoDWaW.exe");
}

#[test]
fn several_plausible_launchers_go_to_review() {
    let t = Temp::new("multi");
    t.exe("Games/Mass Effect Legendary/ME1.exe", 30 * 1024);
    t.exe("Games/Mass Effect Legendary/ME2.exe", 30 * 1024);
    t.exe("Games/Mass Effect Legendary/ME3.exe", 30 * 1024);
    let r = scan_games(&t);
    let g = game(&r, "Mass Effect Legendary");
    assert_eq!(g.candidates.len(), 3);
    assert!(g.uncertain, "close scores must ask for review");
}

#[test]
fn same_name_in_different_folders_are_separate_games() {
    let t = Temp::new("samename");
    let _a = t.exe("Games/Doom/Doom.exe", 5 * 1024);
    let _b = t.exe("Games/PLUG&PLAY/Doom/Doom.exe", 5 * 1024);
    let steam = steam_fixture(&t, &[("1", vec![])]);
    let prefs = prefs_for(&t, &steam);
    let mut lib = Library::default();
    let _guard = serial();
    refresh(&prefs, &mut lib);
    let dooms: Vec<_> = lib.entries.values().filter(|e| e.title == "Doom").collect();
    assert_eq!(dooms.len(), 2);
    assert_ne!(dooms[0].id, dooms[1].id);
    // The second one with the same executable *name* is not a duplicate: different file.
    assert!(dooms.iter().all(|e| e.duplicate.is_none()));
}

#[test]
fn hundreds_of_folders_scan_quickly() {
    let t = Temp::new("many");
    for i in 0..400 {
        t.exe(&format!("Games/Collection {}/Game Number {i}/GameNumber{i}.exe", i % 7), 64);
        t.exe(&format!("Games/Collection {}/Game Number {i}/_Redist/vcredist.exe", i % 7), 1);
    }
    let started = std::time::Instant::now();
    let r = scan_games(&t);
    assert_eq!(r.games.len(), 400);
    assert!(started.elapsed().as_secs() < 30, "took {:?}", started.elapsed());
    assert!(r.folders_scanned >= 800);
}

#[test]
fn cancelling_stops_and_does_not_mark_games_missing() {
    let t = Temp::new("cancel");
    games_fixture(&t);
    let steam = steam_fixture(&t, &[("1", vec![])]);
    let prefs = prefs_for(&t, &steam);
    let _guard = serial();
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    let before = lib.entries.len();
    let summary = bsxl_engine::refresh(&prefs, &mut lib, None, &|_| {}, &AtomicBool::new(true));
    assert!(summary.cancelled);
    assert_eq!(lib.entries.len(), before);
    assert!(lib.entries.values().all(|e| e.status != Status::Missing));
}

#[test]
fn missing_renamed_and_offline_folders() {
    let t = Temp::new("missing");
    games_fixture(&t);
    let steam = steam_fixture(&t, &[("1", vec![])]);
    let mut prefs = prefs_for(&t, &steam);
    let _guard = serial();
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    let halo = id_of(&lib, "Halo");
    lib.reject(&[halo.clone()]);

    // Moved to another folder: decisions travel with it.
    std::fs::rename(t.path("Games/Halo"), t.path("Games/PLUG&PLAY/Halo")).unwrap();
    refresh(&prefs, &mut lib);
    let moved = entry_by_title(&lib, "Halo");
    assert_ne!(moved.id, halo);
    assert_eq!(moved.status, Status::Rejected, "relocated game keeps the user's decision");
    assert!(!lib.entries.contains_key(&halo));

    // Deleted: marked missing, never dropped.
    std::fs::remove_dir_all(t.path("Games/Ōkami HD")).unwrap();
    refresh(&prefs, &mut lib);
    assert_eq!(entry_by_title(&lib, "Ōkami HD").status, Status::Missing);

    // A root that is offline (unplugged drive) does not make its games missing.
    prefs.scan_roots[0].path = t.s("Unplugged");
    let summary = refresh(&prefs, &mut lib);
    assert_eq!(summary.root_errors.len(), 1);
    assert_eq!(entry_by_title(&lib, "A Way Out").status, Status::NeedsReview);
}

#[cfg(windows)]
#[test]
fn junctions_are_not_followed() {
    let t = Temp::new("junction");
    t.exe("Outside/Linked Game/LinkedGame.exe", 2048);
    t.dir("Games");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(t.path("Games/loop"))
        .arg(t.path("Outside"))
        .output();
    if !status.is_ok_and(|o| o.status.success()) {
        return; // junction creation not permitted here
    }
    let r = scan_games(&t);
    assert!(r.games.is_empty(), "{:?}", r.games);
}
