//! Clean Rebuild tests on synthetic Steam accounts and shortcuts.vdf fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use bsxl_engine::{
    import::{
        execute_import, execute_rebuild, inspect, plan_import, plan_rebuild, restore_backup, restore_preview, Decision,
        ExecOptions, RebuildDraft, RebuildMode, REBUILD_PHRASE,
    },
    library::{Library, Status},
    prefs::Preferences,
};
use common::*;

struct World {
    t: Temp,
    steam: std::path::PathBuf,
    prefs: Preferences,
    lib: Library,
}

/// Account 1 has: a manual emulator shortcut, a duplicate pair for A Way Out (BoilR-made
/// and manual), a broken shortcut, a QuickSFV shortcut, and artwork for one of them.
fn world(label: &str) -> World {
    let t = Temp::new(label);
    games_fixture(&t);
    let emu = t.exe("Elsewhere/Emulator/emu.exe", 2048);
    let awo = t.s("Games/PLUG&PLAY/A Way Out/AWayOut.exe");
    let mut boilr = shortcut("A Way Out", &awo, "");
    boilr.dev_kit_game_id = "boilr".into();
    let copy = shortcut("A Way Out Copy", &awo, "");
    let broken = shortcut("Old Game", "C:\\Gone\\old.exe", "");
    let sfv = shortcut("QuickSFV", &t.s("Games/PLUG&PLAY/A Way Out/QuickSFV.EXE"), "");
    let marvel = shortcut("Marvel_vs_Capcom_Launcher", &t.s("Games/PLUG&PLAY/marvel-vs-capcom/game.exe"), "");
    let steam = steam_fixture(
        &t,
        &[
            ("1", vec![shortcut("My Emulator", &emu.to_string_lossy(), "-fullscreen"), boilr, copy, broken, sfv, marvel.clone()]),
            ("2", vec![shortcut("Other Account Game", &emu.to_string_lossy(), "")]),
        ],
    );
    let grid = steam.join("userdata/1/config/grid");
    std::fs::create_dir_all(&grid).unwrap();
    std::fs::write(grid.join(format!("{}p.png", marvel.app_id)), b"cover").unwrap();
    std::fs::write(grid.join(format!("{}_hero.png", marvel.app_id)), b"hero").unwrap();
    let mut prefs = prefs_for(&t, &steam);
    prefs.simulation_mode = false;
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    World { t, steam, prefs, lib }
}

fn key_of(w: &World, name: &str) -> String {
    snap(&w.steam).user("1").unwrap().shortcuts.iter().find(|s| s.name == name).unwrap().key()
}

fn execute(w: &mut World) -> Result<bsxl_engine::library::RunRecord, String> {
    let snapshot = snap(&w.steam);
    let plan = plan_rebuild(&w.lib, &snapshot, &w.prefs)?;
    execute_rebuild(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, REBUILD_PHRASE, true, &NOT_RUNNING)
}

#[test]
fn enumerates_non_steam_shortcuts_and_duplicates() {
    let _g = serial();
    let w = world("enumerate");
    let i = inspect(&w.lib, &snap(&w.steam), &w.prefs, "1", RebuildMode::AllNonSteam);
    assert_eq!(i.counts["total"], 6);
    assert_eq!(i.counts["boilr"], 1);
    assert_eq!(i.counts["broken"], 1);
    assert_eq!(i.groups.len(), 1, "the two A Way Out shortcuts");
    assert_eq!(i.groups[0].keys.len(), 2);
    assert!(i.shortcuts.iter().any(|s| s.shortcut.name == "QuickSFV" && s.utility.is_some()));
    let marvel = i.shortcuts.iter().find(|s| s.shortcut.name == "Marvel_vs_Capcom_Launcher").unwrap();
    assert_eq!(marvel.suggestion.as_ref().unwrap().title, "Marvel vs. Capcom");
    assert_eq!(marvel.artwork.len(), 2);
    // Managed mode: none of these were made by BoilerSteamXL.
    let managed = inspect(&w.lib, &snap(&w.steam), &w.prefs, "1", RebuildMode::Managed);
    assert_eq!(managed.counts["in_scope"], 0);
}

fn full_rebuild_decisions(w: &mut World) {
    let marvel_id = id_of(&w.lib, "Marvel vs. Capcom");
    let awo_id = id_of(&w.lib, "A Way Out");
    let halo_id = id_of(&w.lib, "Halo");
    let mut draft = RebuildDraft::new(RebuildMode::AllNonSteam, "1");
    for id in [&marvel_id, &awo_id, &halo_id] {
        draft.approve(&w.lib, id).unwrap();
    }
    draft.set_decision(&key_of(w, "My Emulator"), Decision::Keep).unwrap();
    draft.set_decision(&key_of(w, "A Way Out"), Decision::Replace { entry_id: awo_id.clone() }).unwrap();
    assert!(draft.set_decision(&key_of(w, "A Way Out Copy"), Decision::Replace { entry_id: awo_id.clone() }).is_err(), "one game, one shortcut");
    draft.set_decision(&key_of(w, "A Way Out Copy"), Decision::Remove { confirmed: true }).unwrap();
    draft.set_decision(&key_of(w, "Old Game"), Decision::Remove { confirmed: true }).unwrap();
    draft.set_decision(&key_of(w, "QuickSFV"), Decision::Remove { confirmed: true }).unwrap();
    draft.set_decision(&key_of(w, "Marvel_vs_Capcom_Launcher"), Decision::Replace { entry_id: marvel_id }).unwrap();
    w.lib.rebuild_draft = Some(draft);
}

#[test]
fn full_rebuild_does_exactly_the_plan() {
    let _g = serial();
    let mut w = world("rebuild");
    let account2 = std::fs::read(vdf_path(&w.steam, "2")).unwrap();
    let manifest_before = std::fs::read(w.t.path("SteamLibrary/steamapps/appmanifest_620.acf")).unwrap();
    full_rebuild_decisions(&mut w);
    let snapshot = snap(&w.steam);
    let plan = plan_rebuild(&w.lib, &snapshot, &w.prefs).unwrap();
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
    assert_eq!(plan.summary["before"], 6);
    assert_eq!(plan.summary["keep"], 1);
    assert_eq!(plan.summary["replace"], 2);
    assert_eq!(plan.summary["remove"], 3);
    assert_eq!(plan.summary["create"], 1, "Halo is new");
    assert_eq!(plan.summary["after"], 4);
    assert_eq!(plan.summary["duplicates_created"], 0);

    let record = execute(&mut w).unwrap();
    let after = read_vdf(&w.steam, "1");
    assert_eq!(after.len(), plan.summary["after"]);
    let names: Vec<&str> = after.iter().map(|s| s.app_name.as_str()).collect();
    assert_eq!(names, ["My Emulator", "A Way Out", "Marvel vs. Capcom", "Halo"]);
    assert_eq!(after[0].launch_options, "-fullscreen", "kept shortcut unchanged");

    // Game files untouched, other account and official games untouched.
    assert!(w.t.path("Games/PLUG&PLAY/A Way Out/QuickSFV.EXE").exists());
    assert!(w.t.path("Games/PLUG&PLAY/A Way Out/AWayOut.exe").exists());
    assert_eq!(std::fs::read(vdf_path(&w.steam, "2")).unwrap(), account2);
    assert_eq!(std::fs::read(w.t.path("SteamLibrary/steamapps/appmanifest_620.acf")).unwrap(), manifest_before);

    // Artwork migrated to the new Steam ID; originals kept.
    let grid = w.steam.join("userdata/1/config/grid");
    let marvel_new = after.iter().find(|s| s.app_name == "Marvel vs. Capcom").unwrap().app_id;
    assert!(grid.join(format!("{marvel_new}p.png")).exists());
    assert!(grid.join(format!("{marvel_new}_hero.png")).exists());
    assert_eq!(std::fs::read_dir(&grid).unwrap().count(), 4);

    // Backup with artwork, history and library state recorded.
    let backup = record.backup.clone().unwrap();
    let manifest = bsxl_engine::import::list_backups().into_iter().find(|b| b.id == backup).unwrap();
    assert!(manifest.verified);
    assert_eq!(manifest.users[0].grid_files.len(), 2);
    assert!(manifest.plan.is_some());
    assert_eq!(w.lib.entries[&id_of(&w.lib, "Marvel vs. Capcom")].status, Status::Imported);
    assert!(w.lib.rebuild_draft.is_none());
    assert_eq!(w.lib.history[0].kind, "rebuild");
}

#[test]
fn rebuild_is_idempotent() {
    let _g = serial();
    let mut w = world("idempotent");
    full_rebuild_decisions(&mut w);
    execute(&mut w).unwrap();
    let first = read_vdf(&w.steam, "1");
    refresh(&w.prefs, &mut w.lib);

    // Second rebuild: managed mode, replace each managed shortcut with its game again.
    let snapshot = snap(&w.steam);
    let inspection = inspect(&w.lib, &snapshot, &w.prefs, "1", RebuildMode::Managed);
    let mut draft = RebuildDraft::new(RebuildMode::Managed, "1");
    for s in inspection.shortcuts.iter().filter(|s| s.in_scope) {
        let suggestion = s.suggestion.as_ref().unwrap();
        assert_eq!(suggestion.evidence, "identity");
        draft.approve(&w.lib, &suggestion.entry_id).unwrap();
        draft.set_decision(&s.key, Decision::Replace { entry_id: suggestion.entry_id.clone() }).unwrap();
    }
    w.lib.rebuild_draft = Some(draft);
    execute(&mut w).unwrap();
    let second = read_vdf(&w.steam, "1");
    let ids = |v: &[steam_shortcuts_util::shortcut::ShortcutOwned]| v.iter().map(|s| (s.app_id, s.exe.clone())).collect::<Vec<_>>();
    assert_eq!(ids(&first), ids(&second));

    // A normal import afterwards finds nothing to add.
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    assert!(plan.ops.is_empty());
}

#[test]
fn unapproved_or_unconfirmed_operations_never_run() {
    let _g = serial();
    let mut w = world("guards");
    let before = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    let marvel = id_of(&w.lib, "Marvel vs. Capcom");
    let mut draft = RebuildDraft::new(RebuildMode::AllNonSteam, "1");
    // Replacement chosen but not approved in this rebuild; removal not confirmed.
    draft.set_decision(&key_of(&w, "Marvel_vs_Capcom_Launcher"), Decision::Replace { entry_id: marvel.clone() }).unwrap();
    draft.set_decision(&key_of(&w, "Old Game"), Decision::Remove { confirmed: false }).unwrap();
    w.lib.rebuild_draft = Some(draft);
    let snapshot = snap(&w.steam);
    let plan = plan_rebuild(&w.lib, &snapshot, &w.prefs).unwrap();
    assert_eq!(plan.blocked.len(), 2);
    assert!(execute(&mut w).is_err());

    // Approved and confirmed, but the phrase or acknowledgement is missing.
    if let Some(d) = w.lib.rebuild_draft.as_mut() {
        d.approved.clear();
        d.decisions.clear();
    }
    let mut d = w.lib.rebuild_draft.clone().unwrap();
    d.approve(&w.lib, &marvel).unwrap();
    d.set_decision(&key_of(&w, "Marvel_vs_Capcom_Launcher"), Decision::Replace { entry_id: marvel }).unwrap();
    w.lib.rebuild_draft = Some(d);
    let plan = plan_rebuild(&w.lib, &snapshot, &w.prefs).unwrap();
    assert!(execute_rebuild(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, "rebuild", true, &NOT_RUNNING).is_err());
    assert!(execute_rebuild(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, REBUILD_PHRASE, false, &NOT_RUNNING).is_err());
    assert!(execute_rebuild(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, REBUILD_PHRASE, true, &RUNNING).is_err());
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);

    // Undecided shortcuts are left exactly as they are.
    let unresolved = plan.ops.iter().filter(|o| o.kind == bsxl_engine::import::OpKind::Unresolved).count();
    assert_eq!(unresolved, 5);
}

#[test]
fn approval_from_before_the_rebuild_does_not_count() {
    let _g = serial();
    let mut w = world("oldapproval");
    let marvel = id_of(&w.lib, "Marvel vs. Capcom");
    w.lib.approve(&[marvel.clone()], false);
    let mut draft = RebuildDraft::new(RebuildMode::AllNonSteam, "1");
    draft.set_decision(&key_of(&w, "Marvel_vs_Capcom_Launcher"), Decision::Replace { entry_id: marvel }).unwrap();
    w.lib.rebuild_draft = Some(draft);
    let plan = plan_rebuild(&w.lib, &snap(&w.steam), &w.prefs).unwrap();
    assert_eq!(plan.blocked.len(), 1);
}

#[test]
fn external_change_after_preview_stops_and_leaves_a_valid_file() {
    let _g = serial();
    let mut w = world("external");
    full_rebuild_decisions(&mut w);
    let snapshot = snap(&w.steam);
    let plan = plan_rebuild(&w.lib, &snapshot, &w.prefs).unwrap();
    let mut changed = read_vdf(&w.steam, "1");
    changed.push(shortcut("Added while previewing", "C:\\new.exe", ""));
    write_vdf(&vdf_path(&w.steam, "1"), &changed);
    let err = execute_rebuild(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, REBUILD_PHRASE, true, &NOT_RUNNING).unwrap_err();
    assert!(err.contains("changed"), "{err}");
    assert_eq!(read_vdf(&w.steam, "1").len(), 7, "file intact and parseable");
}

#[test]
fn rebuild_can_be_rolled_back() {
    let _g = serial();
    let mut w = world("rollback");
    let original = read_vdf(&w.steam, "1");
    full_rebuild_decisions(&mut w);
    let record = execute(&mut w).unwrap();
    let backup = record.backup.unwrap();
    let snapshot = snap(&w.steam);
    let preview = restore_preview(&backup, &snapshot, w.prefs.simulation_mode).unwrap();
    assert!(preview.warnings.is_empty(), "nothing changed since the rebuild");
    restore_backup(&lock(), &mut w.lib, &backup, &snapshot, &w.prefs, &preview.token, &NOT_RUNNING).unwrap();
    assert_eq!(read_vdf(&w.steam, "1"), original);

    // A later change in Steam is warned about before another restore.
    let mut later = read_vdf(&w.steam, "1");
    later.pop();
    write_vdf(&vdf_path(&w.steam, "1"), &later);
    let preview = restore_preview(&backup, &snap(&w.steam), w.prefs.simulation_mode).unwrap();
    assert!(!preview.warnings.is_empty());
}

#[test]
fn managed_mode_only_touches_boilersteamxl_shortcuts() {
    let _g = serial();
    let mut w = world("managed");
    // Import Halo normally so one managed shortcut exists.
    let halo = id_of(&w.lib, "Halo");
    w.prefs.target_users = vec!["1".into()];
    w.lib.approve(&[halo.clone()], false);
    let snapshot = snap(&w.steam);
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    let opts = ExecOptions { steam_running: &NOT_RUNNING, defer_if_running: false, scheduled: false, only: None };
    execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &opts).unwrap();
    let before = read_vdf(&w.steam, "1");

    let mut draft = RebuildDraft::new(RebuildMode::Managed, "1");
    draft.set_decision(&key_of(&w, "Halo"), Decision::Remove { confirmed: true }).unwrap();
    // A decision on a shortcut outside the managed scope is ignored.
    draft.set_decision(&key_of(&w, "My Emulator"), Decision::Remove { confirmed: true }).unwrap();
    w.lib.rebuild_draft = Some(draft);
    let plan = plan_rebuild(&w.lib, &snap(&w.steam), &w.prefs).unwrap();
    assert_eq!(plan.summary["remove"], 1);
    assert_eq!(plan.summary["out_of_scope"], 6);
    execute(&mut w).unwrap();
    let after = read_vdf(&w.steam, "1");
    assert_eq!(after.len(), before.len() - 1);
    assert!(after.iter().any(|s| s.app_name == "My Emulator"));
    assert!(w.t.path("Games/Halo/haloce.exe").exists(), "game files are never deleted");
}

#[test]
fn simulation_mode_rebuild_writes_nothing() {
    let _g = serial();
    let mut w = world("simrebuild");
    w.prefs.simulation_mode = true;
    let before = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    full_rebuild_decisions(&mut w);
    let record = execute(&mut w).unwrap();
    assert!(record.simulated);
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);
    assert!(w.lib.rebuild_draft.is_some(), "draft kept for the real run");
}
