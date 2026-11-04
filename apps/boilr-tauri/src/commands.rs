//! BoilerSteamXL commands for the interface. Every Steam write goes through
//! `bsxl_engine::import`, which needs an approval, a confirmed plan token and a verified
//! backup; in Simulation mode it writes nothing.

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use bsxl_engine::{
    diagnostics,
    import::{self, Decision, ExecOptions, Inspection, Plan, RebuildDraft, RebuildMode, RestorePreview},
    library::{Entry, EntryEdit, Library, RunRecord, ScanSummary, Status},
    prefs::Preferences,
    rules::{self, Rule, RuleSet},
    scan::{self, ScanRoot},
    schedule::{self, Schedule},
    steam::{self, SteamSnapshot},
};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

#[derive(Default)]
pub struct AppState {
    pub cancel: Arc<AtomicBool>,
    pub mode: String,
}

/// How long a command waits for another operation (for example the scheduled refresh
/// saving its results) before giving up.
const LOCK_WAIT: Duration = Duration::from_secs(15);

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn snapshot(prefs: &Preferences) -> SteamSnapshot {
    steam::snapshot(&steam::detect_steam_path(prefs.steam_path.as_deref()))
}

/// Loads the library, applies `change`, saves it, all under the operation lock, which is
/// shared with the scheduled `--scan` so neither overwrites the other's changes.
fn with_library<T>(_state: &AppState, change: impl FnOnce(&mut Library, &Preferences) -> Result<T, String>) -> Result<T, String> {
    let _lock = import::wait_for_lock(LOCK_WAIT)?;
    let prefs = Preferences::load();
    let mut lib = Library::load();
    let value = change(&mut lib, &prefs)?;
    lib.save().map_err(err)?;
    Ok(value)
}

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    mode: String,
    data_dir: String,
    simulation: bool,
    steam_running: bool,
    first_run_complete: bool,
    rebuild_phrase: &'static str,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let prefs = Preferences::load();
    AppInfo {
        version: bsxl_engine::VERSION.to_string(),
        mode: state.mode.clone(),
        data_dir: bsxl_engine::data_dir().to_string_lossy().to_string(),
        simulation: prefs.simulation_mode,
        steam_running: steam::is_steam_running(),
        first_run_complete: prefs.first_run_complete,
        rebuild_phrase: import::REBUILD_PHRASE,
    }
}

#[tauri::command]
pub fn get_prefs() -> Preferences {
    Preferences::load()
}

/// Saves preferences. A changed schedule updates the Windows task; a changed Steam folder
/// is also given to the artwork tools.
#[tauri::command(async)]
pub fn save_prefs(prefs: Preferences) -> Result<Preferences, String> {
    let _lock = import::wait_for_lock(LOCK_WAIT)?;
    let old = Preferences::load();
    prefs.schedule.validate()?;
    if let Some(path) = prefs.steam_path.as_deref().filter(|p| !p.trim().is_empty()) {
        steam::validate_steam_path(path)?;
    }
    for rule in &prefs.rules.rules {
        rules::validate(rule.kind, &rule.pattern).map_err(|e| format!("Rule \"{}\": {e}", rule.pattern))?;
    }
    if old.schedule != prefs.schedule {
        let exe = std::env::current_exe().map_err(err)?;
        schedule::apply(&prefs.schedule, &exe.to_string_lossy())?;
    }
    if old.steam_path != prefs.steam_path {
        if let Ok(mut settings) = boilr_core::settings::Settings::new() {
            settings.steam.location = prefs.steam_path.clone().filter(|p| !p.trim().is_empty());
            let platforms = boilr::platforms::get_platforms();
            let _ = boilr_core::settings::save_settings(&settings, &boilr::platforms::platform_sections(&platforms));
        }
    }
    prefs.save().map_err(err)?;
    Ok(prefs)
}

#[derive(Serialize)]
pub struct SteamUserInfo {
    user_id: String,
    persona: Option<String>,
    most_recent: bool,
    shortcut_count: usize,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct SteamInfo {
    path: String,
    valid: bool,
    error: Option<String>,
    users: Vec<SteamUserInfo>,
    library_folders: Vec<String>,
    official_count: usize,
    running: bool,
}

#[tauri::command(async)]
pub fn steam_info() -> SteamInfo {
    let snap = snapshot(&Preferences::load());
    SteamInfo {
        path: snap.steam_path.clone(),
        valid: snap.valid,
        error: snap.error.clone(),
        users: snap
            .users
            .iter()
            .map(|u| SteamUserInfo {
                user_id: u.user_id.clone(),
                persona: u.persona.clone(),
                most_recent: u.most_recent,
                shortcut_count: u.shortcuts.len(),
                error: u.error.clone(),
            })
            .collect(),
        library_folders: snap.library_folders.clone(),
        official_count: snap.official.len(),
        running: steam::is_steam_running(),
    }
}

#[tauri::command]
pub fn check_steam_path(path: String) -> Result<(), String> {
    steam::validate_steam_path(&path)
}

/// Common game folders that exist on this PC, for the first-run wizard.
#[tauri::command]
pub fn suggest_scan_roots() -> Vec<String> {
    let mut out = Vec::new();
    for drive in ['C', 'D', 'E', 'F', 'G'] {
        for folder in ["Games", "Jeux", "PC Games", "Installed Games"] {
            let path = format!("{drive}:\\{folder}");
            if Path::new(&path).is_dir() {
                out.push(path);
            }
        }
    }
    out
}

#[derive(Serialize)]
pub struct LibraryView {
    entries: Vec<Entry>,
    last_scan: Option<ScanSummary>,
    history: Vec<RunRecord>,
    counts: std::collections::BTreeMap<String, usize>,
    has_rebuild_draft: bool,
    schedule: ScheduleInfo,
}

#[derive(Serialize)]
pub struct ScheduleInfo {
    installed: bool,
    next_run: Option<String>,
    last_scheduled: Option<String>,
}

fn schedule_info(prefs: &Preferences, lib: &Library) -> ScheduleInfo {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    ScheduleInfo {
        installed: schedule::is_installed(),
        next_run: schedule::next_run(&prefs.schedule, now)
            .and_then(|t| t.format(&time::format_description::well_known::Rfc3339).ok()),
        last_scheduled: lib.history.iter().find(|r| r.scheduled && r.kind == "scan").map(|r| r.at.clone()),
    }
}

#[tauri::command(async)]
pub fn get_library() -> LibraryView {
    let prefs = Preferences::load();
    let lib = Library::load();
    let mut counts = std::collections::BTreeMap::new();
    for e in lib.entries.values() {
        *counts.entry(serde_json::to_value(e.status).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()).or_insert(0) += 1;
    }
    let mut entries: Vec<Entry> = lib.entries.values().cloned().collect();
    entries.sort_by_cached_key(|e| e.display_title().to_lowercase());
    LibraryView {
        schedule: schedule_info(&prefs, &lib),
        has_rebuild_draft: lib.rebuild_draft.is_some(),
        entries,
        last_scan: lib.last_scan.clone(),
        history: lib.history.iter().take(100).cloned().collect(),
        counts,
    }
}

/// Games from BoilR's launcher integrations, when the user turned them on.
pub fn launcher_games(prefs: &Preferences) -> Option<Vec<bsxl_engine::library::LauncherGame>> {
    if !prefs.include_launchers {
        return None;
    }
    let mut games = Vec::new();
    for platform in boilr::platforms::get_platforms() {
        // The standalone scanner is replaced by BoilerSteamXL's own.
        if !platform.enabled() || platform.code_name() == "standalone" {
            continue;
        }
        let (code, name) = (platform.code_name().to_string(), platform.name().to_string());
        if let Ok(shortcuts) = boilr::platforms::get_platform_shortcuts(platform) {
            for s in shortcuts {
                let s = s.shortcut;
                games.push(bsxl_engine::library::LauncherGame {
                    launcher_code: code.clone(),
                    launcher_name: name.clone(),
                    key: s.app_id.to_string(),
                    title: s.app_name.clone(),
                    exe: s.exe.trim_matches('"').to_string(),
                    start_dir: s.start_dir.trim_matches('"').to_string(),
                    launch_options: s.launch_options.clone(),
                });
            }
        }
    }
    Some(games)
}

/// Refresh = discovery. Scans in the background with progress events; never imports.
#[tauri::command]
pub async fn refresh_library<R: tauri::Runtime>(app: tauri::AppHandle<R>, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let handle = app.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        let prefs = Preferences::load();
        let launchers = launcher_games(&prefs);
        let discovery = bsxl_engine::discover(
            &prefs,
            &|p| {
                let _ = handle.emit("scan-progress", p);
            },
            &cancel,
        );
        // Merge into the library as it is now (decisions made during the scan included).
        let _lock = import::wait_for_lock(LOCK_WAIT)?;
        let mut lib = Library::load();
        let summary = bsxl_engine::merge(&mut lib, discovery, launchers.as_deref(), &prefs);
        lib.mark_notified(&summary.newly_discovered);
        lib.record(scan_record(&summary, false));
        lib.save().map(|_| summary).map_err(err)
    })
    .await
    .map_err(err)??;
    Ok(summary)
}

pub fn scan_record(summary: &ScanSummary, scheduled: bool) -> RunRecord {
    RunRecord {
        id: format!("scan-{}", summary.at),
        kind: "scan".into(),
        at: summary.at.clone(),
        scheduled,
        counts: [
            ("folders", summary.folders_scanned),
            ("games", summary.games_found),
            ("new", summary.new_games),
            ("needs_review", summary.needs_review),
            ("ignored_executables", summary.exes_excluded),
            ("missing", summary.missing),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect(),
        details: summary.root_errors.iter().map(|(r, e)| format!("{r}: {e}")).collect(),
        error: summary.cancelled.then(|| "Cancelled".to_string()),
        ..RunRecord::default()
    }
}

#[tauri::command(async)]
pub fn cancel_refresh(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

// ---- review queue -------------------------------------------------------------------

#[derive(Serialize)]
pub struct ActionResult {
    errors: Vec<(String, String)>,
}

#[tauri::command(async)]
pub fn approve(state: State<'_, AppState>, ids: Vec<String>, allow_duplicate: bool) -> Result<ActionResult, String> {
    with_library(&state, |lib, _| Ok(ActionResult { errors: lib.approve(&ids, allow_duplicate) }))
}

#[tauri::command(async)]
pub fn approve_eligible(state: State<'_, AppState>) -> Result<ActionResult, String> {
    with_library(&state, |lib, prefs| {
        let snap = snapshot(prefs);
        if !snap.valid || snap.users.iter().any(|u| u.error.is_some()) {
            return Err("Steam could not be fully read, so duplicates can't be ruled out. Approve games one by one, or fix the Steam folder in Settings.".into());
        }
        lib.reconcile(&snap);
        let ids = lib.eligible_for_bulk_approval();
        Ok(ActionResult { errors: lib.approve(&ids, false) })
    })
}

#[tauri::command(async)]
pub fn reject(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    with_library(&state, |lib, _| {
        lib.reject(&ids);
        Ok(())
    })
}

#[tauri::command(async)]
pub fn ignore(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    with_library(&state, |lib, prefs| {
        let rules = lib.ignore(&ids);
        if !rules.is_empty() {
            let mut prefs = prefs.clone();
            prefs.rules.rules.extend(rules);
            prefs.save().map_err(err)?;
        }
        Ok(())
    })
}

#[tauri::command(async)]
pub fn restore_entries(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    with_library(&state, |lib, prefs| {
        let rule_ids = lib.restore(&ids);
        if !rule_ids.is_empty() {
            let mut prefs = prefs.clone();
            prefs.rules.rules.retain(|r| !rule_ids.contains(&r.id));
            prefs.save().map_err(err)?;
        }
        lib.reconcile(&snapshot(prefs));
        Ok(())
    })
}

#[tauri::command(async)]
pub fn edit_entry(state: State<'_, AppState>, id: String, edit: EntryEdit) -> Result<Entry, String> {
    with_library(&state, |lib, prefs| {
        lib.edit(&id, &edit)?;
        lib.reconcile(&snapshot(prefs));
        lib.get(&id).cloned().ok_or_else(|| "Unknown game".into())
    })
}

#[tauri::command(async)]
pub fn reset_override(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_library(&state, |lib, prefs| {
        lib.reset_override(&id)?;
        lib.reconcile(&snapshot(prefs));
        Ok(())
    })
}

#[tauri::command(async)]
pub fn link_entry(state: State<'_, AppState>, id: String, shortcut_key: String) -> Result<(), String> {
    with_library(&state, |lib, prefs| {
        let snap = snapshot(prefs);
        let shortcut = snap.all_shortcuts().find(|s| s.key() == shortcut_key).ok_or("That Steam shortcut was not found.")?;
        lib.link(&id, shortcut)
    })
}

#[derive(Serialize)]
pub struct ShortcutChoice {
    key: String,
    user_id: String,
    name: String,
    exe: String,
}

#[tauri::command(async)]
pub fn list_shortcuts() -> Vec<ShortcutChoice> {
    snapshot(&Preferences::load())
        .all_shortcuts()
        .map(|s| ShortcutChoice { key: s.key(), user_id: s.user_id.clone(), name: s.name.clone(), exe: s.exe.clone() })
        .collect()
}

/// A non-Steam shortcut in Steam now, with the library game it belongs to (if any).
#[derive(Serialize)]
pub struct SteamGame {
    key: String,
    user_id: String,
    app_id: u32,
    name: String,
    exe: String,
    start_dir: String,
    launch_options: String,
    /// "BoilerSteamXL", "BoilR" or empty (made by hand or another tool).
    made_by: &'static str,
    exe_exists: bool,
    entry_id: Option<String>,
    /// Will be removed by the next sync.
    removal: bool,
}

/// Every non-Steam shortcut in the Steam accounts BoilerSteamXL writes to.
#[tauri::command(async)]
pub fn steam_games() -> Vec<SteamGame> {
    let prefs = Preferences::load();
    let lib = Library::load();
    let snap = snapshot(&prefs);
    snap.target_users(&prefs.target_users)
        .into_iter()
        .flat_map(|u| u.shortcuts.iter())
        .map(|s| SteamGame {
            key: s.key(),
            user_id: s.user_id.clone(),
            app_id: s.app_id,
            name: s.name.clone(),
            exe: s.exe_path(),
            start_dir: s.start_dir.clone(),
            launch_options: s.launch_options.clone(),
            made_by: if s.managed { "BoilerSteamXL" } else if s.boilr { "BoilR" } else { "" },
            exe_exists: s.exe_exists,
            entry_id: lib.entry_for_shortcut(s).map(|e| e.id.clone()),
            removal: lib.removals.iter().any(|r| r.matches(s)),
        })
        .collect()
}

/// Marks shortcuts to be removed from Steam by the next sync, or takes that back.
/// Nothing is written to Steam here.
#[tauri::command(async)]
pub fn set_removal(state: State<'_, AppState>, keys: Vec<String>, remove: bool) -> Result<(), String> {
    with_library(&state, |lib, prefs| {
        let snap = snapshot(prefs);
        for key in &keys {
            let s = snap.all_shortcuts().find(|s| &s.key() == key).ok_or("Steam changed; refresh and try again.")?;
            lib.request_removal(s, remove);
        }
        Ok(())
    })
}

#[derive(Serialize)]
pub struct ManualCheck {
    exists: bool,
    is_exe: bool,
    suggested_title: String,
    duplicates: Vec<bsxl_engine::library::DupMatch>,
    preview: Option<import::ShortcutView>,
}

/// Checks a manually entered executable without running it.
#[tauri::command(async)]
pub fn check_manual(exe: String, title: Option<String>, launch_options: Option<String>) -> ManualCheck {
    let exe = exe.trim().trim_matches('"').to_string();
    let path = Path::new(&exe);
    let exists = path.is_file();
    let is_exe = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let folder_title = path.parent().and_then(|p| p.file_name()).map(|n| bsxl_engine::title::title_from_folder(&n.to_string_lossy()));
    let suggested_title = if bsxl_engine::title::is_generic_exe(&stem) { folder_title.unwrap_or(stem) } else { stem };
    let mut duplicates = Vec::new();
    if exists {
        let prefs = Preferences::load();
        let snap = snapshot(&prefs);
        let lib = Library::load();
        let mut probe = Library::new_entry("manual:probe".into(), bsxl_engine::library::Source::Manual, "");
        probe.exe = exe.clone();
        probe.launch_options = launch_options.clone().unwrap_or_default();
        probe.custom_title = title.clone();
        probe.title = suggested_title.clone();
        let others: Vec<Entry> = lib.entries.values().cloned().collect();
        if let Some(d) = bsxl_engine::library::detect_duplicate(&probe, &snap, &others) {
            duplicates.push(d.matched);
        }
        for other in others.iter().filter(|o| bsxl_engine::norm_path(&o.exe) == bsxl_engine::norm_path(&exe)) {
            if !duplicates.iter().any(|d| d.entry_id.as_deref() == Some(other.id.as_str())) {
                duplicates.push(bsxl_engine::library::DupMatch {
                    kind: "entry".into(),
                    name: other.display_title().to_string(),
                    path: other.exe.clone(),
                    user_id: None,
                    app_id: None,
                    entry_id: Some(other.id.clone()),
                    boilr: false,
                });
            }
        }
    }
    let name = title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| suggested_title.clone());
    let preview = exists.then(|| {
        let dir = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let quoted = format!("\"{exe}\"");
        let dirq = format!("\"{dir}\"");
        let args = launch_options.unwrap_or_default();
        let app_id = steam_shortcuts_util::calculate_app_id_for_shortcut(&steam_shortcuts_util::Shortcut::new("0", &name, &quoted, &dirq, "", "", &args));
        import::ShortcutView { app_id, name: name.clone(), exe: quoted, start_dir: dirq, launch_options: args, icon: String::new() }
    });
    ManualCheck { exists, is_exe, suggested_title, duplicates, preview }
}

#[tauri::command(async)]
pub fn add_manual(state: State<'_, AppState>, edit: EntryEdit, allow_duplicate: bool) -> Result<String, String> {
    with_library(&state, |lib, prefs| {
        let id = lib.add_manual(&edit, &snapshot(prefs))?;
        if allow_duplicate {
            if let Some(e) = lib.entries.get_mut(&id) {
                if e.status == Status::AlreadyInSteam {
                    e.status = Status::PossibleDuplicate;
                }
            }
        }
        Ok(id)
    })
}

#[tauri::command(async)]
pub fn remove_manual(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_library(&state, |lib, _| lib.remove_manual(&id))
}

#[derive(Serialize)]
pub struct FolderAnalysis {
    games: Vec<scan::DiscoveredGame>,
    excluded: Vec<scan::ExcludedExe>,
}

/// Analyses one folder (for "+ Add Game → choose a folder"): its executables, scored
/// and filtered by the exclusion rules. Nothing is run.
#[tauri::command(async)]
pub fn analyze_folder(path: String) -> FolderAnalysis {
    let prefs = Preferences::load();
    let roots = vec![ScanRoot { path, enabled: true, max_depth: 4 }];
    let result = scan::scan(&roots, &prefs.rules, &mut scan::PeCache::new(), &|_| {}, &AtomicBool::new(false));
    FolderAnalysis { excluded: result.orphan_excluded, games: result.games }
}

// ---- import -------------------------------------------------------------------------

#[tauri::command(async)]
pub fn plan_import() -> Plan {
    let prefs = Preferences::load();
    import::plan_import(&Library::load(), &snapshot(&prefs), &prefs, None)
}

#[tauri::command]
pub async fn execute_import(state: State<'_, AppState>, token: String) -> Result<import::Outcome, String> {
    let _ = state;
    tauri::async_runtime::spawn_blocking(move || {
        let lock = import::wait_for_lock(LOCK_WAIT)?;
        let prefs = Preferences::load();
        let mut lib = Library::load();
        let snap = snapshot(&prefs);
        let running = steam::is_steam_running;
        let options = ExecOptions { steam_running: &running, defer_if_running: false, scheduled: false, only: None };
        let result = import::execute_import(&lock, &mut lib, &snap, &prefs, &token, &options);
        lib.save().map_err(err)?;
        result
    })
    .await
    .map_err(err)?
}

/// Asks Steam to exit on its own and waits up to 30 seconds. Never kills it. The wait runs on a
/// blocking thread so it does not hold up the async workers (artwork downloads, progress).
#[tauri::command]
pub async fn close_steam() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = steam::detect_steam_path(Preferences::load().steam_path.as_deref());
        if steam::close_steam_gracefully(&path, Duration::from_secs(30)) {
            Ok(())
        } else {
            Err("Steam is still running. Close it from the Steam menu (Steam → Exit), then try again.".into())
        }
    })
    .await
    .map_err(err)?
}

/// Starts Steam again (after BoilerSteamXL asked it to exit for an import).
#[tauri::command]
pub fn start_steam() -> Result<(), String> {
    let path = steam::detect_steam_path(Preferences::load().steam_path.as_deref());
    std::process::Command::new(Path::new(&path).join("steam.exe")).spawn().map(|_| ()).map_err(|e| format!("Could not start Steam: {e}"))
}

// ---- rules --------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RuleMatch {
    path: String,
    game: String,
}

/// Which known executables a rule matches, for the rule editor's preview.
#[tauri::command(async)]
pub fn preview_rule(rule: Rule) -> Result<Vec<RuleMatch>, String> {
    rules::validate(rule.kind, &rule.pattern)?;
    let lib = Library::load();
    let mut single = RuleSet { rules: vec![Rule { enabled: true, ..rule }], allowlist: Vec::new() };
    single.allowlist.clear();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let orphans = lib.last_scan.as_ref().map(|s| s.orphan_excluded.clone()).unwrap_or_default();
    let all = lib
        .entries
        .values()
        .flat_map(|e| {
            e.candidates
                .iter()
                .map(|c| (c.path.clone(), e.display_title().to_string(), e.game_dir.clone()))
                .chain(e.excluded.iter().map(|x| (x.path.clone(), e.display_title().to_string(), e.game_dir.clone())))
                .collect::<Vec<_>>()
        })
        .chain(orphans.into_iter().map(|o| (o.path, String::new(), String::new())));
    for (path, game, dir) in all {
        if !seen.insert(bsxl_engine::norm_path(&path)) {
            continue;
        }
        let game_dir = (!dir.is_empty()).then_some(dir.as_str());
        if single.check(&path, game_dir, None).is_some() {
            out.push(RuleMatch { path, game });
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn validate_rule(kind: rules::RuleKind, pattern: String) -> Result<(), String> {
    rules::validate(kind, &pattern)
}

#[tauri::command]
pub fn recommended_rules() -> RuleSet {
    RuleSet::recommended()
}

#[tauri::command]
pub fn export_json(path: String, what: String) -> Result<(), String> {
    let prefs = Preferences::load();
    let value = match what.as_str() {
        "rules" => serde_json::to_value(&prefs.rules).map_err(err)?,
        _ => {
            let lib = Library::load();
            let manual: Vec<&Entry> = lib.entries.values().filter(|e| e.source == bsxl_engine::library::Source::Manual || e.user_exe || e.custom_title.is_some()).collect();
            serde_json::json!({ "app": "BoilerSteamXL", "version": bsxl_engine::VERSION, "preferences": prefs, "games": manual })
        }
    };
    std::fs::write(&path, serde_json::to_string_pretty(&value).map_err(err)?).map_err(err)
}

#[tauri::command(async)]
pub fn import_json(state: State<'_, AppState>, path: String, what: String) -> Result<(), String> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    let mut prefs = Preferences::load();
    if what == "rules" {
        let incoming: RuleSet = serde_json::from_str(&text).map_err(|e| format!("Not a BoilerSteamXL rules file: {e}"))?;
        for rule in &incoming.rules {
            rules::validate(rule.kind, &rule.pattern)?;
        }
        for rule in incoming.rules {
            prefs.rules.rules.retain(|r| r.id != rule.id);
            prefs.rules.rules.push(rule);
        }
        for entry in incoming.allowlist {
            if !prefs.rules.allowlist.contains(&entry) {
                prefs.rules.allowlist.push(entry);
            }
        }
        return save_prefs(prefs).map(|_| ());
    }
    #[derive(Deserialize)]
    struct Export {
        preferences: Preferences,
        #[serde(default)]
        games: Vec<Entry>,
    }
    let incoming: Export = serde_json::from_str(&text).map_err(|e| format!("Not a BoilerSteamXL settings file: {e}"))?;
    let mut new_prefs = incoming.preferences;
    new_prefs.migrate();
    new_prefs.simulation_mode = prefs.simulation_mode; // never switched off by an import
    save_prefs(new_prefs)?;
    with_library(&state, |lib, _| {
        for mut game in incoming.games {
            game.approval = None;
            game.import = None;
            if game.status != Status::Ignored && game.status != Status::Rejected {
                game.status = Status::NeedsReview;
            }
            lib.entries.entry(game.id.clone()).or_insert(game);
        }
        Ok(())
    })
}

// ---- Clean Rebuild ------------------------------------------------------------------

#[tauri::command(async)]
pub fn rebuild_start(state: State<'_, AppState>, mode: RebuildMode, user_id: String) -> Result<(), String> {
    with_library(&state, |lib, _| {
        lib.rebuild_draft = Some(RebuildDraft::new(mode, &user_id));
        Ok(())
    })
}

#[tauri::command]
pub fn rebuild_draft() -> Option<RebuildDraft> {
    Library::load().rebuild_draft
}

#[tauri::command(async)]
pub fn rebuild_inspect() -> Result<Inspection, String> {
    let prefs = Preferences::load();
    let lib = Library::load();
    let draft = lib.rebuild_draft.clone().ok_or("No Clean Rebuild in progress.")?;
    Ok(import::inspect(&lib, &snapshot(&prefs), &prefs, &draft.user_id, draft.mode))
}

#[tauri::command(async)]
pub fn rebuild_set_decision(state: State<'_, AppState>, key: String, decision: Decision) -> Result<(), String> {
    with_library(&state, |lib, _| lib.rebuild_draft.as_mut().ok_or("No Clean Rebuild in progress.")?.set_decision(&key, decision))
}

#[tauri::command(async)]
pub fn rebuild_approve(state: State<'_, AppState>, ids: Vec<String>, approved: bool) -> Result<Vec<(String, String)>, String> {
    with_library(&state, |lib, _| {
        let mut draft = lib.rebuild_draft.clone().ok_or("No Clean Rebuild in progress.")?;
        let mut errors = Vec::new();
        for id in ids {
            if approved {
                if let Err(e) = draft.approve(lib, &id) {
                    errors.push((id, e));
                }
            } else {
                draft.approved.remove(&id);
            }
        }
        lib.rebuild_draft = Some(draft);
        Ok(errors)
    })
}

#[tauri::command(async)]
pub fn rebuild_plan() -> Result<Plan, String> {
    let prefs = Preferences::load();
    import::plan_rebuild(&Library::load(), &snapshot(&prefs), &prefs)
}

#[tauri::command]
pub async fn rebuild_execute(state: State<'_, AppState>, token: String, phrase: String, acknowledged: bool) -> Result<RunRecord, String> {
    let _ = state;
    tauri::async_runtime::spawn_blocking(move || {
        let lock = import::wait_for_lock(LOCK_WAIT)?;
        let prefs = Preferences::load();
        let mut lib = Library::load();
        let snap = snapshot(&prefs);
        let result = import::execute_rebuild(&lock, &mut lib, &snap, &prefs, &token, &phrase, acknowledged, &steam::is_steam_running);
        lib.save().map_err(err)?;
        result
    })
    .await
    .map_err(err)?
}

#[tauri::command(async)]
pub fn rebuild_cancel(state: State<'_, AppState>) -> Result<(), String> {
    with_library(&state, |lib, _| {
        lib.rebuild_draft = None;
        Ok(())
    })
}

// ---- backups, logs, diagnostics -----------------------------------------------------

#[tauri::command(async)]
pub fn list_backups() -> Vec<import::BackupManifest> {
    import::list_backups()
}

#[tauri::command(async)]
pub fn restore_preview(id: String) -> Result<RestorePreview, String> {
    let prefs = Preferences::load();
    import::restore_preview(&id, &snapshot(&prefs), prefs.simulation_mode)
}

#[tauri::command]
pub async fn restore_backup(state: State<'_, AppState>, id: String, token: String) -> Result<RunRecord, String> {
    let _ = state;
    tauri::async_runtime::spawn_blocking(move || {
        let lock = import::wait_for_lock(LOCK_WAIT)?;
        let prefs = Preferences::load();
        let mut lib = Library::load();
        let result = import::restore_backup(&lock, &mut lib, &id, &snapshot(&prefs), &prefs, &token, &steam::is_steam_running);
        lib.save().map_err(err)?;
        result
    })
    .await
    .map_err(err)?
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[tauri::command(async)]
pub fn export_backup(id: String, dest: String) -> Result<String, String> {
    let from = import::backups_dir().join(&id);
    if !from.join("manifest.json").is_file() {
        return Err("Backup not found".into());
    }
    let to = Path::new(&dest).join(format!("BoilerSteamXL-backup-{id}"));
    copy_dir(&from, &to).map_err(err)?;
    Ok(to.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_logs(limit: usize) -> Vec<serde_json::Value> {
    bsxl_engine::log::tail(limit)
}

#[tauri::command(async)]
pub fn diagnostics_report(state: State<'_, AppState>) -> String {
    let prefs = Preferences::load();
    diagnostics::report(&prefs, &Library::load(), &snapshot(&prefs), &state.mode, schedule::is_installed())
}

#[tauri::command]
pub fn schedule_preview(schedule: Schedule) -> Result<Option<String>, String> {
    schedule.validate()?;
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    Ok(schedule::next_run(&schedule, now).and_then(|t| t.format(&time::format_description::well_known::Rfc3339).ok()))
}

/// Opens a folder (or selects a file) in Explorer. The path is passed as an argument,
/// never through a shell.
#[tauri::command]
pub fn reveal(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    let mut cmd = std::process::Command::new("explorer.exe");
    if p.is_file() {
        cmd.arg(format!("/select,{}", p.display()));
    } else if p.is_dir() {
        cmd.arg(p);
    } else if let Some(parent) = p.parent().filter(|d| d.is_dir()) {
        cmd.arg(parent);
    } else {
        return Err(format!("{path} does not exist."));
    }
    cmd.spawn().map(|_| ()).map_err(err)
}

#[tauri::command]
pub fn open_data_folder() -> Result<(), String> {
    reveal(bsxl_engine::data_dir().to_string_lossy().to_string())
}

/// Starts an imported game through Steam (`steam://rungameid/`).
#[tauri::command]
pub fn launch_in_steam(app_id: u32) -> Result<(), String> {
    let id64 = (u64::from(app_id) << 32) | 0x0200_0000;
    std::process::Command::new("explorer.exe").arg(format!("steam://rungameid/{id64}")).spawn().map(|_| ()).map_err(err)
}

#[derive(Serialize)]
pub struct UpdateInfo {
    current: String,
    latest: Option<String>,
    url: Option<String>,
    available: bool,
}

/// Checks the configured release feed (a GitHub "latest release" API URL). Only reports;
/// nothing is downloaded or installed.
#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let prefs = Preferences::load();
    if prefs.update_url.trim().is_empty() {
        return Err("No update address is configured (Settings → Updates).".into());
    }
    let client = reqwest::Client::builder().user_agent("BoilerSteamXL").build().map_err(err)?;
    let value: serde_json::Value = client.get(prefs.update_url.trim()).send().await.map_err(err)?.json().await.map_err(err)?;
    let latest = value["tag_name"].as_str().map(|t| t.trim_start_matches(['v', '.']).to_string());
    let available = latest.as_deref().is_some_and(|l| l != bsxl_engine::VERSION);
    Ok(UpdateInfo { current: bsxl_engine::VERSION.into(), url: value["html_url"].as_str().map(str::to_string), latest, available })
}

/// Read-only look at one account's non-Steam shortcuts (Steam Library screen).
#[tauri::command(async)]
pub fn inspect_account(user_id: String) -> Inspection {
    let prefs = Preferences::load();
    import::inspect(&Library::load(), &snapshot(&prefs), &prefs, &user_id, RebuildMode::AllNonSteam)
}
