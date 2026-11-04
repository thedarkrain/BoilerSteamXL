//! Operation plans, backups, imports, Clean Rebuild and restore.
//!
//! One [`Plan`] is the single source of truth for what will happen: the preview shows it,
//! every summary count is computed from its `ops`, and execution rebuilds the plan and
//! refuses to run unless its `token` matches the one the user confirmed. The token covers
//! the operations and the exact bytes of each affected `shortcuts.vdf`, so a change made
//! by Steam (or anything else) after the preview stops the operation.
//!
//! Every write: lock → verified backup → build new file → write to a temporary file →
//! parse it back → atomic rename → read back and verify → on any failure restore the
//! backup. Steam is never killed; if it is running the operation stops and says so.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use steam_shortcuts_util::{
    calculate_app_id_for_shortcut, parse_shortcuts, shortcut::ShortcutOwned, Shortcut,
};

use boilr_core::steamgriddb::ImageType;

use crate::{
    artwork, data_dir, fnv64, vdf,
    library::{ImportRecord, Library, RunRecord, Status},
    norm_path, now_rfc3339,
    prefs::Preferences,
    steam::{file_fingerprint, identity_for, to_view, SteamShortcut, SteamSnapshot, SteamUser, MANAGED_TAG},
};

pub const REBUILD_PHRASE: &str = "REBUILD MY NON-STEAM LIBRARY";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Keep,
    Create,
    Replace,
    Update,
    Remove,
    /// Left exactly as it is because no decision was made.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortcutView {
    pub app_id: u32,
    pub name: String,
    pub exe: String,
    pub start_dir: String,
    pub launch_options: String,
    #[serde(default)]
    pub icon: String,
}

impl From<&SteamShortcut> for ShortcutView {
    fn from(s: &SteamShortcut) -> Self {
        ShortcutView {
            app_id: s.app_id,
            name: s.name.clone(),
            exe: s.exe.clone(),
            start_dir: s.start_dir.clone(),
            launch_options: s.launch_options.clone(),
            icon: s.icon.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub kind: OpKind,
    pub user_id: String,
    #[serde(default)]
    pub entry_id: Option<String>,
    #[serde(default)]
    pub shortcut_key: Option<String>,
    #[serde(default)]
    pub old: Option<ShortcutView>,
    #[serde(default)]
    pub new: Option<ShortcutView>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// The user explicitly confirmed a separate shortcut despite a duplicate warning.
    #[serde(default)]
    pub allow_duplicate: bool,
    /// `DevkitGameID` for a created or replacing shortcut; empty means the entry's own.
    #[serde(default)]
    pub identity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blocked {
    pub id: String,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// "import" or "rebuild".
    pub kind: String,
    #[serde(default)]
    pub mode: Option<RebuildMode>,
    pub user_ids: Vec<String>,
    pub ops: Vec<Op>,
    pub blocked: Vec<Blocked>,
    pub summary: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
    pub simulation: bool,
    pub token: String,
}

impl Plan {
    fn finish(&mut self, snapshot: &SteamSnapshot, extra: &[(&str, usize)]) {
        let count = |k: OpKind| self.ops.iter().filter(|o| o.kind == k).count();
        let before: usize = self
            .user_ids
            .iter()
            .filter_map(|u| snapshot.user(u))
            .map(|u| u.shortcuts.len())
            .sum();
        let (create, remove) = (count(OpKind::Create), count(OpKind::Remove));
        let mut summary = BTreeMap::from([
            ("before".to_string(), before),
            ("after".to_string(), before + create - remove.min(before + create)),
            ("keep".to_string(), count(OpKind::Keep)),
            ("create".to_string(), create),
            ("replace".to_string(), count(OpKind::Replace)),
            ("update".to_string(), count(OpKind::Update)),
            ("remove".to_string(), remove),
            ("unresolved".to_string(), count(OpKind::Unresolved)),
            ("blocked".to_string(), self.blocked.len()),
            ("duplicates_created".to_string(), duplicates_created(&self.ops, snapshot)),
        ]);
        for (key, value) in extra {
            summary.insert((*key).to_string(), *value);
        }
        self.summary = summary;
        let fingerprints: Vec<String> = self
            .user_ids
            .iter()
            .filter_map(|u| snapshot.user(u))
            .map(|u| format!("{}={}", u.user_id, u.fingerprint))
            .collect();
        // Artwork downloaded between preview and confirm (icon path, notes) is not a change the
        // user has to see again: the token leaves it out, and the write uses the current cache.
        let ops: Vec<Op> = self
            .ops
            .iter()
            .cloned()
            .map(|mut o| {
                o.notes.clear();
                if let Some(new) = o.new.as_mut() {
                    new.icon.clear();
                }
                o
            })
            .collect();
        // Simulation is part of the token: a plan confirmed as a dry run can never write.
        let body = serde_json::to_string(&(&self.kind, &self.mode, self.simulation, &ops, &self.blocked, fingerprints))
            .unwrap_or_default();
        self.token = format!("{:016x}", fnv64(&body));
    }
}

/// A new shortcut for `entry` in `user`'s account: the cached icon (if any) becomes the
/// shortcut's icon, and the preview says which artwork will be added.
fn with_artwork(mut view: ShortcutView, user_folder: &str, entry_id: &str, notes: &mut Vec<String>) -> ShortcutView {
    if let Some(icon) = artwork::grid_icon_path(user_folder, entry_id, view.app_id) {
        view.icon = icon;
    }
    let kinds = artwork::cached_kinds(entry_id);
    if !kinds.is_empty() {
        notes.push(format!("Artwork added to Steam: {}.", kinds.join(", ").replace("wide_grid", "wide capsule").replace("grid", "cover")));
    }
    view
}

/// Shortcuts the plan would leave sharing an executable and launch options in one account,
/// that were not already duplicated before. Every plan must report 0.
fn duplicates_created(ops: &[Op], snapshot: &SteamSnapshot) -> usize {
    let mut by_user: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for user in snapshot.users.iter() {
        let removed: BTreeSet<&str> = ops
            .iter()
            .filter(|o| o.user_id == user.user_id && matches!(o.kind, OpKind::Remove | OpKind::Replace | OpKind::Update))
            .filter_map(|o| o.shortcut_key.as_deref())
            .collect();
        let list = by_user.entry(user.user_id.as_str()).or_default();
        for s in &user.shortcuts {
            if !removed.contains(s.key().as_str()) {
                list.push(format!("{}|{}", norm_path(&s.exe), s.launch_options.trim()));
            }
        }
    }
    let mut created = 0;
    for op in ops {
        if let (Some(new), true) = (&op.new, matches!(op.kind, OpKind::Create | OpKind::Replace | OpKind::Update)) {
            let key = format!("{}|{}", norm_path(&new.exe), new.launch_options.trim());
            let list = by_user.entry(op.user_id.as_str()).or_default();
            if list.contains(&key) && !op.allow_duplicate {
                created += 1;
            }
            list.push(key);
        }
    }
    created
}

fn quoted(path: &str) -> String {
    format!("\"{}\"", path.trim().trim_matches('"'))
}

fn new_view(name: &str, exe: &str, start_dir: &str, launch_options: &str, icon: &str) -> ShortcutView {
    let exe = quoted(exe);
    let start_dir = quoted(start_dir);
    let app_id = calculate_app_id_for_shortcut(&Shortcut::new("0", name, &exe, &start_dir, icon, "", launch_options));
    ShortcutView {
        app_id,
        name: name.to_string(),
        exe,
        start_dir,
        launch_options: launch_options.to_string(),
        icon: icon.to_string(),
    }
}

fn entry_view(entry: &crate::library::Entry) -> ShortcutView {
    new_view(entry.display_title(), &entry.exe, &entry.start_dir, &entry.launch_options, &entry.icon)
}

fn writable_users<'a>(snapshot: &'a SteamSnapshot, prefs: &Preferences, warnings: &mut Vec<String>) -> Vec<&'a SteamUser> {
    snapshot
        .target_users(&prefs.target_users)
        .into_iter()
        .filter(|u| match &u.error {
            Some(e) => {
                warnings.push(format!("Steam account {} has a damaged shortcuts.vdf and will not be modified: {e}", u.user_id));
                false
            }
            None => true,
        })
        .collect()
}

// ---- import -------------------------------------------------------------------------

/// What "Import approved games" would do. `only` limits it to some entries.
pub fn plan_import(lib: &Library, snapshot: &SteamSnapshot, prefs: &Preferences, only: Option<&[String]>) -> Plan {
    let mut plan = Plan {
        kind: "import".into(),
        simulation: prefs.simulation_mode,
        ..Plan::default()
    };
    if let Some(error) = &snapshot.error {
        plan.warnings.push(error.clone());
    }
    let users = writable_users(snapshot, prefs, &mut plan.warnings);
    plan.user_ids = users.iter().map(|u| u.user_id.clone()).collect();
    if users.is_empty() {
        plan.warnings.push("No Steam account can be written to.".into());
    }

    // Shortcuts the user asked to remove. Never part of an unattended `--sync` (`only`).
    let mut removed: BTreeSet<String> = BTreeSet::new();
    for removal in lib.removals.iter().filter(|_| only.is_none()) {
        let Some(user) = users.iter().find(|u| u.user_id == removal.user_id) else {
            plan.warnings.push(format!("\"{}\" is in a Steam account that is not selected or cannot be written; it was not removed.", removal.name));
            continue;
        };
        let same: Vec<&SteamShortcut> = user.shortcuts.iter().filter(|s| removal.matches(s)).collect();
        let found = same.iter().find(|s| s.index == removal.index).or(if same.len() == 1 { same.first() } else { None });
        let Some(s) = found else {
            plan.warnings.push(if same.is_empty() {
                format!("\"{}\" is no longer in Steam; there is nothing to remove.", removal.name)
            } else {
                format!("Steam has several identical \"{}\" shortcuts; remove the extra ones with Clean Rebuild.", removal.name)
            });
            continue;
        };
        removed.insert(s.key());
        plan.ops.push(Op {
            kind: OpKind::Remove,
            user_id: user.user_id.clone(),
            entry_id: lib.entry_for_shortcut(s).map(|e| e.id.clone()),
            shortcut_key: Some(s.key()),
            old: Some((*s).into()),
            new: None,
            notes: vec!["Only the Steam shortcut is removed; the game's files are not touched. It stays in the backup.".into()],
            allow_duplicate: false,
            identity: s.identity.clone(),
        });
    }

    // Games already in Steam that the user renamed: only the name in Steam changes.
    for entry in lib.entries.values().filter(|_| only.is_none()) {
        if matches!(entry.status, Status::Approved | Status::ReadyToImport) {
            continue; // the approved update below carries the name
        }
        let Some(title) = Library::pending_rename(entry) else { continue };
        for user in &users {
            for s in user.shortcuts.iter().filter(|s| !removed.contains(&s.key()) && s.name != title) {
                if lib.entry_for_shortcut(s).map(|e| e.id.as_str()) != Some(entry.id.as_str()) {
                    continue;
                }
                let old: ShortcutView = s.into();
                let new = ShortcutView { name: title.to_string(), ..old.clone() };
                plan.ops.push(Op {
                    kind: OpKind::Update,
                    user_id: user.user_id.clone(),
                    entry_id: Some(entry.id.clone()),
                    shortcut_key: Some(s.key()),
                    old: Some(old),
                    new: Some(new),
                    notes: vec!["Only the name in Steam changes; the Steam ID, artwork, play time and settings stay.".into()],
                    allow_duplicate: false,
                    identity: s.identity.clone(),
                });
            }
        }
    }

    let mut planned_exes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in lib.entries.values() {
        if !matches!(entry.status, Status::Approved | Status::ReadyToImport) {
            continue;
        }
        if only.is_some_and(|ids| !ids.contains(&entry.id)) {
            continue;
        }
        let title = entry.display_title().to_string();
        let block = |reason: &str| Blocked {
            id: entry.id.clone(),
            title: title.clone(),
            reason: reason.to_string(),
        };
        if !entry.approval_valid() {
            plan.blocked.push(block("The executable or launch options changed after approval. Review it again."));
            continue;
        }
        if !Path::new(&entry.exe).is_file() {
            plan.blocked.push(block("The executable no longer exists."));
            continue;
        }
        let allow_duplicate = entry.approval.as_ref().is_some_and(|a| a.allow_duplicate);
        // A moved game keeps the identity its shortcut was written with.
        let identity = entry.import.as_ref().map_or_else(|| identity_for(&entry.id), |r| r.identity.clone());
        let view = entry_view(entry);
        for user in &users {
            let existing = user.shortcuts.iter().find(|s| s.identity == identity);
            if existing.is_some_and(|s| removed.contains(&s.key())) {
                plan.blocked.push(block("Its Steam shortcut is being removed in this sync."));
                continue;
            }
            match existing {
                Some(old) => {
                    let mut new = ShortcutView::from(old);
                    new.exe = view.exe.clone();
                    new.start_dir = view.start_dir.clone();
                    new.launch_options = view.launch_options.clone();
                    if entry.import.as_ref().is_some_and(|r| r.name != entry.display_title()) {
                        new.name = view.name.clone();
                    }
                    let unchanged = norm_path(&old.exe) == norm_path(&new.exe)
                        && norm_path(&old.start_dir) == norm_path(&new.start_dir)
                        && old.launch_options.trim() == new.launch_options.trim()
                        && old.name == new.name;
                    if !unchanged {
                        let exe_key = norm_path(&entry.exe);
                        // Executable plus arguments: launcher games share one launcher exe.
                        let planned_key = format!("{exe_key}|{}", entry.launch_options.trim());
                        let planned = planned_exes.entry(user.user_id.clone()).or_default();
                        if planned.contains(&planned_key) {
                            plan.blocked.push(block("Another approved game uses the same executable."));
                            continue;
                        }
                        planned.insert(planned_key);
                        let clash = user.shortcuts.iter().find(|s| {
                            s.key() != old.key() && norm_path(&s.exe_path()) == exe_key && s.launch_options.trim() == new.launch_options.trim()
                        });
                        if let (Some(s), false) = (clash, allow_duplicate) {
                            plan.blocked.push(block(&format!("Steam account {} already has \"{}\" for the new executable.", user.user_id, s.name)));
                            continue;
                        }
                        plan.ops.push(Op {
                            kind: OpKind::Update,
                            user_id: user.user_id.clone(),
                            entry_id: Some(entry.id.clone()),
                            shortcut_key: Some(old.key()),
                            old: Some(old.into()),
                            new: Some(new),
                            notes: vec!["Keeps the Steam ID, artwork, play time and Steam settings.".into()],
                            allow_duplicate,
                            identity: identity.clone(),
                        });
                    }
                }
                None => {
                    let exe_key = norm_path(&entry.exe);
                    let args = entry.launch_options.trim();
                    let planned_key = format!("{exe_key}|{args}");
                    // Same rule as detect_duplicate: only two different, non-empty argument lists differ.
                    let same_args = |s: &SteamShortcut| s.launch_options.trim() == args || s.launch_options.trim().is_empty() || args.is_empty();
                    let clash = user
                        .shortcuts
                        .iter()
                        .find(|s| norm_path(&s.exe_path()) == exe_key && same_args(s) && !removed.contains(&s.key()));
                    let planned = planned_exes.entry(user.user_id.clone()).or_default();
                    if planned.contains(&planned_key) {
                        plan.blocked.push(block("Another approved game uses the same executable."));
                        continue;
                    }
                    if let (Some(s), false) = (clash, allow_duplicate) {
                        plan.blocked.push(block(&format!("Steam account {} already has \"{}\" for this executable.", user.user_id, s.name)));
                        continue;
                    }
                    let official = snapshot.official.iter().find(|g| exe_key.starts_with(&format!("{}/", norm_path(&g.install_dir))));
                    if let (Some(g), false) = (official, allow_duplicate) {
                        plan.blocked.push(block(&format!("Installed by Steam as \"{}\".", g.name)));
                        continue;
                    }
                    planned.insert(planned_key);
                    let mut notes = Vec::new();
                    let new = with_artwork(view.clone(), &user.folder, &entry.id, &mut notes);
                    plan.ops.push(Op {
                        kind: OpKind::Create,
                        user_id: user.user_id.clone(),
                        entry_id: Some(entry.id.clone()),
                        shortcut_key: None,
                        old: None,
                        new: Some(new),
                        notes,
                        allow_duplicate,
                        identity: identity.clone(),
                    });
                }
            }
        }
    }
    let pending = lib
        .entries
        .values()
        .filter(|e| matches!(e.status, Status::NeedsReview | Status::PossibleDuplicate | Status::UpdateAvailable))
        .count();
    let excluded = lib.last_scan.as_ref().map_or(0, |s| s.exes_excluded);
    plan.finish(snapshot, &[("still_review", pending), ("excluded", excluded)]);
    plan
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub record: RunRecord,
    /// Steam was running; the confirmed games wait as "Ready to import".
    pub deferred: bool,
}

pub struct ExecOptions<'a> {
    pub steam_running: &'a dyn Fn() -> bool,
    /// Steam was running and the user asked to import when it is closed.
    pub defer_if_running: bool,
    pub scheduled: bool,
    /// Limit to these entries (the headless `--sync` only writes confirmed, deferred ones).
    pub only: Option<Vec<String>>,
}

/// Holds the operation lock (an OS file lock, so it also works across processes: the app
/// and the scheduled task). Every library load-change-save and every Steam write runs
/// while one is held; the write functions take it as a parameter so they cannot run
/// without it.
pub struct OpLock(#[allow(dead_code)] File);

fn lock_file() -> Result<File, String> {
    let path = data_dir().join("operation.lock");
    let _ = std::fs::create_dir_all(data_dir());
    File::create(&path).map_err(|e| format!("Could not create {}: {e}", path.display()))
}

const BUSY: &str = "Another BoilerSteamXL scan or import is running. Try again when it has finished.";

/// Takes the lock or fails at once.
pub fn acquire_lock() -> Result<OpLock, String> {
    let file = lock_file()?;
    match file.try_lock() {
        Ok(()) => Ok(OpLock(file)),
        Err(_) => Err(BUSY.into()),
    }
}

/// Takes the lock, waiting up to `timeout` for another operation to finish.
pub fn wait_for_lock(timeout: std::time::Duration) -> Result<OpLock, String> {
    let file = lock_file()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if file.try_lock().is_ok() {
            return Ok(OpLock(file));
        }
        if std::time::Instant::now() >= deadline {
            return Err(BUSY.into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Runs a confirmed import. Only approved entries in the plan the user confirmed (same
/// token) are written; anything else is refused.
pub fn execute_import(
    _lock: &OpLock,
    lib: &mut Library,
    snapshot: &SteamSnapshot,
    prefs: &Preferences,
    token: &str,
    options: &ExecOptions,
) -> Result<Outcome, String> {
    let plan = plan_import(lib, snapshot, prefs, options.only.as_deref());
    if plan.token != token {
        return Err("The library or Steam changed since the preview. Review the import again.".into());
    }
    if plan.ops.is_empty() {
        return Err("There is nothing to import.".into());
    }
    if plan.summary.get("duplicates_created").copied().unwrap_or(0) > 0 {
        return Err("The plan would create a duplicate shortcut; review it again.".into());
    }
    let mut record = RunRecord {
        id: format!("import-{}", now_rfc3339()),
        kind: "import".into(),
        at: now_rfc3339(),
        simulated: plan.simulation,
        scheduled: options.scheduled,
        user_ids: plan.user_ids.clone(),
        counts: plan.summary.clone(),
        ..RunRecord::default()
    };
    record.details = plan
        .ops
        .iter()
        .map(|o| format!("{:?} {} → {}", o.kind, o.new.as_ref().or(o.old.as_ref()).map_or("", |n| n.name.as_str()), o.user_id))
        .collect();

    if plan.simulation {
        record.details.insert(0, "Simulation mode: nothing was written to Steam.".into());
        lib.record(record.clone());
        return Ok(Outcome { record, deferred: false });
    }
    if (options.steam_running)() {
        if options.defer_if_running {
            // Removals wait for the next sync in the app; the unattended one never removes.
            for op in plan.ops.iter().filter(|o| o.new.is_some()) {
                if let Some(entry) = op.entry_id.as_ref().and_then(|id| lib.entries.get_mut(id)) {
                    entry.status = Status::ReadyToImport;
                }
            }
            record.details.insert(0, "Steam was running; the games will be imported once Steam is closed.".into());
            lib.record(record.clone());
            return Ok(Outcome { record, deferred: true });
        }
        return Err("Steam is running. Close Steam (Steam menu → Exit) and try again; BoilerSteamXL never force-closes it.".into());
    }

    let users: Vec<&SteamUser> = plan.user_ids.iter().filter_map(|u| snapshot.user(u)).collect();
    let backup = create_backup("import", &users, false, Some(&plan)).map_err(|e| format!("Backup failed, nothing was changed: {e}"))?;
    record.backup = Some(backup.id.clone());

    let result = apply_ops(&plan, snapshot, &backup);
    match result {
        Ok(warnings) => {
            record.details.extend(warnings);
            let now = now_rfc3339();
            for op in &plan.ops {
                let (Some(id), Some(new)) = (&op.entry_id, &op.new) else { continue };
                let Some(entry) = lib.entries.get_mut(id) else { continue };
                if Library::pending_rename(entry) == Some(new.name.as_str()) {
                    entry.synced_title = Some(new.name.clone());
                }
                // A rename of a linked shortcut (made by BoilR, by hand...) stays linked, not imported.
                if op.kind == OpKind::Update && entry.import.is_none() {
                    if let Some(linked) = &mut entry.linked {
                        linked.name = new.name.clone();
                    }
                    continue;
                }
                let record = entry.import.get_or_insert_with(|| ImportRecord {
                    app_id: new.app_id,
                    identity: identity_for(id),
                    user_ids: Vec::new(),
                    at: now.clone(),
                    name: new.name.clone(),
                    exe: entry.exe.clone(),
                    launch_options: entry.launch_options.clone(),
                });
                record.app_id = new.app_id;
                record.exe = entry.exe.clone();
                record.launch_options = entry.launch_options.clone();
                record.name = new.name.clone();
                record.at = now.clone();
                if !record.user_ids.contains(&op.user_id) {
                    record.user_ids.push(op.user_id.clone());
                }
                entry.status = Status::Imported;
                entry.approval = None;
                entry.note = None;
            }
            for op in plan.ops.iter().filter(|o| o.kind == OpKind::Remove) {
                let Some(old) = &op.old else { continue };
                lib.removals.retain(|r| !(r.user_id == op.user_id && r.app_id == old.app_id && r.name == old.name && norm_path(&r.exe) == norm_path(&old.exe)));
                let Some(entry) = op.entry_id.as_ref().and_then(|id| lib.entries.get_mut(id)) else { continue };
                if let Some(record) = &mut entry.import {
                    record.user_ids.retain(|u| u != &op.user_id);
                }
                if entry.import.as_ref().is_some_and(|r| r.user_ids.is_empty()) {
                    entry.import = None;
                }
                // Merging keeps the link to the copy that stays; only a link to this one goes.
                if entry.linked.as_ref().is_some_and(|l| l.user_id.as_deref() == Some(op.user_id.as_str()) && l.app_id == Some(old.app_id)) {
                    entry.linked = None;
                    entry.user_linked = false;
                }
                // Removed on purpose: not suggested for Steam again until the user adds it back.
                if entry.import.is_none() && entry.linked.is_none() {
                    entry.status = Status::Rejected;
                    entry.approval = None;
                }
            }
            crate::log::info("import", &format!("{} change(s) written, backup {}", plan.ops.len(), backup.id));
            lib.record(record.clone());
            Ok(Outcome { record, deferred: false })
        }
        Err(e) => {
            for op in plan.ops.iter().filter(|o| o.new.is_some()) {
                if let Some(entry) = op.entry_id.as_ref().and_then(|id| lib.entries.get_mut(id)) {
                    entry.status = Status::ImportFailed;
                    entry.note = Some(e.clone());
                }
            }
            record.error = Some(e.clone());
            crate::log::error("import", &e);
            lib.record(record);
            Err(e)
        }
    }
}

fn shortcuts_path(user: &SteamUser) -> PathBuf {
    Path::new(&user.folder).join("config").join("shortcuts.vdf")
}

/// Reads an account's shortcuts both losslessly (raw records, for writing back) and as
/// parsed shortcuts (for matching). A file either reader rejects is damaged and is never
/// written.
fn read_records(path: &Path) -> Result<(Vec<vdf::Record>, Vec<ShortcutOwned>), String> {
    if !path.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let damaged = |e: String| format!("{} is damaged and was not modified: {e}", path.display());
    let raw = vdf::parse(&bytes).map_err(damaged)?;
    let parsed: Vec<ShortcutOwned> = parse_shortcuts(&bytes)
        .map_err(|e| damaged(e.to_string()))?
        .iter()
        .map(|s| s.to_owned())
        .collect();
    if raw.len() != parsed.len() {
        return Err(damaged("the two readers disagree on the number of shortcuts".into()));
    }
    Ok((raw, parsed))
}

fn op_identity(op: &Op) -> Option<String> {
    if op.identity.is_empty() {
        op.entry_id.as_deref().map(identity_for)
    } else {
        Some(op.identity.clone())
    }
}

fn build_shortcut(view: &ShortcutView, identity: Option<String>, tags: &[String]) -> ShortcutOwned {
    let mut owned = Shortcut::new("0", &view.name, &view.exe, &view.start_dir, &view.icon, "", &view.launch_options).to_owned();
    owned.app_id = view.app_id;
    if let Some(identity) = identity {
        owned.dev_kit_game_id = identity;
    }
    owned.tags = std::iter::once(MANAGED_TAG.to_string()).chain(tags.iter().cloned()).collect();
    owned
}

/// A failed write; `replaced` says whether shortcuts.vdf had already been swapped, in which
/// case the caller must put the backup back.
struct WriteError {
    message: String,
    replaced: bool,
}

/// Validates and writes a complete shortcuts.vdf: temporary file, atomic rename, read back.
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    let fail = |message: String, replaced: bool| WriteError { message, replaced };
    parse_shortcuts(bytes).map_err(|e| fail(format!("The new shortcut data did not validate: {e}"), false))?;
    vdf::parse(bytes).map_err(|e| fail(format!("The new shortcut data did not validate: {e}"), false))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| fail(e.to_string(), false))?;
    }
    let tmp = path.with_extension("vdf.bsxl-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| fail(format!("Could not write {}: {e}", tmp.display()), false))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        fail(format!("Could not replace {}: {e}. Is Steam running?", path.display()), false)
    })?;
    match std::fs::read(path) {
        Ok(back) if back == bytes => Ok(()),
        Ok(_) => Err(fail(format!("{} did not read back as written.", path.display()), true)),
        Err(e) => Err(fail(format!("Could not read back {}: {e}", path.display()), true)),
    }
}

/// Applies a plan account by account; on any failure, every account already written
/// (including one that failed after its file was replaced) is restored from `backup`.
/// Returns warnings for artwork that could not be copied (the shortcuts are written).
fn apply_ops(plan: &Plan, snapshot: &SteamSnapshot, backup: &BackupManifest) -> Result<Vec<String>, String> {
    let mut written: Vec<&SteamUser> = Vec::new();
    let mut failure = None;
    let mut warnings = Vec::new();
    for user_id in &plan.user_ids {
        let Some(user) = snapshot.user(user_id) else { continue };
        let ops: Vec<&Op> = plan.ops.iter().filter(|o| &o.user_id == user_id).collect();
        if ops.iter().all(|o| matches!(o.kind, OpKind::Keep | OpKind::Unresolved)) {
            continue;
        }
        let path = shortcuts_path(user);
        if file_fingerprint(&path) != user.fingerprint {
            failure = Some(format!("Steam changed {} after the preview; nothing more was written.", path.display()));
            break;
        }
        let result = read_records(&path)
            .map_err(|message| WriteError { message, replaced: false })
            .and_then(|(raw, parsed)| {
                let next = next_records(&raw, &parsed, user_id, &ops);
                write_bytes(&path, &vdf::serialize(&next))?;
                if plan.kind == "rebuild" {
                    migrate_artwork(user, &ops);
                }
                // Cached artwork fills whatever Steam (or the old shortcut) did not have.
                for op in ops.iter().filter(|o| matches!(o.kind, OpKind::Create | OpKind::Replace)) {
                    if let (Some(id), Some(new)) = (&op.entry_id, &op.new) {
                        artwork::copy_to_grid(id, &user.folder, new.app_id);
                        if !artwork::missing_in_grid(id, &user.folder, new.app_id).is_empty() {
                            warnings.push(format!("Artwork for \"{}\" could not be copied into Steam.", new.name));
                        }
                    }
                }
                Ok(())
            });
        match result {
            Ok(()) => written.push(user),
            Err(e) => {
                if e.replaced {
                    written.push(user);
                }
                failure = Some(e.message);
                break;
            }
        }
    }
    if let Some(error) = failure {
        let status = if written.is_empty() {
            "Nothing was changed."
        } else if written.iter().all(|user| restore_user_file(backup, user).is_ok()) {
            "Steam's files were restored from the backup."
        } else {
            "Restore the backup from Backups & History."
        };
        return Err(format!("{error} {status}"));
    }
    for user in &snapshot.users {
        if plan.user_ids.contains(&user.user_id) {
            record_after_fingerprint(backup, user);
        }
    }
    Ok(warnings)
}

/// The account's records after the ops: kept records stay byte-for-byte as they were
/// (including keys BoilerSteamXL does not know), updates change only name, target, start
/// folder and launch options, replacements take the old record's place, new games go at
/// the end.
fn next_records(raw: &[vdf::Record], current: &[ShortcutOwned], user_id: &str, ops: &[&Op]) -> Vec<vdf::Record> {
    let by_key: BTreeMap<String, &&Op> = ops.iter().filter_map(|o| o.shortcut_key.clone().map(|k| (k, o))).collect();
    let mut next = Vec::new();
    for (index, (record, old)) in raw.iter().zip(current.iter()).enumerate() {
        let mut view = to_view(user_id, index, old);
        if let Some(id) = record.app_id() {
            view.app_id = id;
        }
        let key = view.key();
        match by_key.get(&key).map(|o| (o.kind, o)) {
            Some((OpKind::Remove, _)) => {}
            Some((OpKind::Replace, op)) => {
                if let Some(new) = &op.new {
                    let mut s = build_shortcut(new, op_identity(op), &[]);
                    if s.icon.is_empty() {
                        s.icon = migrated_icon(&old.icon, old.app_id, new.app_id);
                    }
                    let mut replacement = vdf::Record::from_shortcut(&s);
                    // Steam's own per-game state carries over to the replacement.
                    for key in ["IsHidden", "AllowOverlay", "AllowDesktopConfig", "OpenVR", "LastPlayTime"] {
                        replacement.copy_field(record, key);
                    }
                    next.push(replacement);
                }
            }
            Some((OpKind::Update, op)) => {
                let mut r = record.clone();
                if let Some(new) = &op.new {
                    // Only the fields that change are written, so Steam's own spelling of the rest stays.
                    let old = op.old.as_ref();
                    if old.is_none_or(|o| o.name != new.name) {
                        r.set_str("AppName", &new.name);
                        // Without a stored appid Steam derives the ID from the name; keep the old ID
                        // so the artwork and play time stay with the shortcut.
                        if r.app_id().is_none() {
                            r.set_u32("appid", view.app_id);
                        }
                    }
                    if old.is_none_or(|o| o.exe != new.exe) {
                        r.set_str("Exe", &new.exe);
                    }
                    if old.is_none_or(|o| o.start_dir != new.start_dir) {
                        r.set_str("StartDir", &new.start_dir);
                    }
                    if old.is_none_or(|o| o.launch_options != new.launch_options) {
                        r.set_str("LaunchOptions", &new.launch_options);
                    }
                }
                next.push(r);
            }
            _ => next.push(record.clone()),
        }
    }
    for op in ops.iter().filter(|o| o.kind == OpKind::Create) {
        if let Some(new) = &op.new {
            next.push(vdf::Record::from_shortcut(&build_shortcut(new, op_identity(op), &[])));
        }
    }
    next
}

/// Points shortcuts whose icon is missing at the icon image in Steam's grid folder
/// (Steam takes the small icon from shortcuts.vdf). Same safeguards as an import: no
/// Simulation mode, Steam closed, a verified backup, atomic writes, only the chosen
/// accounts. With `only` (account, app id), just that shortcut, whose icon the user just
/// chose, is pointed at it even if its current icon exists. Returns how many shortcuts changed.
pub fn point_icons_at_grid(
    _lock: &OpLock,
    snapshot: &SteamSnapshot,
    prefs: &Preferences,
    big_picture: bool,
    only: Option<(&str, u32)>,
    steam_running: &dyn Fn() -> bool,
) -> Result<usize, String> {
    if prefs.simulation_mode {
        return Err("Simulation mode is on: shortcut icons are not written to Steam.".into());
    }
    if steam_running() {
        return Err("Steam is running. Close Steam to update shortcut icons; BoilerSteamXL never force-closes it.".into());
    }
    let mut changes: Vec<(&SteamUser, Vec<vdf::Record>, usize)> = Vec::new();
    for user in snapshot.target_users(&prefs.target_users) {
        if user.error.is_some() || user.shortcuts_file.is_none() {
            continue;
        }
        let path = shortcuts_path(user);
        if file_fingerprint(&path) != user.fingerprint {
            return Err(format!("Steam changed {} meanwhile; try again.", path.display()));
        }
        let (mut raw, parsed) = read_records(&path)?;
        let grid = Path::new(&user.folder).join("config").join("grid");
        let mut changed = 0;
        for (record, s) in raw.iter_mut().zip(parsed.iter()) {
            let app_id = record.app_id().unwrap_or(s.app_id);
            match only {
                Some((user_id, id)) if user_id != user.user_id || id != app_id => continue,
                None if !s.icon.is_empty() && Path::new(&s.icon).exists() => continue,
                _ => {}
            }
            let kind = if big_picture { ImageType::BigPicture } else { ImageType::Icon };
            let stem = kind.file_name_no_extension(app_id);
            let found = ["ico", "png", "jpg", "webp"].iter().map(|ext| grid.join(format!("{stem}.{ext}"))).find(|p| p.is_file());
            if let Some(icon) = found.filter(|p| p.to_string_lossy() != s.icon) {
                record.set_str("icon", &icon.to_string_lossy());
                changed += 1;
            }
        }
        if changed > 0 {
            changes.push((user, raw, changed));
        }
    }
    if changes.is_empty() {
        return Ok(0);
    }
    let users: Vec<&SteamUser> = changes.iter().map(|c| c.0).collect();
    let backup = create_backup("icons", &users, false, None).map_err(|e| format!("Backup failed, nothing was changed: {e}"))?;
    let mut done: Vec<&SteamUser> = Vec::new();
    let mut total = 0;
    for (user, raw, changed) in &changes {
        if let Err(e) = write_bytes(&shortcuts_path(user), &vdf::serialize(raw)) {
            if e.replaced {
                done.push(user);
            }
            let status = if done.is_empty() {
                "Nothing was changed."
            } else if done.iter().all(|u| restore_user_file(&backup, u).is_ok()) {
                "Steam's files were restored from the backup."
            } else {
                "Restore the backup from Backups & History."
            };
            return Err(format!("{} {status}", e.message));
        }
        done.push(user);
        total += changed;
    }
    for user in &done {
        record_after_fingerprint(&backup, user);
    }
    crate::log::info("icons", &format!("{total} shortcut icon(s) updated, backup {}", backup.id));
    Ok(total)
}

// ---- artwork ------------------------------------------------------------------------

/// Steam's grid file names for a shortcut: cover, wide capsule, hero, logo, icon.
fn grid_files_for(grid: &Path, app_id: u32) -> Vec<PathBuf> {
    let stems: Vec<String> = [ImageType::Grid, ImageType::WideGrid, ImageType::Hero, ImageType::Logo, ImageType::Icon, ImageType::BigPicture]
        .iter()
        .map(|kind| kind.file_name_no_extension(app_id))
        .collect();
    let Ok(entries) = std::fs::read_dir(grid) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()).is_some_and(|stem| stems.contains(&stem)))
        .collect()
}

fn migrated_icon(old_icon: &str, old_id: u32, new_id: u32) -> String {
    let name = Path::new(old_icon).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if old_id != new_id && name.starts_with(&old_id.to_string()) {
        old_icon.replacen(&old_id.to_string(), &new_id.to_string(), 1)
    } else {
        old_icon.to_string()
    }
}

/// Copies artwork from replaced shortcuts to their new Steam ID. Originals are kept.
fn migrate_artwork(user: &SteamUser, ops: &[&Op]) -> Vec<String> {
    let grid = Path::new(&user.folder).join("config").join("grid");
    let mut copied = Vec::new();
    for op in ops.iter().filter(|o| o.kind == OpKind::Replace) {
        let (Some(old), Some(new)) = (&op.old, &op.new) else { continue };
        if old.app_id == new.app_id {
            continue;
        }
        for file in grid_files_for(&grid, old.app_id) {
            let Some(name) = file.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
            let target = grid.join(name.replacen(&old.app_id.to_string(), &new.app_id.to_string(), 1));
            if !target.exists() && std::fs::copy(&file, &target).is_ok() {
                copied.push(target.to_string_lossy().to_string());
            }
        }
    }
    copied
}

// ---- backups ------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupUser {
    pub user_id: String,
    pub folder: String,
    pub had_file: bool,
    pub file_hash: String,
    pub shortcuts: Vec<ShortcutView>,
    #[serde(default)]
    pub grid_files: Vec<String>,
    #[serde(default)]
    pub collections: bool,
    /// `shortcuts.vdf` right after the operation; differs later if Steam changed it.
    #[serde(default)]
    pub after_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupManifest {
    pub id: String,
    pub created: String,
    pub kind: String,
    pub app_version: String,
    pub users: Vec<BackupUser>,
    #[serde(default)]
    pub plan: Option<Plan>,
    pub verified: bool,
}

pub fn backups_dir() -> PathBuf {
    data_dir().join("Backups")
}

fn stamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(time::macros::format_description!("[year]-[month]-[day]_[hour][minute][second]"))
        .unwrap_or_default()
}

fn copy_verified(from: &Path, to: &Path) -> Result<String, String> {
    let bytes = std::fs::read(from).map_err(|e| format!("Could not read {}: {e}", from.display()))?;
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    std::fs::write(to, &bytes).map_err(|e| format!("Could not write {}: {e}", to.display()))?;
    let back = std::fs::read(to).map_err(|e| format!("Could not read back {}: {e}", to.display()))?;
    if back != bytes {
        return Err(format!("{} did not verify", to.display()));
    }
    Ok(format!("{}:{:016x}", bytes.len(), fnv64(&String::from_utf8_lossy(&bytes))))
}

/// Backs up each account's `shortcuts.vdf` (and for rebuilds its non-Steam artwork and
/// collections), plus BoilerSteamXL's library and the plan, then verifies every copy.
/// An error here means the operation must not start.
pub fn create_backup(kind: &str, users: &[&SteamUser], with_artwork: bool, plan: Option<&Plan>) -> Result<BackupManifest, String> {
    let base = backups_dir();
    std::fs::create_dir_all(&base).map_err(|e| format!("Could not create {}: {e}", base.display()))?;
    // create_dir (not _all) claims the folder atomically, so two backups never share one.
    let stamp = stamp();
    let mut id = format!("{stamp}-{kind}");
    let mut n = 2;
    loop {
        match std::fs::create_dir(base.join(&id)) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => {
                id = format!("{stamp}-{kind}-{n}");
                n += 1;
            }
            Err(e) => return Err(format!("Could not create a backup folder: {e}")),
        }
    }
    let dir = base.join(&id);

    let mut manifest = BackupManifest {
        id: id.clone(),
        created: now_rfc3339(),
        kind: kind.to_string(),
        app_version: crate::VERSION.to_string(),
        users: Vec::new(),
        plan: plan.cloned(),
        verified: false,
    };
    for user in users {
        let user_dir = dir.join(&user.user_id);
        let source = shortcuts_path(user);
        let mut entry = BackupUser {
            user_id: user.user_id.clone(),
            folder: user.folder.clone(),
            had_file: source.exists(),
            file_hash: String::new(),
            shortcuts: user.shortcuts.iter().map(ShortcutView::from).collect(),
            grid_files: Vec::new(),
            collections: false,
            after_fingerprint: None,
        };
        if entry.had_file {
            let target = user_dir.join("shortcuts.vdf");
            entry.file_hash = copy_verified(&source, &target)?;
            // A file Steam already damaged is kept as it is (raw bytes, verified above);
            // a healthy one must read back from the backup.
            if user.error.is_none() {
                read_records(&target).map_err(|e| format!("The backup copy is not readable: {e}"))?;
            }
        }
        if with_artwork {
            let grid = Path::new(&user.folder).join("config").join("grid");
            for s in &user.shortcuts {
                for file in grid_files_for(&grid, s.app_id) {
                    if let Some(name) = file.file_name() {
                        copy_verified(&file, &user_dir.join("grid").join(name))?;
                        entry.grid_files.push(name.to_string_lossy().to_string());
                    }
                }
            }
            let collections = Path::new(&user.folder).join("config").join("cloudstorage").join("cloud-storage-namespace-1.json");
            if collections.is_file() {
                copy_verified(&collections, &user_dir.join("cloud-storage-namespace-1.json"))?;
                entry.collections = true;
            }
        }
        manifest.users.push(entry);
    }
    let library = crate::library::library_file();
    if library.is_file() {
        copy_verified(&library, &dir.join("library.json"))?;
    }
    manifest.verified = true;
    write_manifest(&dir, &manifest)?;
    // Read the manifest back: a backup we cannot read is no backup.
    let check: BackupManifest = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Backup manifest did not verify: {e}"))?;
    if check.users.len() != users.len() {
        return Err("Backup manifest did not verify.".into());
    }
    crate::log::info("backup", &format!("created {id}"));
    Ok(manifest)
}

fn write_manifest(dir: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.json"), json).map_err(|e| format!("Could not write the backup manifest: {e}"))
}

fn record_after_fingerprint(backup: &BackupManifest, user: &SteamUser) {
    let dir = backups_dir().join(&backup.id);
    let Ok(text) = std::fs::read_to_string(dir.join("manifest.json")) else { return };
    let Ok(mut manifest) = serde_json::from_str::<BackupManifest>(&text) else { return };
    for u in manifest.users.iter_mut().filter(|u| u.user_id == user.user_id) {
        u.after_fingerprint = Some(file_fingerprint(&shortcuts_path(user)));
    }
    let _ = write_manifest(&dir, &manifest);
}

/// Copies a backup's artwork back into the account's grid folder. Whether all copies worked.
fn copy_grid_back(backup: &BackupManifest, user: &SteamUser) -> bool {
    let Some(saved) = backup.users.iter().find(|u| u.user_id == user.user_id) else { return true };
    let grid = Path::new(&user.folder).join("config").join("grid");
    let mut ok = true;
    for name in &saved.grid_files {
        let from = backups_dir().join(&backup.id).join(&user.user_id).join("grid").join(name);
        ok &= std::fs::create_dir_all(&grid).is_ok() && std::fs::copy(&from, grid.join(name)).is_ok();
    }
    ok
}

/// Puts an account's shortcuts.vdf back exactly as it was in the backup (byte for byte).
fn restore_user_file(backup: &BackupManifest, user: &SteamUser) -> Result<(), String> {
    let entry = backup.users.iter().find(|u| u.user_id == user.user_id).ok_or("Account not in backup")?;
    let target = shortcuts_path(user);
    let bytes = if entry.had_file {
        let saved = backups_dir().join(&backup.id).join(&user.user_id).join("shortcuts.vdf");
        std::fs::read(&saved).map_err(|e| format!("Could not read the backup {}: {e}", saved.display()))?
    } else {
        vdf::serialize(&[])
    };
    write_bytes(&target, &bytes).map_err(|e| e.message)
}

pub fn list_backups() -> Vec<BackupManifest> {
    let mut out: Vec<BackupManifest> = std::fs::read_dir(backups_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("manifest.json")).ok())
        .filter_map(|t| serde_json::from_str(&t).ok())
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created));
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreUserPreview {
    pub user_id: String,
    pub current_count: usize,
    pub backup_count: usize,
    /// In Steam now, not in the backup: lost by restoring.
    pub lost: Vec<ShortcutView>,
    /// In the backup, not in Steam now: brought back.
    pub regained: Vec<ShortcutView>,
    /// Steam changed the file after the operation that made this backup.
    pub changed_since: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePreview {
    pub backup_id: String,
    pub users: Vec<RestoreUserPreview>,
    pub warnings: Vec<String>,
    pub token: String,
}

pub fn restore_preview(backup_id: &str, snapshot: &SteamSnapshot, simulation: bool) -> Result<RestorePreview, String> {
    let manifest = list_backups().into_iter().find(|b| b.id == backup_id).ok_or("Backup not found")?;
    let mut preview = RestorePreview {
        backup_id: backup_id.to_string(),
        users: Vec::new(),
        warnings: Vec::new(),
        token: String::new(),
    };
    let key = |v: &ShortcutView| format!("{}|{}", v.app_id, norm_path(&v.exe));
    let mut fingerprints = Vec::new();
    for saved in &manifest.users {
        let Some(user) = snapshot.user(&saved.user_id) else {
            preview.warnings.push(format!("Steam account {} is not on this PC.", saved.user_id));
            continue;
        };
        let current: Vec<ShortcutView> = user.shortcuts.iter().map(ShortcutView::from).collect();
        let saved_keys: BTreeSet<String> = saved.shortcuts.iter().map(key).collect();
        let current_keys: BTreeSet<String> = current.iter().map(key).collect();
        let changed_since = saved.after_fingerprint.as_ref().is_some_and(|f| *f != user.fingerprint);
        if changed_since {
            preview.warnings.push(format!(
                "Steam account {}'s shortcuts changed after this backup's operation. Restoring loses those newer changes.",
                saved.user_id
            ));
        }
        fingerprints.push(format!("{}={}", user.user_id, user.fingerprint));
        preview.users.push(RestoreUserPreview {
            user_id: saved.user_id.clone(),
            current_count: current.len(),
            backup_count: saved.shortcuts.len(),
            lost: current.iter().filter(|v| !saved_keys.contains(&key(v))).cloned().collect(),
            regained: saved.shortcuts.iter().filter(|v| !current_keys.contains(&key(v))).cloned().collect(),
            changed_since,
        });
    }
    preview.token = format!("{:016x}", fnv64(&format!("{backup_id}|{simulation}|{}", fingerprints.join(","))));
    Ok(preview)
}

/// Restores a backup after the user saw [`restore_preview`]. The current state is backed
/// up first, so a restore can itself be undone.
pub fn restore_backup(
    _lock: &OpLock,
    lib: &mut Library,
    backup_id: &str,
    snapshot: &SteamSnapshot,
    prefs: &Preferences,
    token: &str,
    steam_running: &dyn Fn() -> bool,
) -> Result<RunRecord, String> {
    let preview = restore_preview(backup_id, snapshot, prefs.simulation_mode)?;
    if preview.token != token {
        return Err("Steam changed since the restore preview. Review it again.".into());
    }
    let manifest = list_backups().into_iter().find(|b| b.id == backup_id).ok_or("Backup not found")?;
    let mut record = RunRecord {
        id: format!("restore-{}", now_rfc3339()),
        kind: "restore".into(),
        at: now_rfc3339(),
        simulated: prefs.simulation_mode,
        user_ids: preview.users.iter().map(|u| u.user_id.clone()).collect(),
        details: vec![format!("Restore of backup {backup_id}")],
        ..RunRecord::default()
    };
    if prefs.simulation_mode {
        record.details.insert(0, "Simulation mode: nothing was written to Steam.".into());
        lib.record(record.clone());
        return Ok(record);
    }
    if steam_running() {
        return Err("Steam is running. Close Steam and try again; BoilerSteamXL never force-closes it.".into());
    }
    let users: Vec<&SteamUser> = preview.users.iter().filter_map(|u| snapshot.user(&u.user_id)).collect();
    for user in &users {
        if file_fingerprint(&shortcuts_path(user)) != user.fingerprint {
            return Err("Steam changed since the restore preview. Review it again.".into());
        }
    }
    let safety = create_backup("before-restore", &users, true, None).map_err(|e| format!("Backup of the current state failed; nothing was restored: {e}"))?;
    record.backup = Some(safety.id.clone());
    let mut restored: Vec<&SteamUser> = Vec::new();
    for user in &users {
        if let Err(e) = restore_user_file(&manifest, user) {
            // All or nothing: every account touched goes back to its state before the restore.
            let mut rolled_back = true;
            for u in restored.iter().chain(std::iter::once(user)) {
                rolled_back &= restore_user_file(&safety, u).is_ok();
                rolled_back &= copy_grid_back(&safety, u);
            }
            let e = if rolled_back {
                format!("{e} Nothing was restored; the previous state was put back.")
            } else {
                format!("{e} The previous state could not be fully put back; restore backup {} from Backups & History.", safety.id)
            };
            record.error = Some(e.clone());
            lib.record(record);
            return Err(e);
        }
        restored.push(user);
        if !copy_grid_back(&manifest, user) {
            record.details.push(format!("Some artwork of account {} could not be put back.", user.user_id));
        }
    }
    crate::log::info("restore", &format!("restored {backup_id}"));
    lib.record(record.clone());
    Ok(record)
}

// ---- Clean Rebuild ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildMode {
    /// Only shortcuts BoilerSteamXL created (the default).
    Managed,
    /// Every non-Steam shortcut of the account (advanced, destructive).
    AllNonSteam,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Decision {
    Unresolved,
    Keep,
    Replace { entry_id: String },
    /// Removal without replacement; `confirmed` is the separate explicit confirmation.
    Remove { confirmed: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RebuildDraft {
    pub mode: RebuildMode,
    pub user_id: String,
    pub started_at: String,
    /// Per existing shortcut key (`user#appid#index`).
    #[serde(default)]
    pub decisions: BTreeMap<String, Decision>,
    /// Replacement games approved in this rebuild, with the fingerprint they were
    /// approved for. Earlier approvals or imports do not count.
    #[serde(default)]
    pub approved: BTreeMap<String, String>,
}

impl RebuildDraft {
    pub fn new(mode: RebuildMode, user_id: &str) -> Self {
        RebuildDraft {
            mode,
            user_id: user_id.to_string(),
            started_at: now_rfc3339(),
            decisions: BTreeMap::new(),
            approved: BTreeMap::new(),
        }
    }

    /// Sets a decision, keeping the mapping one-to-one: a game replaces at most one
    /// old shortcut.
    pub fn set_decision(&mut self, key: &str, decision: Decision) -> Result<(), String> {
        if let Decision::Replace { entry_id } = &decision {
            if self
                .decisions
                .iter()
                .any(|(k, d)| k != key && matches!(d, Decision::Replace { entry_id: e } if e == entry_id))
            {
                return Err("That game already replaces another shortcut.".into());
            }
        }
        self.decisions.insert(key.to_string(), decision);
        Ok(())
    }

    pub fn approve(&mut self, lib: &Library, entry_id: &str) -> Result<(), String> {
        let entry = lib.entries.get(entry_id).ok_or("Unknown game")?;
        if entry.exe.is_empty() || !Path::new(&entry.exe).is_file() {
            return Err("Choose an existing executable for this game first.".into());
        }
        self.approved.insert(entry_id.to_string(), entry.fingerprint());
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub entry_id: String,
    pub title: String,
    /// "identity", "same executable", "same folder" or "same name".
    pub evidence: String,
    /// Strong evidence may be applied in bulk; a name match never is.
    pub strong: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectedShortcut {
    pub shortcut: SteamShortcut,
    pub key: String,
    pub in_scope: bool,
    pub origin: String,
    pub broken: bool,
    pub utility: Option<String>,
    pub duplicate_group: Option<usize>,
    pub suggestion: Option<Suggestion>,
    pub artwork: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub keys: Vec<String>,
    pub recommended_keep: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inspection {
    pub user_id: String,
    pub mode: RebuildMode,
    pub shortcuts: Vec<InspectedShortcut>,
    pub groups: Vec<DuplicateGroup>,
    pub counts: BTreeMap<String, usize>,
    pub error: Option<String>,
}

/// Step 1/2 of the wizard: the account's non-Steam shortcuts with what we know about
/// each, and suggested replacements from the library. Official Steam games are not
/// shortcuts and never appear here.
pub fn inspect(lib: &Library, snapshot: &SteamSnapshot, prefs: &Preferences, user_id: &str, mode: RebuildMode) -> Inspection {
    let mut out = Inspection {
        user_id: user_id.to_string(),
        mode,
        shortcuts: Vec::new(),
        groups: Vec::new(),
        counts: BTreeMap::new(),
        error: None,
    };
    let Some(user) = snapshot.user(user_id) else {
        out.error = Some(format!("Steam account {user_id} was not found."));
        return out;
    };
    if let Some(e) = &user.error {
        out.error = Some(format!("This account's shortcuts.vdf is damaged and will not be modified: {e}"));
        return out;
    }
    let grid = Path::new(&user.folder).join("config").join("grid");
    let entries: Vec<&crate::library::Entry> = lib
        .entries
        .values()
        .filter(|e| !matches!(e.status, Status::Ignored | Status::Rejected) && !e.exe.is_empty())
        .collect();

    // Duplicate groups: same executable and launch options.
    let mut by_exe: BTreeMap<String, Vec<&SteamShortcut>> = BTreeMap::new();
    for s in &user.shortcuts {
        by_exe.entry(format!("{}|{}", norm_path(&s.exe_path()), s.launch_options.trim())).or_default().push(s);
    }
    let mut group_of: BTreeMap<String, usize> = BTreeMap::new();
    for members in by_exe.values().filter(|m| m.len() > 1) {
        let best = members
            .iter()
            .max_by_key(|s| (s.exe_exists, s.managed, !grid_files_for(&grid, s.app_id).is_empty(), std::cmp::Reverse(s.index)))
            .map(|s| s.key())
            .unwrap_or_default();
        let index = out.groups.len();
        for s in members {
            group_of.insert(s.key(), index);
        }
        out.groups.push(DuplicateGroup {
            keys: members.iter().map(|s| s.key()).collect(),
            recommended_keep: best,
            reason: "Recommended: the one whose executable exists, that BoilerSteamXL manages and that has artwork.".into(),
        });
    }

    for s in &user.shortcuts {
        let in_scope = match mode {
            RebuildMode::Managed => s.managed,
            RebuildMode::AllNonSteam => true,
        };
        let exe = norm_path(&s.exe_path());
        let stem = Path::new(&s.exe_path()).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        let utility = prefs
            .rules
            .check(&s.exe_path(), None, None)
            .map(|x| x.reason)
            .or_else(|| crate::rules::categorize(&stem, None).map(|c| format!("Utility: {}", c.label())));
        let suggestion = entries
            .iter()
            .find(|e| e.import.as_ref().is_some_and(|r| r.identity == s.identity) || identity_for(&e.id) == s.identity)
            .map(|e| (e, "identity", true))
            .or_else(|| entries.iter().find(|e| norm_path(&e.exe) == exe).map(|e| (e, "same executable", true)))
            .or_else(|| {
                // The deepest folder wins (a collection's own entry contains other games), and a
                // folder alone is never strong enough to bulk-apply.
                entries
                    .iter()
                    .filter(|e| e.source == crate::library::Source::Folder && exe.starts_with(&format!("{}/", norm_path(&e.game_dir))))
                    .max_by_key(|e| norm_path(&e.game_dir).len())
                    .map(|e| (e, "same folder", false))
            })
            .or_else(|| {
                let name = crate::norm_name(&s.name);
                entries
                    .iter()
                    .find(|e| name.len() >= 3 && crate::norm_name(e.display_title()) == name)
                    .map(|e| (e, "same name", false))
            })
            .map(|(e, evidence, strong)| Suggestion {
                entry_id: e.id.clone(),
                title: e.display_title().to_string(),
                evidence: evidence.to_string(),
                strong,
            });
        out.shortcuts.push(InspectedShortcut {
            key: s.key(),
            in_scope,
            origin: if s.managed { "BoilerSteamXL" } else if s.boilr { "BoilR" } else { "Other / manual" }.to_string(),
            broken: !s.exe_exists,
            utility,
            duplicate_group: group_of.get(&s.key()).copied(),
            suggestion,
            artwork: grid_files_for(&grid, s.app_id)
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            shortcut: s.clone(),
        });
    }
    let count = |f: &dyn Fn(&InspectedShortcut) -> bool| out.shortcuts.iter().filter(|s| f(s)).count();
    out.counts = BTreeMap::from([
        ("total".to_string(), out.shortcuts.len()),
        ("in_scope".to_string(), count(&|s| s.in_scope)),
        ("managed".to_string(), count(&|s| s.origin == "BoilerSteamXL")),
        ("boilr".to_string(), count(&|s| s.origin == "BoilR")),
        ("other".to_string(), count(&|s| s.origin == "Other / manual")),
        ("broken".to_string(), count(&|s| s.broken)),
        ("duplicates".to_string(), count(&|s| s.duplicate_group.is_some())),
        ("utilities".to_string(), count(&|s| s.utility.is_some())),
        ("with_artwork".to_string(), count(&|s| !s.artwork.is_empty())),
        ("unmatched".to_string(), count(&|s| s.suggestion.is_none())),
    ]);
    out
}

/// The rebuild plan from the saved draft decisions.
pub fn plan_rebuild(lib: &Library, snapshot: &SteamSnapshot, prefs: &Preferences) -> Result<Plan, String> {
    let draft = lib.rebuild_draft.as_ref().ok_or("No Clean Rebuild in progress.")?;
    let user = snapshot.user(&draft.user_id).ok_or("The selected Steam account was not found.")?;
    if let Some(e) = &user.error {
        return Err(format!("This account's shortcuts.vdf is damaged and will not be modified: {e}"));
    }
    let mut plan = Plan {
        kind: "rebuild".into(),
        mode: Some(draft.mode),
        user_ids: vec![user.user_id.clone()],
        simulation: prefs.simulation_mode,
        ..Plan::default()
    };
    let approved_valid = |id: &str| -> Option<&crate::library::Entry> {
        let entry = lib.entries.get(id)?;
        (draft.approved.get(id) == Some(&entry.fingerprint()) && Path::new(&entry.exe).is_file()).then_some(entry)
    };
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut kept_exes: BTreeSet<String> = BTreeSet::new();
    let mut out_of_scope = 0;

    for s in &user.shortcuts {
        let key = s.key();
        let in_scope = draft.mode == RebuildMode::AllNonSteam || s.managed;
        let decision = if in_scope { draft.decisions.get(&key).cloned().unwrap_or(Decision::Unresolved) } else { Decision::Keep };
        let mut op = Op {
            kind: OpKind::Keep,
            user_id: user.user_id.clone(),
            entry_id: None,
            shortcut_key: Some(key.clone()),
            old: Some(s.into()),
            new: None,
            notes: Vec::new(),
            allow_duplicate: false,
            identity: String::new(),
        };
        match decision {
            Decision::Keep => {
                if !in_scope {
                    out_of_scope += 1;
                    op.notes.push("Not part of this rebuild (not created by BoilerSteamXL).".into());
                }
                kept_exes.insert(norm_path(&s.exe_path()));
            }
            Decision::Unresolved => {
                op.kind = OpKind::Unresolved;
                op.notes.push("No decision: left exactly as it is.".into());
                kept_exes.insert(norm_path(&s.exe_path()));
            }
            Decision::Remove { confirmed } => {
                if !confirmed {
                    plan.blocked.push(Blocked {
                        id: key.clone(),
                        title: s.name.clone(),
                        reason: "Removing without a replacement needs its own confirmation.".into(),
                    });
                    continue;
                }
                op.kind = OpKind::Remove;
                op.notes.push("Removed without a replacement. Its launch command is kept in the backup.".into());
                if !s.launch_options.trim().is_empty() {
                    op.notes.push(format!("Custom launch options \"{}\" will no longer be in Steam.", s.launch_options));
                }
            }
            Decision::Replace { entry_id } => match approved_valid(&entry_id) {
                Some(entry) if used.insert(entry_id.clone()) => {
                    let new = with_artwork(entry_view(entry), &user.folder, &entry_id, &mut op.notes);
                    op.kind = OpKind::Replace;
                    op.entry_id = Some(entry_id.clone());
                    op.identity = entry.import.as_ref().map_or_else(|| identity_for(&entry_id), |r| r.identity.clone());
                    if new.app_id != s.app_id {
                        op.notes.push(format!(
                            "Steam ID changes from {} to {}: artwork is copied to the new ID; collections and per-game Steam settings tied to the old ID may not carry over.",
                            s.app_id, new.app_id
                        ));
                    }
                    if s.launch_options.trim() != new.launch_options.trim() {
                        op.notes.push(format!("Launch options change from \"{}\" to \"{}\".", s.launch_options, new.launch_options));
                    }
                    op.new = Some(new);
                }
                Some(_) => {
                    plan.blocked.push(Blocked { id: key.clone(), title: s.name.clone(), reason: "That game already replaces another shortcut.".into() });
                    continue;
                }
                None => {
                    plan.blocked.push(Blocked {
                        id: key.clone(),
                        title: s.name.clone(),
                        reason: "The replacement game is not approved in this rebuild (or changed after approval).".into(),
                    });
                    continue;
                }
            },
        }
        plan.ops.push(op);
    }

    // Approved games that replace nothing are created, unless a kept shortcut already
    // starts the same executable.
    for (entry_id, _) in draft.approved.iter() {
        if used.contains(entry_id) {
            continue;
        }
        let Some(entry) = approved_valid(entry_id) else {
            if let Some(e) = lib.entries.get(entry_id) {
                plan.blocked.push(Blocked { id: entry_id.clone(), title: e.display_title().to_string(), reason: "Changed after approval; approve it again.".into() });
            }
            continue;
        };
        if kept_exes.contains(&norm_path(&entry.exe)) {
            plan.warnings.push(format!("\"{}\" is not created: a kept shortcut already starts it.", entry.display_title()));
            continue;
        }
        let mut notes = Vec::new();
        let new = with_artwork(entry_view(entry), &user.folder, entry_id, &mut notes);
        plan.ops.push(Op {
            kind: OpKind::Create,
            user_id: user.user_id.clone(),
            entry_id: Some(entry_id.clone()),
            shortcut_key: None,
            old: None,
            new: Some(new),
            notes,
            allow_duplicate: false,
            identity: entry.import.as_ref().map_or_else(|| identity_for(entry_id), |r| r.identity.clone()),
        });
    }
    let pending = lib
        .entries
        .values()
        .filter(|e| matches!(e.status, Status::NeedsReview | Status::PossibleDuplicate) && !draft.approved.contains_key(&e.id))
        .count();
    plan.finish(snapshot, &[("out_of_scope", out_of_scope), ("still_review", pending)]);
    Ok(plan)
}

/// Executes a Clean Rebuild. Requires the plan token the user saw, the exact typed
/// phrase, and the risk acknowledgement; refuses while anything is blocked.
pub fn execute_rebuild(
    _lock: &OpLock,
    lib: &mut Library,
    snapshot: &SteamSnapshot,
    prefs: &Preferences,
    token: &str,
    phrase: &str,
    acknowledged: bool,
    steam_running: &dyn Fn() -> bool,
) -> Result<RunRecord, String> {
    if phrase != REBUILD_PHRASE {
        return Err(format!("Type {REBUILD_PHRASE} exactly to confirm."));
    }
    if !acknowledged {
        return Err("Confirm that you understand the risks first.".into());
    }
    let plan = plan_rebuild(lib, snapshot, prefs)?;
    if plan.token != token {
        return Err("Steam or your decisions changed since the preview. Review the plan again.".into());
    }
    if !plan.blocked.is_empty() {
        return Err(format!("{} item(s) still need a decision.", plan.blocked.len()));
    }
    if plan.summary.get("duplicates_created").copied().unwrap_or(0) > 0 {
        return Err("The plan would create duplicate shortcuts; review it again.".into());
    }
    let changes = plan.ops.iter().filter(|o| !matches!(o.kind, OpKind::Keep | OpKind::Unresolved)).count();
    if changes == 0 {
        return Err("The plan contains no changes.".into());
    }
    let mut record = RunRecord {
        id: format!("rebuild-{}", now_rfc3339()),
        kind: "rebuild".into(),
        at: now_rfc3339(),
        simulated: plan.simulation,
        user_ids: plan.user_ids.clone(),
        counts: plan.summary.clone(),
        details: plan
            .ops
            .iter()
            .filter(|o| !matches!(o.kind, OpKind::Keep | OpKind::Unresolved))
            .map(|o| {
                let old = o.old.as_ref().map_or("", |v| v.name.as_str());
                let new = o.new.as_ref().map_or("", |v| v.name.as_str());
                format!("{:?}: {old} → {new}", o.kind)
            })
            .collect(),
        ..RunRecord::default()
    };
    if plan.simulation {
        record.details.insert(0, "Simulation mode: nothing was written to Steam.".into());
        lib.record(record.clone());
        return Ok(record);
    }
    if steam_running() {
        return Err("Steam is running. Close Steam (Steam menu → Exit), then continue. BoilerSteamXL never force-closes Steam.".into());
    }
    let users: Vec<&SteamUser> = plan.user_ids.iter().filter_map(|u| snapshot.user(u)).collect();
    let backup = create_backup("rebuild", &users, true, Some(&plan)).map_err(|e| format!("Backup failed, nothing was changed: {e}"))?;
    record.backup = Some(backup.id.clone());

    match apply_ops(&plan, snapshot, &backup) {
        Ok(warnings) => record.details.extend(warnings),
        Err(e) => {
            record.error = Some(e.clone());
            crate::log::error("rebuild", &e);
            lib.record(record);
            return Err(e);
        }
    }

    let now = now_rfc3339();
    // Removals first, so an entry whose old shortcut was removed but which was written
    // again in this run (as a replacement elsewhere) ends up imported.
    let mut ordered: Vec<&Op> = plan.ops.iter().filter(|o| o.kind == OpKind::Remove).collect();
    ordered.extend(plan.ops.iter().filter(|o| o.kind != OpKind::Remove));
    for op in ordered {
        match op.kind {
            OpKind::Create | OpKind::Replace => {
                let (Some(id), Some(new)) = (&op.entry_id, &op.new) else { continue };
                if let Some(entry) = lib.entries.get_mut(id) {
                    let identity = op_identity(op).unwrap_or_else(|| identity_for(id));
                    let record = entry.import.get_or_insert_with(|| ImportRecord {
                        app_id: new.app_id,
                        identity: identity.clone(),
                        user_ids: Vec::new(),
                        at: now.clone(),
                        name: new.name.clone(),
                        exe: entry.exe.clone(),
                        launch_options: entry.launch_options.clone(),
                    });
                    record.identity = identity;
                    record.app_id = new.app_id;
                    record.at = now.clone();
                    record.name = new.name.clone();
                    record.exe = entry.exe.clone();
                    record.launch_options = entry.launch_options.clone();
                    if !record.user_ids.contains(&op.user_id) {
                        record.user_ids.push(op.user_id.clone());
                    }
                    entry.status = Status::Imported;
                    entry.approval = None;
                    entry.user_linked = false;
                    entry.note = None;
                }
            }
            OpKind::Remove => {
                let identity = op.shortcut_key.as_ref().and_then(|k| {
                    snapshot.user(&op.user_id)?.shortcuts.iter().find(|s| &s.key() == k).map(|s| s.identity.clone())
                });
                for entry in lib.entries.values_mut() {
                    let Some(record) = entry.import.as_mut().filter(|r| Some(&r.identity) == identity.as_ref()) else { continue };
                    // Only this account's shortcut went; others it was imported into remain.
                    record.user_ids.retain(|u| u != &op.user_id);
                    if record.user_ids.is_empty() {
                        entry.import = None;
                        entry.status = Status::NeedsReview;
                        entry.note = Some("Its shortcut was removed by a Clean Rebuild.".into());
                    }
                }
            }
            _ => {}
        }
    }
    lib.rebuild_draft = None;
    crate::log::info("rebuild", &format!("{changes} change(s), backup {}", backup.id));
    lib.record(record.clone());
    Ok(record)
}
