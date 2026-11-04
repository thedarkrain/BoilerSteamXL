//! Regression tests for defects found by the adversarial review. Synthetic fixtures only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::AtomicBool;

use bsxl_engine::{
    import::{
        execute_import, execute_rebuild, list_backups, plan_import, plan_rebuild, point_icons_at_grid, restore_backup, restore_preview, Decision,
        ExecOptions, RebuildDraft, RebuildMode, REBUILD_PHRASE,
    },
    library::{EntryEdit, Library, Status},
    rules::RuleSet,
    scan::{scan, PeCache, ScanRoot},
    vdf,
};
use common::*;

fn opts() -> ExecOptions<'static> {
    ExecOptions { steam_running: &NOT_RUNNING, defer_if_running: false, scheduled: false, only: None }
}

struct World {
    t: Temp,
    steam: std::path::PathBuf,
    prefs: bsxl_engine::prefs::Preferences,
    lib: Library,
}

fn world(label: &str, users: &[&str]) -> World {
    let t = Temp::new(label);
    games_fixture(&t);
    let emu = t.exe("Elsewhere/Emulator/emu.exe", 2048);
    let mut accounts: Vec<(&str, Vec<_>)> = Vec::new();
    for (i, u) in users.iter().enumerate() {
        accounts.push((u, if i == 0 { vec![shortcut("My Emulator", &emu.to_string_lossy(), "-fullscreen")] } else { vec![] }));
    }
    let steam = steam_fixture(&t, &accounts);
    let mut prefs = prefs_for(&t, &steam);
    prefs.simulation_mode = false;
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    World { t, steam, prefs, lib }
}

fn import_all(w: &mut World) -> Result<bsxl_engine::import::Outcome, String> {
    let snap = snap(&w.steam);
    let plan = plan_import(&w.lib, &snap, &w.prefs, None);
    execute_import(&lock(), &mut w.lib, &snap, &w.prefs, &plan.token, &opts())
}

/// Adds a key the shortcut library does not know (Steam writes "sortas", for example).
fn add_unknown_key(path: &std::path::Path) {
    let mut records = vdf::parse(&std::fs::read(path).unwrap()).unwrap();
    records[0].fields.push(vdf::Field { kind: 0x01, name: b"sortas".to_vec(), value: b"Zzz Emulator".to_vec() });
    std::fs::write(path, vdf::serialize(&records)).unwrap();
}

fn sortas(path: &std::path::Path) -> Option<String> {
    vdf::parse(&std::fs::read(path).unwrap()).unwrap().first().and_then(|r| r.get_str("sortas"))
}

#[test]
fn unknown_steam_fields_survive_imports_and_restores_are_byte_exact() {
    let _g = serial();
    let mut w = world("unknownkeys", &["1"]);
    let path = vdf_path(&w.steam, "1");
    add_unknown_key(&path);
    let before = std::fs::read(&path).unwrap();
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    let outcome = import_all(&mut w).unwrap();
    assert_eq!(sortas(&path).as_deref(), Some("Zzz Emulator"), "kept record is byte-for-byte");
    assert!(std::fs::read(&path).unwrap().starts_with(&before[..before.len() - 2]), "existing bytes untouched");

    let backup = outcome.record.backup.unwrap();
    let snapshot = snap(&w.steam);
    let preview = restore_preview(&backup, &snapshot, false).unwrap();
    restore_backup(&lock(), &mut w.lib, &backup, &snapshot, &w.prefs, &preview.token, &NOT_RUNNING).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before, "restore is byte-for-byte");
}

#[test]
fn a_simulated_confirmation_cannot_perform_a_real_write() {
    let _g = serial();
    let mut w = world("simtoken", &["1"]);
    let before = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    w.prefs.simulation_mode = true;
    let snapshot = snap(&w.steam);
    let simulated = plan_import(&w.lib, &snapshot, &w.prefs, None);
    w.prefs.simulation_mode = false;
    assert!(execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &simulated.token, &opts()).is_err());
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);
}

#[test]
fn moved_imported_game_is_updated_in_place_not_duplicated() {
    let _g = serial();
    let mut w = world("relocate", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    import_all(&mut w).unwrap();
    std::fs::rename(w.t.path("Games/Halo"), w.t.path("Games/PLUG&PLAY/Halo")).unwrap();
    refresh(&w.prefs, &mut w.lib);
    let moved = entry_by_title(&w.lib, "Halo").clone();
    assert_eq!(moved.status, Status::UpdateAvailable);
    w.lib.approve(&[moved.id.clone()], false);
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    assert_eq!(plan.summary["update"], 1);
    assert_eq!(plan.summary["create"], 0);
    import_all(&mut w).unwrap();
    let halos: Vec<_> = read_vdf(&w.steam, "1").into_iter().filter(|s| s.app_name == "Halo").collect();
    assert_eq!(halos.len(), 1);
    assert!(halos[0].exe.contains("PLUG&PLAY"));
}

#[test]
fn update_to_an_executable_another_shortcut_uses_is_blocked() {
    let _g = serial();
    let mut w = world("updclash", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    import_all(&mut w).unwrap();
    let emu = w.t.s("Elsewhere/Emulator/emu.exe");
    w.lib.edit(&halo, &EntryEdit { exe: Some(emu), launch_options: Some("-fullscreen".into()), ..EntryEdit::default() }).unwrap();
    w.lib.approve(&[halo], false);
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    assert_eq!(plan.ops.len(), 0);
    assert!(plan.blocked.iter().any(|b| b.reason.contains("My Emulator")));
}

#[test]
fn user_launch_options_survive_rescans_and_reach_steam_as_an_update() {
    let _g = serial();
    let mut w = world("args", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.edit(&halo, &EntryEdit { launch_options: Some("-dx11".into()), ..EntryEdit::default() }).unwrap();
    w.lib.approve(&[halo.clone()], false);
    refresh(&w.prefs, &mut w.lib);
    assert_eq!(w.lib.entries[&halo].launch_options, "-dx11");
    assert_eq!(w.lib.entries[&halo].status, Status::Approved, "approval kept");
    import_all(&mut w).unwrap();

    // Changing them later offers an update that keeps the Steam ID.
    w.lib.edit(&halo, &EntryEdit { launch_options: Some("-novid".into()), ..EntryEdit::default() }).unwrap();
    w.lib.reconcile(&snap(&w.steam));
    assert_eq!(w.lib.entries[&halo].status, Status::UpdateAvailable);
    w.lib.approve(&[halo.clone()], false);
    let before_id = read_vdf(&w.steam, "1").iter().find(|s| s.app_name == "Halo").unwrap().app_id;
    import_all(&mut w).unwrap();
    let halo_sc = read_vdf(&w.steam, "1").into_iter().find(|s| s.app_name == "Halo").unwrap();
    assert_eq!(halo_sc.launch_options, "-novid");
    assert_eq!(halo_sc.app_id, before_id);
    refresh(&w.prefs, &mut w.lib);
    assert_eq!(w.lib.entries[&halo].status, Status::Imported);
}

#[test]
fn an_unreadable_account_does_not_erase_import_records() {
    let _g = serial();
    let mut w = world("damaged", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    import_all(&mut w).unwrap();
    let good = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    std::fs::write(vdf_path(&w.steam, "1"), b"truncated").unwrap();
    refresh(&w.prefs, &mut w.lib);
    assert!(w.lib.entries[&halo].import.is_some());
    assert_eq!(w.lib.entries[&halo].status, Status::Imported);
    std::fs::write(vdf_path(&w.steam, "1"), good).unwrap();
}

#[test]
fn an_invalid_steam_folder_keeps_duplicate_verdicts() {
    let _g = serial();
    let t = Temp::new("nosteam");
    t.exe("SteamLibrary/steamapps/common/Portal 2/portal2.exe", 4096);
    let steam = steam_fixture(&t, &[("1", vec![])]);
    let mut prefs = prefs_for(&t, &steam);
    prefs.scan_roots[0].path = t.s("SteamLibrary/steamapps/common");
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    assert_eq!(entry_by_title(&lib, "Portal 2").status, Status::AlreadyInSteam);
    prefs.steam_path = Some(t.s("Unplugged/Steam"));
    refresh(&prefs, &mut lib);
    assert_eq!(entry_by_title(&lib, "Portal 2").status, Status::AlreadyInSteam);
    assert!(lib.eligible_for_bulk_approval().is_empty());
}

#[test]
fn a_committed_game_keeps_its_approval_against_a_newer_copy() {
    let _g = serial();
    let mut w = world("tiebreak", &["1"]);
    // Added and approved by hand first...
    let exe = w.t.exe("Games/Zed Game/ZedGame.exe", 8 * 1024).to_string_lossy().to_string();
    let manual = w.lib.add_manual(&EntryEdit { exe: Some(exe), ..EntryEdit::default() }, &snap(&w.steam)).unwrap();
    assert!(w.lib.approve(&[manual.clone()], false).is_empty());
    // ...then the folder scan finds the same exe under an id that sorts first ("fs:" < "manual:").
    refresh(&w.prefs, &mut w.lib);
    assert_eq!(w.lib.entries[&manual].status, Status::Approved, "the user's approval survives");
    assert_eq!(entry_by_title(&w.lib, "Zed Game").status, Status::PossibleDuplicate);
}

#[test]
fn withdrawing_an_approved_update_really_withdraws_it() {
    let _g = serial();
    let mut w = world("withdraw", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    import_all(&mut w).unwrap();
    w.lib.edit(&halo, &EntryEdit { launch_options: Some("-x".into()), ..EntryEdit::default() }).unwrap();
    w.lib.approve(&[halo.clone()], false);
    w.lib.restore(&[halo.clone()]);
    assert!(w.lib.entries[&halo].approval.is_none());
    assert_eq!(w.lib.entries[&halo].status, Status::UpdateAvailable);
    assert!(plan_import(&w.lib, &snap(&w.steam), &w.prefs, None).ops.is_empty());
}

#[test]
fn a_failed_multi_account_restore_puts_every_account_back() {
    let _g = serial();
    let mut w = world("restorefail", &["1", "2"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    let outcome = import_all(&mut w).unwrap();
    let after1 = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    let after2 = std::fs::read(vdf_path(&w.steam, "2")).unwrap();
    let backup = outcome.record.backup.unwrap();
    // Break account 2's copy in the backup so its restore fails half-way.
    let dir = bsxl_engine::import::backups_dir().join(&backup);
    std::fs::write(dir.join("2").join("shortcuts.vdf"), b"broken").unwrap();
    let snapshot = snap(&w.steam);
    let preview = restore_preview(&backup, &snapshot, false).unwrap();
    assert!(restore_backup(&lock(), &mut w.lib, &backup, &snapshot, &w.prefs, &preview.token, &NOT_RUNNING).is_err());
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), after1);
    assert_eq!(std::fs::read(vdf_path(&w.steam, "2")).unwrap(), after2);
    assert!(list_backups().iter().any(|b| b.kind == "before-restore"));
}

#[test]
fn rebuild_that_replaces_and_removes_the_same_games_shortcuts_keeps_it_imported() {
    let _g = serial();
    let mut w = world("rebuildorder", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.prefs.target_users = vec!["1".into()];
    w.lib.approve(&[halo.clone()], false);
    import_all(&mut w).unwrap();
    // Replace the emulator shortcut with Halo and remove Halo's current shortcut.
    let s = snap(&w.steam);
    let user = s.user("1").unwrap();
    let emu_key = user.shortcuts.iter().find(|x| x.name == "My Emulator").unwrap().key();
    let halo_key = user.shortcuts.iter().find(|x| x.name == "Halo").unwrap().key();
    let mut draft = RebuildDraft::new(RebuildMode::AllNonSteam, "1");
    draft.approve(&w.lib, &halo).unwrap();
    draft.set_decision(&emu_key, Decision::Replace { entry_id: halo.clone() }).unwrap();
    draft.set_decision(&halo_key, Decision::Remove { confirmed: true }).unwrap();
    w.lib.rebuild_draft = Some(draft);
    let plan = plan_rebuild(&w.lib, &s, &w.prefs).unwrap();
    execute_rebuild(&lock(), &mut w.lib, &s, &w.prefs, &plan.token, REBUILD_PHRASE, true, &NOT_RUNNING).unwrap();
    assert_eq!(w.lib.entries[&halo].status, Status::Imported);
    assert!(w.lib.entries[&halo].import.is_some());
}

#[test]
fn icons_follow_simulation_and_steam_rules_and_are_backed_up() {
    let _g = serial();
    let mut w = world("icons", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    import_all(&mut w).unwrap();
    let app_id = read_vdf(&w.steam, "1").iter().find(|s| s.app_name == "Halo").unwrap().app_id;
    let grid = w.steam.join("userdata/1/config/grid");
    std::fs::create_dir_all(&grid).unwrap();
    std::fs::write(grid.join(format!("{app_id}-icon.png")), b"icon").unwrap();
    add_unknown_key(&vdf_path(&w.steam, "1"));
    let s = snap(&w.steam);
    w.prefs.simulation_mode = true;
    assert!(point_icons_at_grid(&lock(), &s, &w.prefs, false, None, &NOT_RUNNING).is_err());
    w.prefs.simulation_mode = false;
    assert!(point_icons_at_grid(&lock(), &s, &w.prefs, false, None, &RUNNING).is_err());
    assert_eq!(point_icons_at_grid(&lock(), &s, &w.prefs, false, None, &NOT_RUNNING).unwrap(), 1);
    let halo_sc = read_vdf(&w.steam, "1").into_iter().find(|s| s.app_name == "Halo").unwrap();
    assert!(halo_sc.icon.ends_with("-icon.png"));
    assert_eq!(sortas(&vdf_path(&w.steam, "1")).as_deref(), Some("Zzz Emulator"));
    assert!(list_backups().iter().any(|b| b.kind == "icons"));
}

// ---- scanner ------------------------------------------------------------------------

fn scan_dir(t: &Temp, rel: &str, depth: usize) -> bsxl_engine::scan::ScanResult {
    let roots = vec![ScanRoot { path: t.s(rel), enabled: true, max_depth: depth }];
    scan(&roots, &RuleSet::recommended(), &mut PeCache::new(), &|_| {}, &AtomicBool::new(false))
}

#[test]
fn games_inside_a_windows_build_folder_are_found() {
    let t = Temp::new("winfolder");
    t.exe("Games/MyIndie/Windows/MyIndie.exe", 8 * 1024);
    let r = scan_dir(&t, "Games", 6);
    assert_eq!(r.games.len(), 1);
    assert_eq!(r.games[0].title, "MyIndie");
}

#[test]
fn a_publisher_folder_with_a_launcher_does_not_swallow_its_games() {
    let t = Temp::new("publisher");
    t.exe("Games/Rockstar Games/Launcher/Launcher.exe", 4 * 1024);
    t.exe("Games/Rockstar Games/Grand Theft Auto V/GTA5.exe", 60 * 1024);
    t.exe("Games/Rockstar Games/Red Dead Redemption 2/RDR2.exe", 60 * 1024);
    let r = scan_dir(&t, "Games", 6);
    let titles: Vec<&str> = r.games.iter().map(|g| g.title.as_str()).collect();
    assert!(titles.contains(&"Grand Theft Auto V"), "{titles:?}");
    assert!(titles.contains(&"Red Dead Redemption 2"), "{titles:?}");
}

#[test]
fn a_root_that_is_a_game_folder_is_one_game() {
    let t = Temp::new("rootgame");
    t.exe("Cyberpunk 2077/REDprelauncher.exe", 2 * 1024);
    t.exe("Cyberpunk 2077/bin/x64/Cyberpunk2077.exe", 60 * 1024);
    let r = scan_dir(&t, "Cyberpunk 2077", 6);
    assert_eq!(r.games.len(), 1, "{:?}", r.games.iter().map(|g| &g.title).collect::<Vec<_>>());
    assert_eq!(r.games[0].title, "Cyberpunk 2077");
    assert_eq!(r.games[0].candidates.len(), 2);
}

#[test]
fn a_deeper_inner_root_is_still_scanned() {
    let t = Temp::new("nested");
    t.exe("Drive/Emulation/PC/Repacks/Deep Game/bin/DeepGame.exe", 8 * 1024);
    let roots = vec![
        ScanRoot { path: t.s("Drive"), enabled: true, max_depth: 2 },
        ScanRoot { path: t.s("Drive/Emulation/PC/Repacks"), enabled: true, max_depth: 6 },
    ];
    let r = scan(&roots, &RuleSet::recommended(), &mut PeCache::new(), &|_| {}, &AtomicBool::new(false));
    assert_eq!(r.games.len(), 1);
    assert_eq!(r.games[0].title, "Deep Game");
}

#[test]
fn excluded_files_are_shown_with_their_own_game() {
    let t = Temp::new("orphans");
    t.exe("Games/stray.exe", 2 * 1024);
    t.exe("Games/Halo/haloce.exe", 5 * 1024);
    t.exe("Games/Halo/_Redist/vcredist.exe", 1024);
    let r = scan_dir(&t, "Games", 6);
    let halo = r.games.iter().find(|g| g.title == "Halo").unwrap();
    assert_eq!(halo.excluded.len(), 1);
}

#[test]
fn gog_manifest_is_found_after_a_dlc_manifest() {
    let t = Temp::new("gog");
    t.exe("Games/witcher3/bin/x64/witcher3.exe", 60 * 1024);
    t.exe("Games/witcher3/bin/x64/tool.exe", 60 * 1024);
    std::fs::write(t.path("Games/witcher3/goggame-1.info"), r#"{"name":"Some DLC","playTasks":[]}"#).unwrap();
    std::fs::write(
        t.path("Games/witcher3/goggame-2.info"),
        r#"{"name":"The Witcher 3: Wild Hunt - Game of the Year Edition","playTasks":[{"isPrimary":true,"type":"FileTask","path":"bin\\x64\\witcher3.exe"}]}"#,
    )
    .unwrap();
    let r = scan_dir(&t, "Games", 6);
    assert_eq!(r.games[0].title_source, "GOG manifest");
    assert!(r.games[0].candidates[0].reasons.iter().any(|x| x.contains("GOG manifest")));
}

// ---- second review round ------------------------------------------------------------

#[test]
fn a_scan_that_finishes_during_an_import_keeps_the_import_record() {
    let _g = serial();
    let mut w = world("stalesnap", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    w.lib.save().unwrap();
    // The scan runs (no lock) while the import happens; it merges afterwards.
    let discovery = bsxl_engine::discover(&w.prefs, &|_| {}, &AtomicBool::new(false));
    import_all(&mut w).unwrap();
    w.lib.save().unwrap();
    let mut lib = Library::load();
    bsxl_engine::merge(&mut lib, discovery, None, &w.prefs);
    assert!(lib.entries[&halo].import.is_some(), "the import record survives the merge");
    assert_eq!(lib.entries[&halo].status, Status::Imported);
}

#[test]
fn a_game_with_tool_sub_folders_stays_one_game() {
    let t = Temp::new("tools");
    t.exe("Games/Oblivion/Oblivion.exe", 20 * 1024);
    t.exe("Games/Oblivion/Mopy/Wrye Bash.exe", 4 * 1024);
    t.exe("Games/Oblivion/TES4Edit/TES4Edit.exe", 4 * 1024);
    t.exe("Games/Oblivion/LOOT/LOOT.exe", 4 * 1024);
    let r = scan_dir(&t, "Games", 6);
    assert_eq!(r.games.len(), 1, "{:?}", r.games.iter().map(|g| &g.title).collect::<Vec<_>>());
    assert_eq!(r.games[0].candidates[0].file_name, "Oblivion.exe");
}

#[test]
fn an_unreal_game_used_as_root_is_one_game() {
    let t = Temp::new("ueroot");
    t.exe("Hogwarts Legacy/HogwartsLegacy.exe", 300);
    t.exe("Hogwarts Legacy/Phoenix/Binaries/Win64/HogwartsLegacy.exe", 80 * 1024);
    let r = scan_dir(&t, "Hogwarts Legacy", 6);
    assert_eq!(r.games.len(), 1, "{:?}", r.games.iter().map(|g| &g.title).collect::<Vec<_>>());
    assert_eq!(r.games[0].candidates.len(), 2);
}

#[test]
fn overlapping_roots_keep_the_complete_copy() {
    let t = Temp::new("overlap");
    t.exe("Drive/Emulation/PC/Repacks/Deep Game/Launcher.exe", 1024);
    t.exe("Drive/Emulation/PC/Repacks/Deep Game/bin/DeepGame.exe", 40 * 1024);
    let roots = vec![
        ScanRoot { path: t.s("Drive"), enabled: true, max_depth: 4 },
        ScanRoot { path: t.s("Drive/Emulation/PC/Repacks"), enabled: true, max_depth: 6 },
    ];
    let r = scan(&roots, &RuleSet::recommended(), &mut PeCache::new(), &|_| {}, &AtomicBool::new(false));
    assert_eq!(r.games.len(), 1, "{:?}", r.games.iter().map(|g| (&g.title, &g.game_dir, g.candidates.len())).collect::<Vec<_>>());
    assert_eq!(r.games[0].candidates.len(), 2, "the deeper root's copy has the real executable");
}

#[test]
fn a_damaged_shortcuts_file_can_be_restored() {
    let _g = serial();
    let mut w = world("restoredamaged", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    let backup = import_all(&mut w).unwrap().record.backup.unwrap();
    let original = std::fs::read(bsxl_engine::import::backups_dir().join(&backup).join("1").join("shortcuts.vdf")).unwrap();
    std::fs::write(vdf_path(&w.steam, "1"), b"steam crashed while writing").unwrap();
    let snapshot = snap(&w.steam);
    let preview = restore_preview(&backup, &snapshot, false).unwrap();
    restore_backup(&lock(), &mut w.lib, &backup, &snapshot, &w.prefs, &preview.token, &NOT_RUNNING).unwrap();
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), original);
}

#[test]
fn an_update_steam_already_has_is_recorded_not_stuck() {
    let _g = serial();
    let mut w = world("noopupdate", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    import_all(&mut w).unwrap();
    // The user typed the same launch options in Steam and in BoilerSteamXL.
    let path = vdf_path(&w.steam, "1");
    let mut records = vdf::parse(&std::fs::read(&path).unwrap()).unwrap();
    for r in &mut records {
        if r.get_str("AppName").as_deref() == Some("Halo") {
            r.set_str("LaunchOptions", "-safe");
        }
    }
    std::fs::write(&path, vdf::serialize(&records)).unwrap();
    w.lib.edit(&halo, &EntryEdit { launch_options: Some("-safe".into()), ..EntryEdit::default() }).unwrap();
    w.lib.reconcile(&snap(&w.steam));
    assert_eq!(w.lib.entries[&halo].status, Status::Imported);
}

// ---- artwork ------------------------------------------------------------------------

#[test]
fn cached_artwork_is_copied_into_steam_on_import_only() {
    let _g = serial();
    let mut w = world("artimport", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    let cache = bsxl_engine::artwork::cache_dir(&halo);
    std::fs::create_dir_all(&cache).unwrap();
    for (name, bytes) in [("grid.png", &b"cover"[..]), ("hero.jpg", b"hero"), ("logo.png", b"logo"), ("icon.ico", b"icon")] {
        std::fs::write(cache.join(name), bytes).unwrap();
    }
    w.lib.approve(&[halo.clone()], false);
    let grid = w.steam.join("userdata/1/config/grid");

    // Simulation: the plan announces the artwork, nothing is copied.
    w.prefs.simulation_mode = true;
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    assert!(plan.ops[0].notes.iter().any(|n| n.contains("cover")));
    import_all(&mut w).unwrap();
    assert!(!grid.exists() || std::fs::read_dir(&grid).unwrap().count() == 0);

    // A real import copies it under the new shortcut's id and points the icon at it.
    w.prefs.simulation_mode = false;
    import_all(&mut w).unwrap();
    let sc = read_vdf(&w.steam, "1").into_iter().find(|s| s.app_name == "Halo").unwrap();
    assert!(grid.join(format!("{}p.png", sc.app_id)).is_file());
    assert!(grid.join(format!("{}_hero.jpg", sc.app_id)).is_file());
    assert!(grid.join(format!("{}_logo.png", sc.app_id)).is_file());
    assert!(grid.join(format!("{}-icon.ico", sc.app_id)).is_file());
    assert!(sc.icon.ends_with("-icon.ico"), "{}", sc.icon);
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn existing_steam_artwork_is_never_replaced() {
    let t = Temp::new("artkeep");
    let user = t.dir("userdata/1");
    let grid = user.join("config/grid");
    std::fs::create_dir_all(&grid).unwrap();
    std::fs::write(grid.join("42p.jpg"), b"mine").unwrap();
    let _g = serial();
    let id = "fs:test-keep";
    let cache = bsxl_engine::artwork::cache_dir(id);
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("grid.png"), b"downloaded").unwrap();
    std::fs::write(cache.join("hero.png"), b"downloaded").unwrap();
    assert_eq!(bsxl_engine::artwork::copy_to_grid(id, &user.to_string_lossy(), 42), 1, "only the hero is new");
    assert_eq!(std::fs::read(grid.join("42p.jpg")).unwrap(), b"mine");
    let _ = std::fs::remove_dir_all(&cache);
}

#[cfg(windows)]
#[test]
fn the_executables_own_icon_is_extracted_without_running_it() {
    let _g = serial();
    let notepad = r"C:\Windows\System32\notepad.exe";
    if !std::path::Path::new(notepad).is_file() {
        return;
    }
    let icon = bsxl_engine::artwork::exe_icon("fs:icon-test", notepad).unwrap();
    let bytes = std::fs::read(&icon).unwrap();
    assert_eq!(&bytes[..4], &[0, 0, 1, 0], "an .ico file");
    let _ = std::fs::remove_dir_all(bsxl_engine::artwork::cache_dir("fs:icon-test"));
}

#[test]
fn removing_from_steam_needs_a_confirmed_sync_and_touches_nothing_else() {
    let _g = serial();
    let mut w = world("removals", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    import_all(&mut w).unwrap();
    let before = read_vdf(&w.steam, "1");
    assert_eq!(before.len(), 2, "the emulator and Halo");

    // Ask for Halo's shortcut to go.
    let s = snap(&w.steam);
    let halo_sc = s.user("1").unwrap().shortcuts.iter().find(|x| x.name == "Halo").unwrap().clone();
    assert_eq!(w.lib.entry_for_shortcut(&halo_sc).map(|e| e.id.as_str()), Some(halo.as_str()));
    w.lib.request_removal(&halo_sc, true);

    // An unattended sync (`only`) never removes.
    let plan = plan_import(&w.lib, &s, &w.prefs, Some(&[halo.clone()]));
    assert!(plan.ops.iter().all(|o| o.kind != bsxl_engine::import::OpKind::Remove));

    // Simulation: planned and reported, nothing written, still pending.
    w.prefs.simulation_mode = true;
    let plan = plan_import(&w.lib, &s, &w.prefs, None);
    assert_eq!(plan.summary["remove"], 1);
    import_all(&mut w).unwrap();
    assert_eq!(read_vdf(&w.steam, "1").len(), 2);
    assert_eq!(w.lib.removals.len(), 1);

    // Steam running: refused, nothing written.
    w.prefs.simulation_mode = false;
    let s = snap(&w.steam);
    let plan = plan_import(&w.lib, &s, &w.prefs, None);
    let running = ExecOptions { steam_running: &RUNNING, defer_if_running: false, scheduled: false, only: None };
    assert!(execute_import(&lock(), &mut w.lib, &s, &w.prefs, &plan.token, &running).is_err());
    assert_eq!(read_vdf(&w.steam, "1").len(), 2);

    // A confirmed sync removes only that shortcut, with a backup, and the game is not suggested again.
    let outcome = import_all(&mut w).unwrap();
    assert!(outcome.record.backup.is_some());
    let after = read_vdf(&w.steam, "1");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].app_name, "My Emulator");
    assert_eq!(after[0].launch_options, "-fullscreen");
    assert!(w.lib.removals.is_empty());
    let entry = &w.lib.entries[&halo];
    assert_eq!(entry.status, Status::Rejected);
    assert!(entry.import.is_none());
    assert!(std::path::Path::new(&entry.exe).is_file(), "the game's files are untouched");
    refresh(&w.prefs, &mut w.lib);
    assert_eq!(w.lib.entries[&halo].status, Status::Rejected);
    assert!(plan_import(&w.lib, &snap(&w.steam), &w.prefs, None).ops.is_empty());
}

#[test]
fn a_removal_that_no_longer_matches_is_skipped_with_a_warning() {
    let _g = serial();
    let mut w = world("removalgone", &["1"]);
    let s = snap(&w.steam);
    let mut emu = s.user("1").unwrap().shortcuts[0].clone();
    emu.name = "Renamed in Steam since".into();
    w.lib.request_removal(&emu, true);
    let plan = plan_import(&w.lib, &s, &w.prefs, None);
    assert!(plan.ops.is_empty());
    assert!(plan.warnings.iter().any(|x| x.contains("no longer in Steam")));
}

#[test]
fn merging_duplicates_keeps_the_chosen_copy_linked() {
    let _g = serial();
    let mut w = world("merge", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    let exe = w.lib.entries[&halo].exe.clone();
    // Steam has the game twice under different names; the path is what makes them the same.
    let mut list = read_vdf(&w.steam, "1");
    list.push(shortcut("Halo CE", &exe, ""));
    list.push(shortcut("HALO (old)", &exe, ""));
    write_vdf(&vdf_path(&w.steam, "1"), &list);
    refresh(&w.prefs, &mut w.lib);
    assert_eq!(w.lib.entries[&halo].status, Status::AlreadyInSteam, "never added a third time");
    assert!(plan_import(&w.lib, &snap(&w.steam), &w.prefs, None).ops.is_empty());

    // Keep "HALO (old)", remove "Halo CE".
    let s = snap(&w.steam);
    let keep = s.user("1").unwrap().shortcuts.iter().find(|x| x.name == "HALO (old)").unwrap().clone();
    let drop = s.user("1").unwrap().shortcuts.iter().find(|x| x.name == "Halo CE").unwrap().clone();
    w.lib.link(&halo, &keep).unwrap();
    w.lib.request_removal(&drop, true);
    import_all(&mut w).unwrap();

    let names: Vec<String> = read_vdf(&w.steam, "1").into_iter().map(|x| x.app_name).collect();
    assert_eq!(names, ["My Emulator", "HALO (old)"]);
    let entry = &w.lib.entries[&halo];
    assert_eq!(entry.status, Status::AlreadyInSteam);
    assert_eq!(entry.linked.as_ref().and_then(|l| l.app_id), Some(keep.app_id));
}

// ---- fourth review --------------------------------------------------------------------

#[test]
fn renaming_an_imported_game_renames_it_in_steam_at_the_next_sync() {
    let _g = serial();
    let mut w = world("rename", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    import_all(&mut w).unwrap();
    w.lib.edit(&halo, &EntryEdit { title: Some("Halo Combat Evolved".into()), ..EntryEdit::default() }).unwrap();
    w.lib.reconcile(&snap(&w.steam));
    assert_eq!(w.lib.entries[&halo].status, Status::Imported, "a rename needs no extra approval");
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    let op = plan.ops.iter().find(|o| o.entry_id.as_deref() == Some(halo.as_str())).unwrap();
    assert_eq!(op.kind, bsxl_engine::import::OpKind::Update);
    assert_eq!(op.new.as_ref().unwrap().name, "Halo Combat Evolved");
    import_all(&mut w).unwrap();
    assert!(read_vdf(&w.steam, "1").iter().any(|s| s.app_name == "Halo Combat Evolved"));
    w.lib.reconcile(&snap(&w.steam));
    assert_eq!(w.lib.entries[&halo].status, Status::Imported, "no update loop");
}

#[test]
fn artwork_downloaded_after_the_preview_keeps_the_token() {
    let _g = serial();
    let mut w = world("arttoken", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    let snapshot = snap(&w.steam);
    let before = plan_import(&w.lib, &snapshot, &w.prefs, None);
    let cache = bsxl_engine::artwork::cache_dir(&halo);
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("icon.png"), b"icon").unwrap();
    let after = plan_import(&w.lib, &snapshot, &w.prefs, None);
    assert_ne!(before.ops, after.ops, "the icon shows up in the plan");
    assert_eq!(before.token, after.token);
    execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &before.token, &opts()).unwrap();
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn launcher_games_sharing_one_launcher_exe_are_all_added() {
    let _g = serial();
    let mut w = world("launchers", &["1"]);
    let launcher = w.t.exe("Launchers/launcher.exe", 1024).to_string_lossy().to_string();
    let halo = id_of(&w.lib, "Halo");
    let cod = id_of(&w.lib, "Call of Duty WaW");
    for (id, args) in [(&halo, "launch://halo"), (&cod, "launch://cod")] {
        w.lib.edit(id, &EntryEdit { exe: Some(launcher.clone()), launch_options: Some(args.into()), ..EntryEdit::default() }).unwrap();
    }
    w.lib.reconcile(&snap(&w.steam));
    w.lib.approve(&[halo.clone(), cod.clone()], false);
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    assert_eq!(plan.ops.iter().filter(|o| o.kind == bsxl_engine::import::OpKind::Create).count(), 2, "{:?}", plan.blocked);
}

#[test]
fn a_folder_match_in_clean_rebuild_is_never_applied_in_bulk() {
    let _g = serial();
    let t = Temp::new("rbfolder");
    games_fixture(&t);
    let other = t.exe("Games/PLUG&PLAY/Haze1/Tools/HazeTool.exe", 1024);
    let steam = steam_fixture(&t, &[("1", vec![shortcut("Haze tool", &other.to_string_lossy(), "")])]);
    let prefs = prefs_for(&t, &steam);
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    let i = bsxl_engine::import::inspect(&lib, &snap(&steam), &prefs, "1", RebuildMode::AllNonSteam);
    let s = i.shortcuts.iter().find(|s| s.shortcut.name == "Haze tool").unwrap().suggestion.clone().unwrap();
    assert_eq!(s.evidence, "same folder");
    assert_eq!(s.title, "Haze1");
    assert!(!s.strong);
}

#[test]
fn a_leftover_steam_icon_is_the_one_the_shortcut_points_at() {
    let t = Temp::new("iconext");
    let user = t.dir("userdata/1");
    let grid = user.join("config/grid");
    std::fs::create_dir_all(&grid).unwrap();
    std::fs::write(grid.join("42-icon.png"), b"old").unwrap();
    let _g = serial();
    let id = "fs:test-iconext";
    let cache = bsxl_engine::artwork::cache_dir(id);
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("icon.ico"), b"new").unwrap();
    let folder = user.to_string_lossy().to_string();
    let path = bsxl_engine::artwork::grid_icon_path(&folder, id, 42).unwrap();
    assert!(path.ends_with("42-icon.png"), "{path}");
    bsxl_engine::artwork::copy_to_grid(id, &folder, 42);
    assert!(std::path::Path::new(&path).is_file());
    assert!(bsxl_engine::artwork::missing_in_grid(id, &folder, 42).is_empty());
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn crack_and_unlocker_folders_do_not_split_a_repack() {
    let t = Temp::new("crack");
    t.exe("Games/Hitman Absolution/HMA.exe", 40 * 1024);
    t.exe("Games/Hitman Absolution/Crack/HMA.exe", 40 * 1024);
    t.exe("Games/Hitman Absolution/DLC Unlocker/Unlocker.exe", 1024);
    let r = scan_dir(&t, "Games", 6);
    assert_eq!(r.games.len(), 1, "{:?}", r.games.iter().map(|g| &g.title).collect::<Vec<_>>());
    assert_eq!(r.games[0].title, "Hitman Absolution");
}

#[test]
fn choosing_one_icon_changes_only_that_shortcut() {
    let _g = serial();
    let mut w = world("oneicon", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    import_all(&mut w).unwrap();
    let shortcuts = read_vdf(&w.steam, "1");
    let halo_id = shortcuts.iter().find(|s| s.app_name == "Halo").unwrap().app_id;
    let emu_id = shortcuts.iter().find(|s| s.app_name == "My Emulator").unwrap().app_id;
    let grid = w.steam.join("userdata/1/config/grid");
    std::fs::create_dir_all(&grid).unwrap();
    std::fs::write(grid.join(format!("{halo_id}-icon.png")), b"icon").unwrap();
    std::fs::write(grid.join(format!("{emu_id}-icon.png")), b"icon").unwrap();
    let s = snap(&w.steam);
    assert_eq!(point_icons_at_grid(&lock(), &s, &w.prefs, false, Some(("1", halo_id)), &NOT_RUNNING).unwrap(), 1);
    let after = read_vdf(&w.steam, "1");
    assert!(after.iter().find(|s| s.app_name == "Halo").unwrap().icon.ends_with("-icon.png"));
    assert!(after.iter().find(|s| s.app_name == "My Emulator").unwrap().icon.is_empty(), "other shortcuts are left alone");
}

#[test]
fn renaming_a_game_linked_to_another_tools_shortcut_renames_only_that_shortcut() {
    let _g = serial();
    let mut w = world("renamelinked", &["1"]);
    let halo = id_of(&w.lib, "Halo");
    let exe = w.lib.entries[&halo].exe.clone();
    let mut list = read_vdf(&w.steam, "1");
    list.push(shortcut("HALO_CE", &exe, ""));
    write_vdf(&vdf_path(&w.steam, "1"), &list);
    refresh(&w.prefs, &mut w.lib);
    assert_eq!(w.lib.entries[&halo].status, Status::AlreadyInSteam);
    let raw_before = vdf::parse(&std::fs::read(vdf_path(&w.steam, "1")).unwrap()).unwrap();

    w.lib.edit(&halo, &EntryEdit { title: Some("Halo: Combat Evolved".into()), ..EntryEdit::default() }).unwrap();
    // An unattended sync never renames.
    assert!(plan_import(&w.lib, &snap(&w.steam), &w.prefs, Some(&[halo.clone()])).ops.is_empty());
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    assert_eq!(plan.ops.len(), 1);
    assert_eq!(plan.ops[0].kind, bsxl_engine::import::OpKind::Update);
    assert_eq!(plan.ops[0].new.as_ref().unwrap().name, "Halo: Combat Evolved");
    import_all(&mut w).unwrap();

    let raw = vdf::parse(&std::fs::read(vdf_path(&w.steam, "1")).unwrap()).unwrap();
    assert_eq!(raw[1].get_str("AppName").as_deref(), Some("Halo: Combat Evolved"));
    assert_eq!(raw[1].get_str("Exe"), raw_before[1].get_str("Exe"), "the target is written exactly as before");
    assert_eq!(raw[1].app_id(), raw_before[1].app_id(), "same Steam ID, so artwork stays");
    assert_eq!(raw[0].fields, raw_before[0].fields, "other shortcuts untouched");
    let entry = &w.lib.entries[&halo];
    assert_eq!(entry.status, Status::AlreadyInSteam);
    assert!(entry.import.is_none(), "still the other tool's shortcut, only linked");
    assert!(plan_import(&w.lib, &snap(&w.steam), &w.prefs, None).ops.is_empty(), "done once");

    // Renamed again in Steam itself: BoilerSteamXL does not fight it.
    let mut list = read_vdf(&w.steam, "1");
    list[1].app_name = "My own name".into();
    write_vdf(&vdf_path(&w.steam, "1"), &list);
    refresh(&w.prefs, &mut w.lib);
    assert!(plan_import(&w.lib, &snap(&w.steam), &w.prefs, None).ops.is_empty());
}

fn game(t: &Temp, dir: &str, exe: &str) -> bsxl_engine::scan::DiscoveredGame {
    let path = t.exe(exe, 2048);
    bsxl_engine::scan::DiscoveredGame {
        game_dir: t.path(dir).to_string_lossy().to_string(),
        title: dir.rsplit('/').next().unwrap().to_string(),
        title_source: "Folder name".into(),
        root: t.path("Games").to_string_lossy().to_string(),
        candidates: vec![bsxl_engine::scan::Candidate {
            path: path.to_string_lossy().to_string(),
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            score: 80,
            size: 2048 * 1024,
            reasons: vec![],
            description: None,
            arguments: None,
        }],
        excluded: vec![],
        confidence: 80,
        uncertain: false,
    }
}

#[test]
fn a_rescan_that_regroups_a_crack_subfolder_leaves_one_game_and_no_missing_entry() {
    let _g = serial();
    let t = Temp::new("regroup");
    let steam = steam_fixture(&t, &[("1", vec![])]);
    let snapshot = snap(&steam);
    let roots = vec![t.path("Games").to_string_lossy().to_string()];
    let mut lib = Library::default();

    // Before: the NoDVD sub-folder was found as its own game; the user renamed it and said "don't add".
    let sub = game(&t, "Games/Split Fiction/NoDVD/Goldberg/Split", "Games/Split Fiction/NoDVD/Goldberg/Split/Binaries/Win64/loader.exe");
    let first = bsxl_engine::scan::ScanResult { games: vec![sub], roots_scanned: roots.clone(), ..Default::default() };
    lib.apply_scan(&first, None, &snapshot);
    let old_id = lib.entries.keys().next().unwrap().clone();
    lib.edit(&old_id, &EntryEdit { title: Some("Split Fiction".into()), ..EntryEdit::default() }).unwrap();
    lib.reject(&[old_id.clone()]);

    // After: the scanner groups the whole repack folder as one game.
    let whole = game(&t, "Games/Split Fiction", "Games/Split Fiction/Split/Binaries/Win64/SplitFiction.exe");
    let second = bsxl_engine::scan::ScanResult { games: vec![whole], roots_scanned: roots.clone(), ..Default::default() };
    lib.apply_scan(&second, None, &snapshot);
    assert_eq!(lib.entries.len(), 1, "{:?}", lib.entries.keys().collect::<Vec<_>>());
    let e = lib.entries.values().next().unwrap();
    assert_ne!(e.status, Status::Missing);
    assert_eq!(e.status, Status::Rejected, "the user's decision carried over");
    assert_eq!(e.display_title(), "Split Fiction");

    // And scanning again changes nothing.
    lib.apply_scan(&second, None, &snapshot);
    assert_eq!(lib.entries.len(), 1);
    assert_eq!(lib.entries.values().next().unwrap().status, Status::Rejected);
}

#[test]
fn an_old_or_backup_copy_of_the_exe_is_not_chosen() {
    let _g = serial();
    let t = Temp::new("oldcopy");
    t.exe("Games/Spiderman 2 2004/System/Webhead[old].exe", 4096);
    t.exe("Games/Spiderman 2 2004/System/Webhead.exe", 4096);
    let steam = steam_fixture(&t, &[("1", vec![])]);
    let prefs = prefs_for(&t, &steam);
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    let e = lib.entries.values().find(|e| e.game_dir.contains("Spiderman")).unwrap();
    assert!(e.exe.ends_with("Webhead.exe"), "{}", e.exe);
}
