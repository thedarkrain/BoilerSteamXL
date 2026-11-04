//! The review queue and import history (`library.json`).
//!
//! Every game BoilerSteamXL knows about is an [`Entry`] with a [`Status`]. Scans add and
//! update entries but never undo a user decision: approvals, rejections, ignores, chosen
//! executables, titles and imports survive every refresh. An approval is bound to the
//! executable and launch options it was given for; if either changes, the approval is
//! dropped and the game goes back to review.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    atomic_write, data_dir, fnv64, norm_name, norm_path, now_rfc3339, read_json,
    rules::{Rule, RuleKind},
    scan::{Candidate, ExcludedExe, ScanResult},
    steam::{SteamShortcut, SteamSnapshot},
};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    NeedsReview,
    Approved,
    Rejected,
    Ignored,
    PossibleDuplicate,
    /// Linked to an existing Steam entry (detected or chosen by the user).
    AlreadyInSteam,
    /// Confirmed for import, but Steam was running; written by the next `--sync`.
    ReadyToImport,
    Importing,
    Imported,
    ImportFailed,
    Missing,
    /// Imported, but the game's executable changed; an update needs approval.
    UpdateAvailable,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::NeedsReview => "Needs review",
            Status::Approved => "Approved",
            Status::Rejected => "Rejected",
            Status::Ignored => "Ignored",
            Status::PossibleDuplicate => "Possible duplicate",
            Status::AlreadyInSteam => "Already in Steam",
            Status::ReadyToImport => "Ready to import",
            Status::Importing => "Importing",
            Status::Imported => "Imported",
            Status::ImportFailed => "Import failed",
            Status::Missing => "Missing",
            Status::UpdateAvailable => "Update available",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Folder,
    Manual,
    Launcher,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    pub fingerprint: String,
    pub at: String,
    /// The user saw a duplicate warning and chose to add a separate shortcut anyway.
    #[serde(default)]
    pub allow_duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DupMatch {
    /// "shortcut", "official" or "entry".
    pub kind: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub app_id: Option<u32>,
    #[serde(default)]
    pub entry_id: Option<String>,
    #[serde(default)]
    pub boilr: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateInfo {
    /// "high", "medium" or "low".
    pub confidence: String,
    pub reason: String,
    pub matched: DupMatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportRecord {
    pub app_id: u32,
    pub identity: String,
    pub user_ids: Vec<String>,
    pub at: String,
    pub name: String,
    pub exe: String,
    pub launch_options: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub source: Source,
    #[serde(default)]
    pub launcher: Option<String>,
    pub title: String,
    #[serde(default)]
    pub title_source: String,
    #[serde(default)]
    pub custom_title: Option<String>,
    /// The name of yours last written to this game's Steam shortcut. A rename reaches Steam
    /// once; a later rename made in Steam itself is left alone.
    #[serde(default)]
    pub synced_title: Option<String>,
    pub game_dir: String,
    #[serde(default)]
    pub root: Option<String>,
    /// The executable that will be imported; empty when none is chosen yet.
    pub exe: String,
    /// The user picked `exe`; scans never replace it.
    #[serde(default)]
    pub user_exe: bool,
    pub start_dir: String,
    #[serde(default)]
    pub user_start_dir: bool,
    #[serde(default)]
    pub launch_options: String,
    /// The user set `launch_options`; scans never replace them.
    #[serde(default)]
    pub user_launch_options: bool,
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    #[serde(default)]
    pub excluded: Vec<ExcludedExe>,
    #[serde(default)]
    pub confidence: u8,
    #[serde(default)]
    pub uncertain: bool,
    pub status: Status,
    /// Status before the game went missing, restored when it is found again.
    #[serde(default)]
    pub prev_status: Option<Status>,
    #[serde(default)]
    pub approval: Option<Approval>,
    #[serde(default)]
    pub duplicate: Option<DuplicateInfo>,
    #[serde(default)]
    pub import: Option<ImportRecord>,
    #[serde(default)]
    pub linked: Option<DupMatch>,
    #[serde(default)]
    pub user_linked: bool,
    #[serde(default)]
    pub ignore_rule: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub collection: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub artwork_game_id: Option<u32>,
    #[serde(default)]
    pub note: Option<String>,
}

impl Entry {
    pub fn display_title(&self) -> &str {
        self.custom_title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(&self.title)
    }

    /// What an approval is bound to: the executable and the launch command.
    pub fn fingerprint(&self) -> String {
        format!("{}\u{0}{}", norm_path(&self.exe), self.launch_options.trim())
    }

    pub fn approval_valid(&self) -> bool {
        self.approval
            .as_ref()
            .is_some_and(|a| a.fingerprint == self.fingerprint())
    }

    fn invalidate_approval(&mut self, why: &str) {
        if self.approval.take().is_some() {
            self.note = Some(why.to_string());
        }
        if matches!(self.status, Status::Approved | Status::ReadyToImport) {
            self.status = Status::NeedsReview;
        }
    }

    fn set_exe_from_candidates(&mut self) {
        if let Some(best) = self.candidates.first() {
            self.exe = best.path.clone();
            if !self.user_launch_options {
                self.launch_options = best.arguments.clone().unwrap_or_default();
            }
        } else {
            self.exe.clear();
        }
        if !self.user_start_dir {
            self.start_dir = parent_dir(&self.exe).unwrap_or_else(|| self.game_dir.clone());
        }
    }
}

pub fn parent_dir(path: &str) -> Option<String> {
    Path::new(path).parent().map(|p| p.to_string_lossy().to_string()).filter(|p| !p.is_empty())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    /// "scan", "import", "rebuild" or "restore".
    pub kind: String,
    pub at: String,
    #[serde(default)]
    pub simulated: bool,
    #[serde(default)]
    pub scheduled: bool,
    #[serde(default)]
    pub user_ids: Vec<String>,
    #[serde(default)]
    pub counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub details: Vec<String>,
    #[serde(default)]
    pub backup: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanSummary {
    pub at: String,
    pub folders_scanned: usize,
    pub exes_found: usize,
    pub exes_excluded: usize,
    pub games_found: usize,
    pub new_games: usize,
    pub already_in_steam: usize,
    pub needs_review: usize,
    pub missing: usize,
    pub duplicates: usize,
    pub roots_scanned: Vec<String>,
    pub root_errors: Vec<(String, String)>,
    pub cancelled: bool,
    pub duration_ms: u128,
    /// Games new in this scan that the user has not been told about yet.
    #[serde(default)]
    pub newly_discovered: Vec<String>,
    /// Excluded executables that belong to no game.
    #[serde(default)]
    pub orphan_excluded: Vec<ExcludedExe>,
}

/// A game reported by one of BoilR's launcher integrations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LauncherGame {
    pub launcher_code: String,
    pub launcher_name: String,
    pub key: String,
    pub title: String,
    pub exe: String,
    pub start_dir: String,
    pub launch_options: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub entries: BTreeMap<String, Entry>,
    #[serde(default)]
    pub history: Vec<RunRecord>,
    /// Entry ids already announced in a notification.
    #[serde(default)]
    pub notified: Vec<String>,
    #[serde(default)]
    pub last_scan: Option<ScanSummary>,
    #[serde(default)]
    pub rebuild_draft: Option<crate::import::RebuildDraft>,
    /// Steam shortcuts the user asked to remove; done by the next confirmed sync.
    #[serde(default)]
    pub removals: Vec<Removal>,
}

/// A shortcut to remove from Steam. It is found again by account, app id, target and name,
/// since its position in shortcuts.vdf can change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Removal {
    pub user_id: String,
    pub app_id: u32,
    pub name: String,
    pub exe: String,
    pub index: usize,
}

impl Removal {
    pub fn of(s: &SteamShortcut) -> Self {
        Removal { user_id: s.user_id.clone(), app_id: s.app_id, name: s.name.clone(), exe: s.exe_path(), index: s.index }
    }

    pub fn matches(&self, s: &SteamShortcut) -> bool {
        s.user_id == self.user_id && s.app_id == self.app_id && s.name == self.name && norm_path(&s.exe_path()) == norm_path(&self.exe)
    }
}

pub fn library_file() -> std::path::PathBuf {
    data_dir().join("library.json")
}

/// Fields the user can edit on an entry (also used by "+ Add Game").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EntryEdit {
    pub title: Option<String>,
    pub exe: Option<String>,
    pub start_dir: Option<String>,
    pub launch_options: Option<String>,
    pub tags: Option<Vec<String>>,
    pub collection: Option<String>,
    pub notes: Option<String>,
    pub icon: Option<String>,
    pub artwork_game_id: Option<u32>,
}

const MAX_HISTORY: usize = 200;

impl Library {
    pub fn load() -> Library {
        let mut lib: Library = read_json(&library_file()).unwrap_or_default();
        lib.schema_version = SCHEMA_VERSION;
        lib
    }

    pub fn save(&self) -> eyre::Result<()> {
        atomic_write(&library_file(), serde_json::to_string_pretty(self)?.as_bytes())?;
        Ok(())
    }

    pub fn record(&mut self, record: RunRecord) {
        self.history.insert(0, record);
        self.history.truncate(MAX_HISTORY);
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    pub fn new_entry(id: String, source: Source, now: &str) -> Entry {
        Entry {
            id,
            source,
            launcher: None,
            title: String::new(),
            title_source: String::new(),
            custom_title: None,
            synced_title: None,
            game_dir: String::new(),
            root: None,
            exe: String::new(),
            user_exe: false,
            start_dir: String::new(),
            user_start_dir: false,
            launch_options: String::new(),
            user_launch_options: false,
            candidates: Vec::new(),
            excluded: Vec::new(),
            confidence: 0,
            uncertain: false,
            status: Status::NeedsReview,
            prev_status: None,
            approval: None,
            duplicate: None,
            import: None,
            linked: None,
            user_linked: false,
            ignore_rule: None,
            first_seen: now.to_string(),
            last_seen: now.to_string(),
            tags: Vec::new(),
            collection: String::new(),
            notes: String::new(),
            icon: String::new(),
            artwork_game_id: None,
            note: None,
        }
    }

    /// Merges a scan into the queue. `launchers` is `None` when launcher integrations
    /// were not queried (their entries are then left alone).
    pub fn apply_scan(
        &mut self,
        scan: &ScanResult,
        launchers: Option<&[LauncherGame]>,
        snapshot: &SteamSnapshot,
    ) -> ScanSummary {
        let now = now_rfc3339();
        let mut seen: HashSet<String> = HashSet::new();
        let mut created: Vec<String> = Vec::new();

        // Folder games that are gone, but only under roots that were actually scanned:
        // an unplugged drive must not make a whole library "missing". Done first, so a
        // game moved to another folder is recognized as moved in this same scan.
        let found: HashSet<String> = scan.games.iter().map(|g| format!("fs:{}", norm_path(&g.game_dir))).collect();
        if !scan.cancelled {
            let roots: Vec<String> = scan.roots_scanned.iter().map(|r| norm_path(r)).collect();
            for entry in self.entries.values_mut() {
                // Ignored games are excluded by their rule, so not finding them is expected.
                if entry.source != Source::Folder || found.contains(&entry.id) || entry.status == Status::Ignored {
                    continue;
                }
                let dir = norm_path(&entry.game_dir);
                if roots.iter().any(|r| dir == *r || dir.starts_with(&format!("{r}/"))) {
                    mark_missing(entry);
                }
            }
        }

        for game in &scan.games {
            let id = format!("fs:{}", norm_path(&game.game_dir));
            seen.insert(id.clone());
            if !self.entries.contains_key(&id) {
                if let Some(old_id) = self.find_relocated(game, &found) {
                    if let Some(mut old) = self.entries.remove(&old_id) {
                        old.id = id.clone();
                        old.status = old.prev_status.take().unwrap_or(Status::NeedsReview);
                        old.note = Some(format!("Moved from {}", old.game_dir));
                        old.user_exe = false;
                        old.user_start_dir = false;
                        self.entries.insert(id.clone(), old);
                    }
                } else {
                    let mut entry = Self::new_entry(id.clone(), Source::Folder, &now);
                    entry.game_dir = game.game_dir.clone();
                    self.entries.insert(id.clone(), entry);
                    created.push(id.clone());
                }
            }
            let Some(entry) = self.entries.get_mut(&id) else { continue };
            entry.title = game.title.clone();
            entry.title_source = game.title_source.clone();
            entry.game_dir = game.game_dir.clone();
            entry.root = Some(game.root.clone());
            entry.candidates = game.candidates.clone();
            entry.excluded = game.excluded.clone();
            entry.confidence = game.confidence;
            entry.uncertain = game.uncertain;
            entry.last_seen = now.clone();
            if entry.status == Status::Missing {
                entry.status = entry.prev_status.take().unwrap_or(Status::NeedsReview);
            }
            Self::refresh_exe(entry);
        }
        if !scan.cancelled {
            self.absorb_regrouped(scan, &created);
        }

        if let Some(launchers) = launchers {
            let codes: HashSet<&str> = launchers.iter().map(|g| g.launcher_code.as_str()).collect();
            for game in launchers {
                let id = format!("launcher:{}:{}", game.launcher_code, game.key);
                seen.insert(id.clone());
                if !self.entries.contains_key(&id) {
                    let mut entry = Self::new_entry(id.clone(), Source::Launcher, &now);
                    entry.launcher = Some(game.launcher_name.clone());
                    self.entries.insert(id.clone(), entry);
                    created.push(id.clone());
                }
                let Some(entry) = self.entries.get_mut(&id) else { continue };
                entry.title = game.title.clone();
                entry.title_source = format!("{} metadata", game.launcher_name);
                entry.game_dir = game.start_dir.clone();
                entry.candidates = vec![Candidate {
                    path: game.exe.clone(),
                    file_name: Path::new(&game.exe).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    score: 95,
                    size: 0,
                    reasons: vec![format!("+95 reported by {}", game.launcher_name)],
                    description: None,
                    arguments: Some(game.launch_options.clone()),
                }];
                entry.confidence = 95;
                entry.uncertain = false;
                entry.last_seen = now.clone();
                if entry.status == Status::Missing {
                    entry.status = entry.prev_status.take().unwrap_or(Status::NeedsReview);
                }
                if !entry.user_start_dir {
                    entry.start_dir = game.start_dir.clone();
                }
                Self::refresh_exe(entry);
            }
            // Launcher games that disappeared from a launcher that answered.
            for entry in self.entries.values_mut() {
                if entry.source == Source::Launcher && !seen.contains(&entry.id) {
                    let code = entry.id.split(':').nth(1).unwrap_or_default();
                    if codes.contains(code) {
                        mark_missing(entry);
                    }
                }
            }
        }

        // Manual games: only the file's existence matters.
        for entry in self.entries.values_mut() {
            if entry.source != Source::Manual {
                continue;
            }
            let exists = Path::new(&entry.exe).exists();
            if !exists {
                mark_missing(entry);
            } else if entry.status == Status::Missing {
                entry.status = entry.prev_status.take().unwrap_or(Status::NeedsReview);
            }
        }

        self.reconcile(snapshot);

        let notified: HashSet<String> = self.notified.iter().cloned().collect();
        let newly: Vec<String> = created
            .iter()
            .filter(|id| {
                !notified.contains(*id)
                    && self.entries.get(*id).is_some_and(|e| {
                        matches!(e.status, Status::NeedsReview | Status::PossibleDuplicate)
                    })
            })
            .cloned()
            .collect();

        let count = |s: Status| self.entries.values().filter(|e| e.status == s).count();
        let summary = ScanSummary {
            at: now,
            folders_scanned: scan.folders_scanned,
            exes_found: scan.exes_found,
            exes_excluded: scan.exes_excluded,
            games_found: scan.games.len() + launchers.map_or(0, <[LauncherGame]>::len),
            new_games: created.len(),
            already_in_steam: count(Status::AlreadyInSteam) + count(Status::Imported),
            needs_review: count(Status::NeedsReview) + count(Status::PossibleDuplicate) + count(Status::UpdateAvailable),
            missing: count(Status::Missing),
            duplicates: count(Status::PossibleDuplicate),
            roots_scanned: scan.roots_scanned.clone(),
            root_errors: scan.root_errors.clone(),
            cancelled: scan.cancelled,
            duration_ms: scan.duration_ms,
            newly_discovered: newly,
            orphan_excluded: Vec::new(),
        };
        self.last_scan = Some(summary.clone());
        summary
    }

    /// A rescan can group folders differently: a crack or NoDVD sub-folder found as its own
    /// game before now belongs to the game folder around it. That old entry is not a missing
    /// game; its decisions (your name, a Steam link, "don't add") move to the game around it
    /// and it goes away, so the same game is never shown twice.
    fn absorb_regrouped(&mut self, scan: &ScanResult, created: &[String]) {
        let dirs: Vec<String> = scan.games.iter().map(|g| norm_path(&g.game_dir)).collect();
        let stale: Vec<(String, String)> = self
            .entries
            .values()
            .filter(|e| e.source == Source::Folder && e.status == Status::Missing)
            .filter_map(|e| {
                let dir = norm_path(&e.game_dir);
                let outer = dirs.iter().filter(|d| dir.starts_with(&format!("{d}/"))).max_by_key(|d| d.len())?;
                Some((e.id.clone(), format!("fs:{outer}")))
            })
            .collect();
        for (old_id, outer_id) in stale {
            let Some(old) = self.entries.get(&old_id).cloned() else { continue };
            let Some(outer) = self.entries.get_mut(&outer_id) else { continue };
            // Both have their own shortcut in Steam: keep both and let the user merge them.
            if old.import.is_some() && outer.import.is_some() {
                continue;
            }
            if outer.custom_title.is_none() {
                outer.custom_title = old.custom_title.clone();
                outer.synced_title = old.synced_title.clone();
            }
            if outer.import.is_none() && old.import.is_some() {
                outer.import = old.import.clone();
                outer.status = Status::Imported;
            } else if old.user_linked && outer.linked.is_none() && outer.import.is_none() {
                outer.linked = old.linked.clone();
                outer.user_linked = true;
                outer.status = Status::AlreadyInSteam;
            } else if created.contains(&outer_id)
                && outer.import.is_none()
                && matches!(old.prev_status, Some(Status::Rejected))
                && matches!(outer.status, Status::NeedsReview | Status::PossibleDuplicate)
            {
                outer.status = Status::Rejected;
            }
            self.entries.remove(&old_id);
        }
    }

    /// Marks the given ids as announced so the next scan does not notify again.
    pub fn mark_notified(&mut self, ids: &[String]) {
        for id in ids {
            if !self.notified.contains(id) {
                self.notified.push(id.clone());
            }
        }
    }

    /// Updates the executable after a scan without overriding the user's choice.
    fn refresh_exe(entry: &mut Entry) {
        if entry.user_exe {
            if !Path::new(&entry.exe).exists() {
                mark_missing(entry);
            }
            return;
        }
        let before = entry.fingerprint();
        entry.set_exe_from_candidates();
        if entry.fingerprint() != before {
            if entry.approval.is_some() {
                entry.invalidate_approval("The detected executable changed since you approved it.");
            }
        }
    }

    /// A missing folder game with the same main executable (name and size) as a newly
    /// found folder: the game was moved, so its decisions move with it.
    fn find_relocated(&self, game: &crate::scan::DiscoveredGame, found: &HashSet<String>) -> Option<String> {
        let best = game.candidates.first()?;
        self.entries
            .values()
            .find(|e| {
                e.source == Source::Folder
                    && e.status == Status::Missing
                    && !found.contains(&e.id)
                    && norm_name(&e.title) == norm_name(&game.title)
                    && e.candidates.iter().any(|c| {
                        c.file_name.eq_ignore_ascii_case(&best.file_name) && c.size == best.size && c.size > 0
                    })
            })
            .map(|e| e.id.clone())
    }

    /// Re-checks every entry against Steam: imported games still present, duplicates.
    pub fn reconcile(&mut self, snapshot: &SteamSnapshot) {
        let ids: Vec<String> = self.entries.keys().cloned().collect();
        for id in ids {
            let Some(entry) = self.entries.get_mut(&id) else { continue };
            if matches!(entry.status, Status::Ignored | Status::Rejected | Status::Missing) {
                continue;
            }

            if let Some(record) = entry.import.clone() {
                // Only conclude anything when every account it was written to could be read:
                // a damaged or missing file must not erase import records.
                let present: Vec<&crate::steam::SteamUser> = record.user_ids.iter().filter_map(|u| snapshot.user(u)).collect();
                let readable = snapshot.valid && !present.is_empty() && present.iter().all(|user| user.error.is_none());
                if !readable {
                    continue;
                }
                let found = snapshot
                    .all_shortcuts()
                    .find(|s| s.identity == record.identity || (s.app_id == record.app_id && norm_path(&s.exe) == norm_path(&record.exe)));
                match found {
                    Some(shortcut) => {
                        let steam_has_it = norm_path(&shortcut.exe) == norm_path(&entry.exe)
                            && shortcut.launch_options.trim() == entry.launch_options.trim();
                        if steam_has_it && (record.exe != entry.exe || record.launch_options != entry.launch_options) {
                            if let Some(r) = entry.import.as_mut() {
                                r.exe = entry.exe.clone();
                                r.launch_options = entry.launch_options.clone();
                            }
                            entry.approval = None;
                            entry.status = Status::Imported;
                            continue;
                        }
                        // Compared with what BoilerSteamXL last wrote, so edits made in Steam
                        // itself are left alone; only changes on our side offer an update.
                        // A new name is not an update to approve: the next sync renames it.
                        let changed = !entry.exe.is_empty()
                            && (norm_path(&record.exe) != norm_path(&entry.exe)
                                || record.launch_options.trim() != entry.launch_options.trim());
                        if changed {
                            if entry.status != Status::Approved || !entry.approval_valid() {
                                entry.status = Status::UpdateAvailable;
                            }
                        } else if !matches!(entry.status, Status::Approved | Status::ReadyToImport) {
                            entry.status = Status::Imported;
                        }
                    }
                    None => {
                        // The user deleted it in Steam: respect that, ask before re-adding.
                        entry.import = None;
                        entry.approval = None;
                        entry.status = Status::NeedsReview;
                        entry.note = Some("This game's shortcut was removed from Steam.".into());
                    }
                }
                continue;
            }

            if !snapshot.valid || snapshot.users.iter().any(|u| u.error.is_some()) {
                continue;
            }
            if entry.exe.is_empty() {
                entry.duplicate = None;
                if entry.status != Status::NeedsReview {
                    entry.status = Status::NeedsReview;
                }
                continue;
            }

            // Compared with the other entries as they are now (no copies: this runs on every edit).
            let probe = entry.clone();
            let duplicate = detect_duplicate(&probe, snapshot, self.entries.values().filter(|e| e.id != id));
            let Some(entry) = self.entries.get_mut(&id) else { continue };
            let allowed = entry.approval.as_ref().is_some_and(|a| a.allow_duplicate) && entry.approval_valid();
            entry.duplicate = duplicate.clone();
            if entry.user_linked {
                continue;
            }
            match duplicate {
                Some(dup) if !allowed => {
                    entry.approval = None;
                    if dup.confidence == "high" {
                        entry.status = Status::AlreadyInSteam;
                        entry.linked = Some(dup.matched);
                    } else {
                        entry.status = Status::PossibleDuplicate;
                    }
                }
                _ => {
                    if matches!(entry.status, Status::AlreadyInSteam | Status::PossibleDuplicate) {
                        entry.status = Status::NeedsReview;
                        entry.linked = None;
                    }
                    if matches!(entry.status, Status::Approved | Status::ReadyToImport) && !entry.approval_valid() {
                        entry.invalidate_approval("The executable or launch options changed since you approved it.");
                    }
                }
            }
        }
    }

    // ---- review actions -------------------------------------------------------------

    /// Approves entries. A possible duplicate is only approved with `allow_duplicate`,
    /// which the UI sends after an explicit "add a separate shortcut anyway".
    pub fn approve(&mut self, ids: &[String], allow_duplicate: bool) -> Vec<(String, String)> {
        let mut errors = Vec::new();
        let now = now_rfc3339();
        for id in ids {
            let Some(entry) = self.entries.get_mut(id) else {
                errors.push((id.clone(), "Unknown game".into()));
                continue;
            };
            let problem = if entry.exe.is_empty() {
                Some("Choose an executable first.")
            } else if !Path::new(&entry.exe).is_file() {
                Some("The executable no longer exists.")
            } else if matches!(entry.status, Status::Ignored | Status::Rejected | Status::Missing | Status::Imported | Status::AlreadyInSteam) {
                Some("This game is not waiting for approval.")
            } else if entry.status == Status::PossibleDuplicate && !allow_duplicate {
                Some("This may be a duplicate; confirm that you want a separate shortcut.")
            } else {
                None
            };
            if let Some(problem) = problem {
                errors.push((id.clone(), problem.into()));
                continue;
            }
            entry.approval = Some(Approval {
                fingerprint: entry.fingerprint(),
                at: now.clone(),
                allow_duplicate: allow_duplicate || entry.status == Status::PossibleDuplicate,
            });
            entry.status = Status::Approved;
            entry.note = None;
        }
        errors
    }

    /// Only clear-cut games: a confident executable, no duplicate warning.
    pub fn eligible_for_bulk_approval(&self) -> Vec<String> {
        self.entries
            .values()
            .filter(|e| {
                e.status == Status::NeedsReview
                    && !e.uncertain
                    && e.duplicate.is_none()
                    && !e.exe.is_empty()
                    && Path::new(&e.exe).is_file()
            })
            .map(|e| e.id.clone())
            .collect()
    }

    pub fn reject(&mut self, ids: &[String]) {
        for id in ids {
            if let Some(entry) = self.entries.get_mut(id) {
                if entry.import.is_none() {
                    entry.approval = None;
                    entry.status = Status::Rejected;
                }
            }
        }
    }

    /// Back to review from Rejected/Ignored/linked. Returns rule ids to delete.
    pub fn restore(&mut self, ids: &[String]) -> Vec<String> {
        let mut rules = Vec::new();
        for id in ids {
            if let Some(entry) = self.entries.get_mut(id) {
                if let Some(rule) = entry.ignore_rule.take() {
                    rules.push(rule);
                }
                entry.user_linked = false;
                entry.linked = None;
                entry.approval = None;
                entry.status = if entry.import.is_some() {
                    if matches!(entry.status, Status::Approved | Status::ReadyToImport) {
                        Status::UpdateAvailable
                    } else {
                        entry.status
                    }
                } else {
                    Status::NeedsReview
                };
            }
        }
        rules
    }

    /// The library game a Steam shortcut belongs to: imported by BoilerSteamXL or linked.
    pub fn entry_for_shortcut(&self, s: &SteamShortcut) -> Option<&Entry> {
        self.entries.values().find(|e| {
            e.import.as_ref().is_some_and(|r| !s.identity.is_empty() && r.identity == s.identity && r.user_ids.contains(&s.user_id))
                || e.linked.as_ref().is_some_and(|l| l.kind == "shortcut" && l.user_id.as_deref() == Some(s.user_id.as_str()) && l.app_id == Some(s.app_id))
        })
    }

    /// The name the user gave a game that Steam should get at the next sync, if any.
    pub fn pending_rename(entry: &Entry) -> Option<&str> {
        let title = entry.custom_title.as_deref().map(str::trim).filter(|t| !t.is_empty())?;
        (entry.synced_title.as_deref() != Some(title)).then_some(title)
    }

    /// Asks for a shortcut to be removed by the next confirmed sync (`remove: false` takes it back).
    pub fn request_removal(&mut self, s: &SteamShortcut, remove: bool) {
        self.removals.retain(|r| !r.matches(s));
        if remove {
            self.removals.push(Removal::of(s));
        }
    }

    /// Ignores permanently: returns the rules to add so later scans skip the game.
    pub fn ignore(&mut self, ids: &[String]) -> Vec<Rule> {
        let mut rules = Vec::new();
        for id in ids {
            let Some(entry) = self.entries.get_mut(id) else { continue };
            if entry.import.is_some() {
                continue;
            }
            entry.approval = None;
            entry.status = Status::Ignored;
            if entry.source == Source::Folder {
                let rule_id = format!("u-ignore-{:x}", fnv64(&entry.id));
                rules.push(Rule {
                    id: rule_id.clone(),
                    kind: RuleKind::DirPath,
                    pattern: entry.game_dir.clone(),
                    enabled: true,
                    builtin: false,
                    inside_game_only: false,
                    note: format!("Ignored game: {}", entry.display_title()),
                });
                entry.ignore_rule = Some(rule_id);
            }
        }
        rules
    }

    pub fn edit(&mut self, id: &str, edit: &EntryEdit) -> Result<(), String> {
        let entry = self.entries.get_mut(id).ok_or("Unknown game")?;
        let before = entry.fingerprint();
        if let Some(title) = &edit.title {
            let trimmed = title.trim();
            entry.custom_title = (!trimmed.is_empty() && trimmed != entry.title).then(|| trimmed.to_string());
        }
        if let Some(exe) = &edit.exe {
            let exe = exe.trim().trim_matches('"');
            validate_exe(exe)?;
            if norm_path(exe) != norm_path(&entry.exe) {
                entry.exe = exe.to_string();
                entry.user_exe = true;
                if edit.start_dir.is_none() && !entry.user_start_dir {
                    entry.start_dir = parent_dir(exe).unwrap_or_default();
                }
                if entry.status == Status::Missing {
                    entry.status = entry.prev_status.take().unwrap_or(Status::NeedsReview);
                }
            }
        }
        if let Some(dir) = &edit.start_dir {
            entry.start_dir = dir.trim().trim_matches('"').to_string();
            entry.user_start_dir = true;
        }
        if let Some(args) = &edit.launch_options {
            if args.trim() != entry.launch_options.trim() {
                entry.launch_options = args.trim().to_string();
                entry.user_launch_options = true;
            }
        }
        if let Some(tags) = &edit.tags {
            entry.tags = tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        }
        if let Some(collection) = &edit.collection {
            entry.collection = collection.trim().to_string();
        }
        if let Some(notes) = &edit.notes {
            entry.notes = notes.clone();
        }
        if let Some(icon) = &edit.icon {
            entry.icon = icon.clone();
        }
        if edit.artwork_game_id.is_some() {
            entry.artwork_game_id = edit.artwork_game_id;
        }
        if entry.fingerprint() != before {
            entry.invalidate_approval("The executable or launch options changed since you approved it.");
            entry.user_linked = false;
            if entry.import.is_some() {
                entry.status = Status::UpdateAvailable;
            }
        }
        Ok(())
    }

    /// Forgets the user's executable choice and goes back to the detected one.
    pub fn reset_override(&mut self, id: &str) -> Result<(), String> {
        let entry = self.entries.get_mut(id).ok_or("Unknown game")?;
        if entry.source != Source::Folder {
            return Err("Only scanned games have a detected executable to go back to.".into());
        }
        let before = entry.fingerprint();
        entry.user_exe = false;
        entry.user_start_dir = false;
        entry.user_launch_options = false;
        entry.set_exe_from_candidates();
        if entry.fingerprint() != before {
            entry.invalidate_approval("The executable changed since you approved it.");
        }
        Ok(())
    }

    pub fn link(&mut self, id: &str, shortcut: &SteamShortcut) -> Result<(), String> {
        let entry = self.entries.get_mut(id).ok_or("Unknown game")?;
        entry.approval = None;
        entry.user_linked = true;
        entry.status = Status::AlreadyInSteam;
        entry.linked = Some(DupMatch {
            kind: "shortcut".into(),
            name: shortcut.name.clone(),
            path: shortcut.exe.clone(),
            user_id: Some(shortcut.user_id.clone()),
            app_id: Some(shortcut.app_id),
            entry_id: None,
            boilr: shortcut.boilr,
        });
        Ok(())
    }

    /// Adds a game by hand. It enters the review queue like any other discovery and is
    /// never written to Steam before it is approved and confirmed.
    pub fn add_manual(&mut self, edit: &EntryEdit, snapshot: &SteamSnapshot) -> Result<String, String> {
        let exe = edit.exe.as_deref().unwrap_or_default().trim().trim_matches('"').to_string();
        validate_exe(&exe)?;
        let now = now_rfc3339();
        let id = format!("manual:{:016x}", fnv64(&format!("{}|{}", norm_path(&exe), now)));
        let mut entry = Self::new_entry(id.clone(), Source::Manual, &now);
        let file_stem = Path::new(&exe).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let folder_title = Path::new(&exe)
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| crate::title::title_from_folder(&n.to_string_lossy()))
            .unwrap_or_else(|| file_stem.clone());
        entry.title = if crate::title::is_generic_exe(&file_stem) { folder_title } else { file_stem.clone() };
        entry.title_source = "Added manually".into();
        entry.exe = exe.clone();
        entry.user_exe = true;
        entry.game_dir = parent_dir(&exe).unwrap_or_default();
        entry.start_dir = entry.game_dir.clone();
        entry.confidence = 100;
        self.entries.insert(id.clone(), entry);
        let mut edit = edit.clone();
        edit.exe = None;
        self.edit(&id, &edit)?;
        if let Some(e) = self.entries.get_mut(&id) {
            e.status = Status::NeedsReview;
        }
        self.reconcile(snapshot);
        Ok(id)
    }

    pub fn remove_manual(&mut self, id: &str) -> Result<(), String> {
        match self.entries.get(id) {
            Some(e) if e.source == Source::Manual && e.import.is_none() => {
                self.entries.remove(id);
                Ok(())
            }
            Some(_) => Err("Only manually added games that are not imported can be removed.".into()),
            None => Err("Unknown game".into()),
        }
    }
}

fn mark_missing(entry: &mut Entry) {
    if entry.status != Status::Missing {
        entry.prev_status = Some(entry.status);
        entry.status = Status::Missing;
    }
}

/// An executable path the user typed: it must exist and is never run.
pub fn validate_exe(exe: &str) -> Result<(), String> {
    if exe.is_empty() {
        return Err("Enter the path of the game's executable.".into());
    }
    let path = Path::new(exe);
    if !path.is_absolute() {
        return Err("Use the full path, for example D:\\Games\\MyGame\\Game.exe.".into());
    }
    if !path.is_file() {
        return Err(format!("{exe} does not exist."));
    }
    Ok(())
}

/// Duplicate evidence for one entry, strongest first. Title similarity alone is only
/// ever "low": it is shown, never acted on.
pub fn detect_duplicate<'a>(entry: &Entry, snapshot: &SteamSnapshot, others: impl IntoIterator<Item = &'a Entry>) -> Option<DuplicateInfo> {
    let exe = norm_path(&entry.exe);
    let args = entry.launch_options.trim();
    let game_dir = format!("{}/", norm_path(&entry.game_dir));
    let shortcut_match = |s: &SteamShortcut| DupMatch {
        kind: "shortcut".into(),
        name: s.name.clone(),
        path: s.exe.clone(),
        user_id: Some(s.user_id.clone()),
        app_id: Some(s.app_id),
        entry_id: None,
        boilr: s.boilr,
    };

    for s in snapshot.all_shortcuts() {
        let same_args = s.launch_options.trim() == args || s.launch_options.trim().is_empty() || args.is_empty();
        if norm_path(&s.exe_path()) == exe && same_args {
            let by = if s.boilr { " (created by BoilR)" } else if s.managed { " (created by BoilerSteamXL)" } else { "" };
            return Some(DuplicateInfo {
                confidence: "high".into(),
                reason: format!("Steam already has a shortcut to this executable: \"{}\"{by}", s.name),
                matched: shortcut_match(s),
            });
        }
    }
    for g in &snapshot.official {
        let dir = format!("{}/", norm_path(&g.install_dir));
        if exe.starts_with(&dir) {
            return Some(DuplicateInfo {
                confidence: "high".into(),
                reason: format!("Installed by Steam as \"{}\"", g.name),
                matched: DupMatch {
                    kind: "official".into(),
                    name: g.name.clone(),
                    path: g.install_dir.clone(),
                    user_id: None,
                    app_id: Some(g.app_id),
                    entry_id: None,
                    boilr: false,
                },
            });
        }
    }
    for other in others {
        if matches!(other.status, Status::Ignored | Status::Rejected) || other.exe.is_empty() {
            continue;
        }
        // The user accepted `other` as a separate copy; it does not make this one a duplicate.
        if other.approval.as_ref().is_some_and(|a| a.allow_duplicate) {
            continue;
        }
        let committed = |e: &Entry| {
            e.import.is_some() || matches!(e.status, Status::Approved | Status::ReadyToImport | Status::AlreadyInSteam)
        };
        let ahead = if committed(entry) { committed(other) && other.id < entry.id } else { committed(other) || other.id < entry.id };
        if ahead && norm_path(&other.exe) == exe && other.launch_options.trim() == args {
            return Some(DuplicateInfo {
                confidence: "medium".into(),
                reason: format!("Same executable as \"{}\" in BoilerSteamXL", other.display_title()),
                matched: DupMatch {
                    kind: "entry".into(),
                    name: other.display_title().to_string(),
                    path: other.exe.clone(),
                    user_id: None,
                    app_id: None,
                    entry_id: Some(other.id.clone()),
                    boilr: false,
                },
            });
        }
    }
    if entry.source == Source::Folder {
        if let Some(s) = snapshot.all_shortcuts().find(|s| norm_path(&s.exe_path()).starts_with(&game_dir)) {
            return Some(DuplicateInfo {
                confidence: "medium".into(),
                reason: format!("A Steam shortcut already starts a program in this game's folder: \"{}\"", s.name),
                matched: shortcut_match(s),
            });
        }
    }
    let title = norm_name(entry.display_title());
    if title.len() >= 3 {
        if let Some(s) = snapshot.all_shortcuts().find(|s| norm_name(&s.name) == title) {
            return Some(DuplicateInfo {
                confidence: "low".into(),
                reason: format!("A Steam shortcut has the same name: \"{}\" (the name alone is not proof)", s.name),
                matched: shortcut_match(s),
            });
        }
        if let Some(g) = snapshot.official.iter().find(|g| norm_name(&g.name) == title) {
            return Some(DuplicateInfo {
                confidence: "low".into(),
                reason: format!("A Steam game has the same name: \"{}\" (the name alone is not proof)", g.name),
                matched: DupMatch {
                    kind: "official".into(),
                    name: g.name.clone(),
                    path: g.install_dir.clone(),
                    user_id: None,
                    app_id: Some(g.app_id),
                    entry_id: None,
                    boilr: false,
                },
            });
        }
    }
    None
}
