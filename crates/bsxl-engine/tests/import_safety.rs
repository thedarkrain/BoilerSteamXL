//! Release-blocking tests for the review queue and approval-gated Steam writes.
//! Everything runs against a synthetic Steam folder.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use bsxl_engine::{
    import::{execute_import, list_backups, plan_import, restore_backup, restore_preview, ExecOptions},
    library::{EntryEdit, Library, Status},
    prefs::Preferences,
};
use common::*;

fn opts() -> ExecOptions<'static> {
    ExecOptions { steam_running: &NOT_RUNNING, defer_if_running: false, scheduled: false, only: None }
}

struct World {
    t: Temp,
    steam: std::path::PathBuf,
    prefs: Preferences,
    lib: Library,
}

/// Games folder + Steam with two accounts; account 1 has one existing shortcut.
fn world(label: &str) -> World {
    let t = Temp::new(label);
    games_fixture(&t);
    let existing = t.exe("Elsewhere/Emulator/emu.exe", 2048);
    let steam = steam_fixture(
        &t,
        &[
            ("1", vec![shortcut("My Emulator", &existing.to_string_lossy(), "-fullscreen")]),
            ("2", vec![]),
        ],
    );
    let mut prefs = prefs_for(&t, &steam);
    prefs.simulation_mode = false; // the fixture Steam may be written
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    World { t, steam, prefs, lib }
}

fn import(w: &mut World) -> Result<bsxl_engine::import::Outcome, String> {
    let snap = snap(&w.steam);
    let plan = plan_import(&w.lib, &snap, &w.prefs, None);
    execute_import(&lock(), &mut w.lib, &snap, &w.prefs, &plan.token, &opts())
}

#[test]
fn scans_never_write_to_steam_and_nothing_imports_without_approval() {
    let _g = serial();
    let mut w = world("noapprove");
    let before = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    for _ in 0..3 {
        refresh(&w.prefs, &mut w.lib);
    }
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);
    assert!(w.lib.entries.values().all(|e| e.status != Status::Imported && e.status != Status::Approved));
    let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
    assert!(plan.ops.is_empty());
    assert!(import(&mut w).is_err());
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);
}

#[test]
fn repeated_scans_do_not_duplicate_queue_entries() {
    let _g = serial();
    let mut w = world("rescan");
    let count = w.lib.entries.len();
    let ids: Vec<String> = w.lib.entries.keys().cloned().collect();
    for _ in 0..3 {
        refresh(&w.prefs, &mut w.lib);
    }
    assert_eq!(w.lib.entries.len(), count);
    assert_eq!(w.lib.entries.keys().cloned().collect::<Vec<_>>(), ids);
}

#[test]
fn approved_import_preserves_existing_entries_and_is_idempotent() {
    let _g = serial();
    let mut w = world("import");
    let original = read_vdf(&w.steam, "1");
    let awo = id_of(&w.lib, "A Way Out");
    assert!(w.lib.approve(&[awo.clone()], false).is_empty());

    let snapshot = snap(&w.steam);
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    assert_eq!(plan.summary["create"], 2, "one per account");
    assert_eq!(plan.summary["remove"], 0);
    assert_eq!(plan.summary["duplicates_created"], 0);
    let outcome = execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &opts()).unwrap();
    assert!(outcome.record.backup.is_some());

    // The preview matched what happened.
    let after = read_vdf(&w.steam, "1");
    assert_eq!(after.len(), original.len() + 1);
    assert_eq!(after[..original.len()], original[..], "existing shortcuts untouched");
    let created = &after[original.len()];
    let planned = plan.ops.iter().find(|o| o.user_id == "1").unwrap().new.clone().unwrap();
    assert_eq!(created.app_name, planned.name);
    assert_eq!(created.exe, planned.exe);
    assert_eq!(created.app_id, planned.app_id);
    assert_eq!(read_vdf(&w.steam, "2").len(), 1);
    assert_eq!(w.lib.entries[&awo].status, Status::Imported);

    // Again: rescans and another import create nothing.
    for _ in 0..2 {
        refresh(&w.prefs, &mut w.lib);
        assert_eq!(w.lib.entries[&awo].status, Status::Imported);
        let plan = plan_import(&w.lib, &snap(&w.steam), &w.prefs, None);
        assert!(plan.ops.is_empty());
        assert!(import(&mut w).is_err());
    }
    assert_eq!(read_vdf(&w.steam, "1").len(), original.len() + 1);
}

#[test]
fn manual_games_need_approval_and_survive_restarts() {
    let _g = serial();
    let mut w = world("manual");
    let exe = w.t.exe("Elsewhere/Missed Game/GameLauncher.exe", 4096);
    let id = w
        .lib
        .add_manual(
            &EntryEdit {
                exe: Some(exe.to_string_lossy().to_string()),
                title: Some("Missed Game".into()),
                launch_options: Some("-windowed".into()),
                ..EntryEdit::default()
            },
            &snap(&w.steam),
        )
        .unwrap();
    assert_eq!(w.lib.entries[&id].status, Status::NeedsReview);
    assert!(plan_import(&w.lib, &snap(&w.steam), &w.prefs, None).ops.is_empty(), "not written before approval");

    // "Restart": save, load, rescan.
    w.lib.save().unwrap();
    let mut reloaded = Library::load();
    refresh(&w.prefs, &mut reloaded);
    refresh(&w.prefs, &mut reloaded);
    let entry = &reloaded.entries[&id];
    assert_eq!(entry.display_title(), "Missed Game");
    assert_eq!(entry.launch_options, "-windowed");
    assert_eq!(entry.status, Status::NeedsReview);

    // A manual game whose file disappears is marked missing, not removed.
    std::fs::remove_file(&exe).unwrap();
    refresh(&w.prefs, &mut reloaded);
    assert_eq!(reloaded.entries[&id].status, Status::Missing);
}

#[test]
fn manual_add_reports_duplicates() {
    let _g = serial();
    let mut w = world("manualdup");
    let existing = w.t.path("Elsewhere/Emulator/emu.exe");
    let id = w
        .lib
        .add_manual(&EntryEdit { exe: Some(existing.to_string_lossy().to_string()), ..EntryEdit::default() }, &snap(&w.steam))
        .unwrap();
    let entry = &w.lib.entries[&id];
    assert_eq!(entry.status, Status::AlreadyInSteam);
    assert!(entry.duplicate.as_ref().unwrap().reason.contains("My Emulator"));
    assert!(!w.lib.approve(&[id.clone()], false).is_empty(), "no silent duplicate");
}

#[test]
fn executable_overrides_survive_rescans_and_can_be_reset() {
    let _g = serial();
    let mut w = world("override");
    let haze = id_of(&w.lib, "Haze1");
    let selector = w.t.s("Games/PLUG&PLAY/Haze1/Language Selector.exe");
    w.lib.edit(&haze, &EntryEdit { exe: Some(selector.clone()), ..EntryEdit::default() }).unwrap();
    for _ in 0..2 {
        refresh(&w.prefs, &mut w.lib);
    }
    assert_eq!(w.lib.entries[&haze].exe, selector);
    assert!(w.lib.entries[&haze].user_exe);
    w.lib.reset_override(&haze).unwrap();
    assert!(w.lib.entries[&haze].exe.ends_with("Game-Win64-Shipping.exe"));
}

#[test]
fn rejected_and_ignored_stay_that_way() {
    let _g = serial();
    let mut w = world("reject");
    let halo = id_of(&w.lib, "Halo");
    let other = id_of(&w.lib, "Other Game");
    w.lib.reject(&[halo.clone()]);
    let rules = w.lib.ignore(&[other.clone()]);
    w.prefs.rules.rules.extend(rules);
    for _ in 0..2 {
        refresh(&w.prefs, &mut w.lib);
    }
    assert_eq!(w.lib.entries[&halo].status, Status::Rejected);
    // Ignored permanently: the folder is excluded from scans; its entry stays ignored.
    assert_eq!(w.lib.entries[&other].status, Status::Ignored);
    let summary = w.lib.last_scan.clone().unwrap();
    assert!(summary.orphan_excluded.iter().any(|e| e.path.contains("Other Game")));
}

#[test]
fn approval_is_invalidated_when_the_launch_command_changes() {
    let _g = serial();
    let mut w = world("invalidate");
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    assert_eq!(w.lib.entries[&halo].status, Status::Approved);
    w.lib.edit(&halo, &EntryEdit { launch_options: Some("-novid".into()), ..EntryEdit::default() }).unwrap();
    assert_eq!(w.lib.entries[&halo].status, Status::NeedsReview);
    assert!(w.lib.entries[&halo].approval.is_none());

    // Detected executable changes on disk: a better match appears.
    w.lib.approve(&[halo.clone()], false);
    w.t.exe("Games/Halo/Halo.exe", 60 * 1024);
    std::fs::remove_file(w.t.path("Games/Halo/haloce.exe")).unwrap();
    refresh(&w.prefs, &mut w.lib);
    assert_eq!(w.lib.entries[&halo].status, Status::NeedsReview);
}

#[test]
fn plan_token_detects_changes_after_the_preview() {
    let _g = serial();
    let mut w = world("token");
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    let snapshot = snap(&w.steam);
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    // Another game approved after the preview: the old confirmation is not valid.
    let awo = id_of(&w.lib, "A Way Out");
    w.lib.approve(&[awo], false);
    assert!(execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &opts()).is_err());
    // Steam changed the file after the preview.
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    write_vdf(&vdf_path(&w.steam, "1"), &[]);
    assert!(execute_import(&lock(), &mut w.lib, &snap(&w.steam), &w.prefs, &plan.token, &opts()).is_err());
}

#[test]
fn steam_running_blocks_or_defers_and_never_writes() {
    let _g = serial();
    let mut w = world("running");
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    let before = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    let snapshot = snap(&w.steam);
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    let running = ExecOptions { steam_running: &RUNNING, defer_if_running: false, scheduled: true, only: None };
    assert!(execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &running).unwrap_err().contains("Steam is running"));
    let defer = ExecOptions { steam_running: &RUNNING, defer_if_running: true, scheduled: true, only: None };
    let outcome = execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &defer).unwrap();
    assert!(outcome.deferred);
    assert_eq!(w.lib.entries[&halo].status, Status::ReadyToImport);
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);
    // Later, with Steam closed, the deferred game is written.
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &opts()).unwrap();
    assert_eq!(w.lib.entries[&halo].status, Status::Imported);
}

#[test]
fn corrupted_shortcuts_file_is_never_overwritten() {
    let _g = serial();
    let mut w = world("corrupt");
    std::fs::write(vdf_path(&w.steam, "2"), b"definitely not a shortcuts file").unwrap();
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    let snapshot = snap(&w.steam);
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    assert!(plan.warnings.iter().any(|x| x.contains("damaged")));
    assert!(plan.ops.iter().all(|o| o.user_id == "1"));
    execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &opts()).unwrap();
    assert_eq!(std::fs::read(vdf_path(&w.steam, "2")).unwrap(), b"definitely not a shortcuts file");
}

#[test]
fn only_the_chosen_steam_account_is_written() {
    let _g = serial();
    let mut w = world("accounts");
    w.prefs.target_users = vec!["2".into()];
    let before = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    import(&mut w).unwrap();
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);
    assert_eq!(read_vdf(&w.steam, "2").len(), 1);
}

#[test]
fn failed_import_restores_accounts_already_written() {
    let _g = serial();
    let mut w = world("rollback");
    let before1 = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    let snapshot = snap(&w.steam);
    let plan = plan_import(&w.lib, &snapshot, &w.prefs, None);
    // Account 2's file changes between account 1 being written and account 2.
    write_vdf(&vdf_path(&w.steam, "2"), &[shortcut("Sneaky", "C:\\x.exe", "")]);
    let err = execute_import(&lock(), &mut w.lib, &snapshot, &w.prefs, &plan.token, &opts()).unwrap_err();
    assert!(err.contains("restored"), "{err}");
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before1);
    assert_eq!(read_vdf(&w.steam, "2").len(), 1, "the newer change is not overwritten");
    assert_eq!(w.lib.entries[&halo].status, Status::ImportFailed);
}

#[test]
fn simulation_mode_writes_nothing() {
    let _g = serial();
    let mut w = world("simulate");
    w.prefs.simulation_mode = true;
    let before = std::fs::read(vdf_path(&w.steam, "1")).unwrap();
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo.clone()], false);
    let outcome = import(&mut w).unwrap();
    assert!(outcome.record.simulated);
    assert_eq!(std::fs::read(vdf_path(&w.steam, "1")).unwrap(), before);
    assert_eq!(w.lib.entries[&halo].status, Status::Approved);
}

#[test]
fn backups_are_verified_and_restore_works() {
    let _g = serial();
    let mut w = world("restore");
    let original = read_vdf(&w.steam, "1");
    let halo = id_of(&w.lib, "Halo");
    w.lib.approve(&[halo], false);
    let outcome = import(&mut w).unwrap();
    let backup_id = outcome.record.backup.clone().unwrap();
    let manifest = list_backups().into_iter().find(|b| b.id == backup_id).unwrap();
    assert!(manifest.verified);
    assert_eq!(manifest.users.len(), 2);

    let snapshot = snap(&w.steam);
    let preview = restore_preview(&backup_id, &snapshot, w.prefs.simulation_mode).unwrap();
    let user1 = preview.users.iter().find(|u| u.user_id == "1").unwrap();
    assert_eq!(user1.lost.len(), 1, "the import is listed as lost by restoring");
    restore_backup(&lock(), &mut w.lib, &backup_id, &snapshot, &w.prefs, &preview.token, &NOT_RUNNING).unwrap();
    assert_eq!(read_vdf(&w.steam, "1"), original);
}

#[test]
fn official_steam_games_are_recognized() {
    let _g = serial();
    let t = Temp::new("official");
    t.exe("SteamLibrary/steamapps/common/Portal 2/portal2.exe", 4096);
    let steam = steam_fixture(&t, &[("1", vec![])]);
    let mut prefs = prefs_for(&t, &steam);
    prefs.scan_roots[0].path = t.s("SteamLibrary/steamapps/common");
    // steamapps folders are skipped by the walker, so scan the common folder directly.
    let mut lib = Library::default();
    refresh(&prefs, &mut lib);
    let portal = entry_by_title(&lib, "Portal 2");
    assert_eq!(portal.status, Status::AlreadyInSteam);
    assert!(portal.duplicate.as_ref().unwrap().reason.contains("Installed by Steam"));
}
