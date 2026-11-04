//! Headless command line, used by the scheduled task:
//!
//! ```text
//! BoilerSteamXL.exe --scan [--scheduled]   discover games, update the review queue
//! BoilerSteamXL.exe --sync                 scan, then write games already confirmed for
//!                                          import that waited for Steam to close
//! BoilerSteamXL.exe --dry-run              scan and show what an import would do
//! BoilerSteamXL.exe --status               queue, last scan and schedule
//! ```
//!
//! None of these approve or import a newly discovered game.

use std::sync::atomic::AtomicBool;

use bsxl_engine::{
    import::{self, ExecOptions, OpKind},
    library::{Library, Status},
    portable,
    prefs::Preferences,
    schedule, steam,
};

use crate::commands::{launcher_games, scan_record};

const HELP: &str = "BoilerSteamXL — discover, review and import non-Steam games safely

Usage: BoilerSteamXL.exe [command]

  (no command)   open the app
  --scan         look for new games and update the review queue (never imports)
  --sync         --scan, then import games you already confirmed that were waiting
                 for Steam to close (never imports anything unapproved)
  --dry-run      scan and show what importing the approved games would do; writes nothing
  --status       show the review queue, last scan and schedule
  --scheduled    (with --scan) mark the run as scheduled and send a notification
  --version      print the version
  --help         this text
";

pub fn is_cli(args: &[String]) -> bool {
    args.iter().any(|a| {
        matches!(a.as_str(), "--scan" | "--sync" | "--dry-run" | "--status" | "--help" | "-h" | "--version" | "/?")
    })
}

/// Portable edition: point the data folder next to the executable. Returns the edition.
pub fn setup_data_dir() -> String {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let fallback = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let Some(exe_dir) = exe_dir else { return "installed".into() };
    let location = portable::resolve(&exe_dir, &fallback);
    if let Some(dir) = &location.dir {
        // Single-threaded here: nothing else reads the environment yet.
        std::env::set_var("BOILERSTEAMXL_HOME", dir);
    }
    match location.mode {
        portable::Mode::Installed => "installed",
        portable::Mode::Portable => "portable",
        portable::Mode::PortableFallback => "portable (read-only folder; data in %LOCALAPPDATA%\\BoilerSteamXL-portable)",
    }
    .into()
}

#[cfg(windows)]
fn attach_console() {
    // A GUI-subsystem program has no console; borrow the terminal it was started from.
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    // SAFETY: AttachConsole has no memory-safety preconditions.
    unsafe {
        AttachConsole(u32::MAX);
    }
}

#[cfg(not(windows))]
fn attach_console() {}

pub fn run(args: &[String], mode: &str) -> i32 {
    attach_console();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--help") || has("-h") || has("/?") {
        println!("{HELP}");
        return 0;
    }
    if has("--version") {
        println!("BoilerSteamXL {}", bsxl_engine::VERSION);
        return 0;
    }
    if has("--status") {
        return status(mode);
    }
    let scheduled = has("--scheduled");
    let dry_run = has("--dry-run");

    let prefs = Preferences::load();
    if prefs.scan_roots.is_empty() {
        println!("No game folders configured yet. Open BoilerSteamXL to set them up.");
        return 2;
    }
    // Scan without holding the lock (the app stays usable meanwhile), then merge into
    // the library as it is at that moment, under the lock.
    let launchers = launcher_games(&prefs);
    let discovery = bsxl_engine::discover(&prefs, &|_| {}, &AtomicBool::new(false));

    if dry_run {
        // Merged into a throw-away copy: nothing is saved and nothing is written.
        let mut lib = Library::load();
        let summary = bsxl_engine::merge(&mut lib, discovery, launchers.as_deref(), &prefs);
        print_summary(&summary);
        let plan = import::plan_import(&lib, &steam::snapshot(&steam::detect_steam_path(prefs.steam_path.as_deref())), &prefs, None);
        println!("
Dry run (nothing was written):");
        print_plan(&plan);
        return 0;
    }

    let lock = match import::wait_for_lock(std::time::Duration::from_secs(120)) {
        Ok(lock) => lock,
        Err(e) => {
            println!("{e}");
            bsxl_engine::log::warn("cli", &e);
            return 3;
        }
    };
    let mut lib = Library::load();
    let summary = bsxl_engine::merge(&mut lib, discovery, launchers.as_deref(), &prefs);
    print_summary(&summary);
    if scheduled && prefs.notifications && !summary.newly_discovered.is_empty() {
        notify(summary.newly_discovered.len());
    }
    lib.mark_notified(&summary.newly_discovered);
    lib.record(scan_record(&summary, scheduled));

    let mut code = 0;
    if has("--sync") {
        code = sync_confirmed(&lock, &mut lib, &prefs);
    }
    if let Err(e) = lib.save() {
        println!("Could not save the library: {e}");
        return 1;
    }
    code
}

fn print_summary(summary: &bsxl_engine::library::ScanSummary) {
    println!(
        "Library refreshed: {} folders scanned, {} games found, {} new, {} need review, {} executables ignored, {} missing.",
        summary.folders_scanned, summary.games_found, summary.new_games, summary.needs_review, summary.exes_excluded, summary.missing
    );
    for (root, e) in &summary.root_errors {
        println!("  Folder problem: {root}: {e}");
    }
}

/// Writes only games the user already confirmed in the final import step, which were
/// deferred because Steam was running.
fn sync_confirmed(lock: &import::OpLock, lib: &mut Library, prefs: &Preferences) -> i32 {
    let ready: Vec<String> = lib.entries.values().filter(|e| e.status == Status::ReadyToImport).map(|e| e.id.clone()).collect();
    if ready.is_empty() {
        println!("No confirmed imports are waiting.");
        return 0;
    }
    if steam::is_steam_running() {
        println!("{} confirmed game(s) are waiting, but Steam is running; they stay queued.", ready.len());
        return 0;
    }
    let snap = steam::snapshot(&steam::detect_steam_path(prefs.steam_path.as_deref()));
    let plan = import::plan_import(lib, &snap, prefs, Some(&ready));
    let options = ExecOptions { steam_running: &steam::is_steam_running, defer_if_running: true, scheduled: true, only: Some(ready) };
    match import::execute_import(lock, lib, &snap, prefs, &plan.token, &options) {
        Ok(outcome) => {
            println!("{}", outcome.record.details.join("\n"));
            0
        }
        Err(e) => {
            println!("Import failed: {e}");
            1
        }
    }
}

fn print_plan(plan: &import::Plan) {
    for op in &plan.ops {
        if matches!(op.kind, OpKind::Keep | OpKind::Unresolved) {
            continue;
        }
        let new = op.new.as_ref().map_or(String::new(), |n| format!("{} ({})", n.name, n.exe));
        println!("  {:?} for account {}: {new}", op.kind, op.user_id);
    }
    for b in &plan.blocked {
        println!("  Blocked: {} — {}", b.title, b.reason);
    }
    for w in &plan.warnings {
        println!("  Note: {w}");
    }
    println!(
        "  {} to create, {} to update, {} to remove, {} duplicates created, {} still need review.",
        plan.summary.get("create").unwrap_or(&0),
        plan.summary.get("update").unwrap_or(&0),
        plan.summary.get("remove").unwrap_or(&0),
        plan.summary.get("duplicates_created").unwrap_or(&0),
        plan.summary.get("still_review").unwrap_or(&0),
    );
    if plan.simulation {
        println!("  Simulation mode is on: even a confirmed import would not write to Steam.");
    }
}

fn status(mode: &str) -> i32 {
    let prefs = Preferences::load();
    let lib = Library::load();
    println!("BoilerSteamXL {} ({mode})", bsxl_engine::VERSION);
    println!("Data folder: {}", bsxl_engine::data_dir().display());
    println!("Simulation mode: {}", if prefs.simulation_mode { "on (no Steam writes)" } else { "off" });
    println!("Game folders: {}", prefs.scan_roots.iter().map(|r| r.path.as_str()).collect::<Vec<_>>().join(", "));
    match &lib.last_scan {
        Some(s) => println!("Last scan: {} — {} games, {} new, {} need review", s.at, s.games_found, s.new_games, s.needs_review),
        None => println!("Last scan: never"),
    }
    let mut counts = std::collections::BTreeMap::new();
    for e in lib.entries.values() {
        *counts.entry(e.status.label()).or_insert(0) += 1;
    }
    for (label, n) in counts {
        println!("  {label}: {n}");
    }
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    println!(
        "Schedule: {:?} at {} (task registered: {}); next run: {}",
        prefs.schedule.mode,
        prefs.schedule.time,
        schedule::is_installed(),
        schedule::next_run(&prefs.schedule, now).map_or("—".to_string(), |t| t.to_string())
    );
    0
}

#[cfg(windows)]
fn notify(count: usize) {
    use tauri_winrt_notification::Toast;
    let title = if count == 1 { "BoilerSteamXL — 1 new game discovered".to_string() } else { format!("BoilerSteamXL — {count} new games discovered") };
    // The installer registers the app id on its Start-menu shortcut; the portable
    // edition falls back to a generic sender.
    let shown = Toast::new("io.github.boilersteamxl").title(&title).text1("Open BoilerSteamXL to review and approve them.").show();
    if shown.is_err() {
        let _ = Toast::new(Toast::POWERSHELL_APP_ID).title(&title).text1("Open BoilerSteamXL to review and approve them.").show();
    }
}

#[cfg(not(windows))]
fn notify(count: usize) {
    println!("{count} new game(s) discovered; open BoilerSteamXL to review them.");
}
