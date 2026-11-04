// Typed wrappers for the BoilerSteamXL backend commands (src/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export type Status =
  | "needs_review"
  | "approved"
  | "rejected"
  | "ignored"
  | "possible_duplicate"
  | "already_in_steam"
  | "ready_to_import"
  | "importing"
  | "imported"
  | "import_failed"
  | "missing"
  | "update_available";

export type Candidate = {
  path: string;
  file_name: string;
  score: number;
  size: number;
  reasons: string[];
  description?: string | null;
  arguments?: string | null;
};
export type ExcludedExe = { path: string; rule_id: string; reason: string };
export type DupMatch = {
  kind: "shortcut" | "official" | "entry";
  name: string;
  path: string;
  user_id?: string | null;
  app_id?: number | null;
  entry_id?: string | null;
  boilr?: boolean;
};
export type DuplicateInfo = { confidence: "high" | "medium" | "low"; reason: string; matched: DupMatch };
export type ImportRecord = { app_id: number; identity: string; user_ids: string[]; at: string; name: string; exe: string; launch_options: string };

export type Entry = {
  id: string;
  source: "folder" | "manual" | "launcher";
  launcher?: string | null;
  title: string;
  title_source: string;
  custom_title?: string | null;
  /** Your name last written to its Steam shortcut. */
  synced_title?: string | null;
  game_dir: string;
  root?: string | null;
  exe: string;
  user_exe: boolean;
  start_dir: string;
  user_start_dir: boolean;
  launch_options: string;
  candidates: Candidate[];
  excluded: ExcludedExe[];
  confidence: number;
  uncertain: boolean;
  status: Status;
  prev_status?: Status | null;
  approval?: { fingerprint: string; at: string; allow_duplicate: boolean } | null;
  duplicate?: DuplicateInfo | null;
  import?: ImportRecord | null;
  linked?: DupMatch | null;
  user_linked: boolean;
  first_seen: string;
  last_seen: string;
  tags: string[];
  collection: string;
  notes: string;
  icon: string;
  note?: string | null;
};

export type ScanSummary = {
  at: string;
  folders_scanned: number;
  exes_found: number;
  exes_excluded: number;
  games_found: number;
  new_games: number;
  already_in_steam: number;
  needs_review: number;
  missing: number;
  duplicates: number;
  roots_scanned: string[];
  root_errors: [string, string][];
  cancelled: boolean;
  duration_ms: number;
  newly_discovered: string[];
  orphan_excluded: ExcludedExe[];
};

export type RunRecord = {
  id: string;
  kind: "scan" | "import" | "rebuild" | "restore";
  at: string;
  simulated: boolean;
  scheduled: boolean;
  user_ids: string[];
  counts: Record<string, number>;
  details: string[];
  backup?: string | null;
  error?: string | null;
};

/** A non-Steam shortcut in Steam now; `entry_id` is the library game it belongs to. */
export type SteamGame = { key: string; user_id: string; app_id: number; name: string; exe: string; start_dir: string; launch_options: string; made_by: string; exe_exists: boolean; entry_id: string | null; removal: boolean };

/** Where a game stands for the next "Sync with Steam". */
export type SyncState = "in_steam" | "rename" | "add" | "check" | "duplicate" | "maybe" | "remove" | "skipped" | "missing";
export const SYNC_LABEL: Record<SyncState, string> = {
  in_steam: "In Steam",
  rename: "Will be renamed",
  add: "Will be added",
  check: "Check this",
  duplicate: "In Steam twice: merge",
  maybe: "Maybe in Steam already",
  remove: "Will be removed",
  skipped: "Not added",
  missing: "Missing from this PC",
};

/** Two shortcuts are the same game when they start the same program, whatever their names. */
export const samePath = (p: string) => p.trim().replace(/^"+|"+$/g, "").replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();

/** Clear-cut new games: added by the next sync unless the user says "Don't add". */
export const syncEligible = (e: Entry) => e.status === "needs_review" && !e.uncertain && !e.duplicate && Boolean(e.exe);

/** The name you gave a game in Steam that Steam gets at the next sync (mirrors Library::pending_rename). */
export const pendingRename = (e: Entry, steamGames: SteamGame[]) => {
  const t = e.custom_title?.trim();
  if (!t || e.synced_title === t) return null;
  return steamGames.some((g) => g.entry_id === e.id && !g.removal && g.name !== t) ? t : null;
};

export const syncState = (e: Entry, removing: boolean): SyncState => {
  if (removing) return "remove";
  switch (e.status) {
    case "imported":
    case "already_in_steam":
      return "in_steam";
    case "approved":
    case "ready_to_import":
      return "add";
    case "needs_review":
      return syncEligible(e) ? "add" : "check";
    case "possible_duplicate":
      return "maybe";
    case "rejected":
    case "ignored":
      return "skipped";
    case "missing":
      return "missing";
    default:
      return "check";
  }
};

export type LibraryView = {
  entries: Entry[];
  last_scan: ScanSummary | null;
  history: RunRecord[];
  counts: Partial<Record<Status, number>>;
  has_rebuild_draft: boolean;
  schedule: { installed: boolean; next_run: string | null; last_scheduled: string | null };
};

export type RuleKind = "dir_name" | "dir_path" | "file_name" | "glob" | "regex" | "category";
export type Rule = { id: string; kind: RuleKind; pattern: string; enabled: boolean; builtin: boolean; inside_game_only: boolean; note: string };
export type RuleSet = { rules: Rule[]; allowlist: string[] };
export type ScanRoot = { path: string; enabled: boolean; max_depth: number };
export type Schedule = {
  mode: "manual" | "daily" | "weekly" | "custom";
  time: string;
  days: string[];
  interval_days: number;
  run_missed: boolean;
  scan_on_startup: boolean;
};
export type Preferences = {
  schema_version: number;
  first_run_complete: boolean;
  steam_path: string | null;
  scan_roots: ScanRoot[];
  rules: RuleSet;
  schedule: Schedule;
  simulation_mode: boolean;
  target_users: string[];
  include_launchers: boolean;
  notifications: boolean;
  theme: "dark" | "light";
  download_artwork: boolean;
  check_updates: boolean;
  update_url: string;
  /** The user removed the SteamGridDB key: a key from another BoilR config is not adopted again. */
  steamgriddb_key_removed: boolean;
};

export type AppInfo = {
  version: string;
  mode: string;
  data_dir: string;
  simulation: boolean;
  steam_running: boolean;
  first_run_complete: boolean;
  rebuild_phrase: string;
};

export type SteamInfo = {
  path: string;
  valid: boolean;
  error: string | null;
  users: { user_id: string; persona: string | null; most_recent: boolean; shortcut_count: number; error: string | null }[];
  library_folders: string[];
  official_count: number;
  running: boolean;
};

export type OpKind = "keep" | "create" | "replace" | "update" | "remove" | "unresolved";
export type ShortcutView = { app_id: number; name: string; exe: string; start_dir: string; launch_options: string; icon: string };
export type Op = {
  kind: OpKind;
  user_id: string;
  entry_id?: string | null;
  shortcut_key?: string | null;
  old?: ShortcutView | null;
  new?: ShortcutView | null;
  notes: string[];
};
export type Plan = {
  kind: "import" | "rebuild";
  mode?: RebuildMode | null;
  user_ids: string[];
  ops: Op[];
  blocked: { id: string; title: string; reason: string }[];
  summary: Record<string, number>;
  warnings: string[];
  simulation: boolean;
  token: string;
};
export type Outcome = { record: RunRecord; deferred: boolean };

export type RebuildMode = "managed" | "all_non_steam";
export type Decision =
  | { action: "unresolved" }
  | { action: "keep" }
  | { action: "replace"; entry_id: string }
  | { action: "remove"; confirmed: boolean };
export type RebuildDraft = {
  mode: RebuildMode;
  user_id: string;
  started_at: string;
  decisions: Record<string, Decision>;
  approved: Record<string, string>;
};
export type SteamShortcut = {
  user_id: string;
  index: number;
  app_id: number;
  name: string;
  exe: string;
  start_dir: string;
  launch_options: string;
  icon: string;
  tags: string[];
  identity: string;
  managed: boolean;
  boilr: boolean;
  exe_exists: boolean;
};
export type InspectedShortcut = {
  shortcut: SteamShortcut;
  key: string;
  in_scope: boolean;
  origin: string;
  broken: boolean;
  utility: string | null;
  duplicate_group: number | null;
  suggestion: { entry_id: string; title: string; evidence: string; strong: boolean } | null;
  artwork: string[];
};
export type Inspection = {
  user_id: string;
  mode: RebuildMode;
  shortcuts: InspectedShortcut[];
  groups: { keys: string[]; recommended_keep: string; reason: string }[];
  counts: Record<string, number>;
  error: string | null;
};

export type BackupManifest = {
  id: string;
  created: string;
  kind: string;
  app_version: string;
  users: { user_id: string; had_file: boolean; shortcuts: ShortcutView[]; grid_files: string[]; collections: boolean; after_fingerprint?: string | null }[];
  plan?: Plan | null;
  verified: boolean;
};
export type RestorePreview = {
  backup_id: string;
  users: { user_id: string; current_count: number; backup_count: number; lost: ShortcutView[]; regained: ShortcutView[]; changed_since: boolean }[];
  warnings: string[];
  token: string;
};
export type ManualCheck = { exists: boolean; is_exe: boolean; suggested_title: string; duplicates: DupMatch[]; preview: ShortcutView | null };
export type EntryEdit = Partial<{
  title: string;
  exe: string;
  start_dir: string;
  launch_options: string;
  tags: string[];
  collection: string;
  notes: string;
  icon: string;
}>;
export type DiscoveredGame = {
  game_dir: string;
  title: string;
  title_source: string;
  root: string;
  candidates: Candidate[];
  excluded: ExcludedExe[];
  confidence: number;
  uncertain: boolean;
};
export type GameArt = {
  cover: string | null;
  hero: string | null;
  logo: string | null;
  icon: string | null;
  source: string | null;
  match_name: string | null;
  match_chosen: boolean;
  /** Cover file's modification time, so a new image under the same name is shown. */
  version?: number | null;
};
export type ArtProgress = { done: number; total: number; current: string };
export type FetchSummary = { checked: number; downloaded: number; not_found: number; failed: number };

export type ScanProgress = { phase: string; folders_scanned: number; exes_found: number; current: string };

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getPrefs: () => invoke<Preferences>("get_prefs"),
  savePrefs: (prefs: Preferences) => invoke<Preferences>("save_prefs", { prefs }),
  steamInfo: () => invoke<SteamInfo>("steam_info"),
  checkSteamPath: (path: string) => invoke<void>("check_steam_path", { path }),
  suggestRoots: () => invoke<string[]>("suggest_scan_roots"),
  library: () => invoke<LibraryView>("get_library"),
  refresh: () => invoke<ScanSummary>("refresh_library"),
  cancelRefresh: () => invoke<void>("cancel_refresh"),
  approve: (ids: string[], allowDuplicate = false) => invoke<{ errors: [string, string][] }>("approve", { ids, allowDuplicate }),
  approveEligible: () => invoke<{ errors: [string, string][] }>("approve_eligible"),
  reject: (ids: string[]) => invoke<void>("reject", { ids }),
  ignore: (ids: string[]) => invoke<void>("ignore", { ids }),
  restoreEntries: (ids: string[]) => invoke<void>("restore_entries", { ids }),
  editEntry: (id: string, edit: EntryEdit) => invoke<Entry>("edit_entry", { id, edit }),
  resetOverride: (id: string) => invoke<void>("reset_override", { id }),
  linkEntry: (id: string, shortcutKey: string) => invoke<void>("link_entry", { id, shortcutKey }),
  listShortcuts: () => invoke<{ key: string; user_id: string; name: string; exe: string }[]>("list_shortcuts"),
  steamGames: () => invoke<SteamGame[]>("steam_games"),
  setRemoval: (keys: string[], remove: boolean) => invoke<void>("set_removal", { keys, remove }),
  checkManual: (exe: string, title?: string, launchOptions?: string) => invoke<ManualCheck>("check_manual", { exe, title, launchOptions }),
  addManual: (edit: EntryEdit, allowDuplicate: boolean) => invoke<string>("add_manual", { edit, allowDuplicate }),
  removeManual: (id: string) => invoke<void>("remove_manual", { id }),
  analyzeFolder: (path: string) => invoke<{ games: DiscoveredGame[]; excluded: ExcludedExe[] }>("analyze_folder", { path }),
  planImport: () => invoke<Plan>("plan_import"),
  executeImport: (token: string) => invoke<Outcome>("execute_import", { token }),
  closeSteam: () => invoke<void>("close_steam"),
  startSteam: () => invoke<void>("start_steam"),
  previewRule: (rule: Rule) => invoke<{ path: string; game: string }[]>("preview_rule", { rule }),
  validateRule: (kind: RuleKind, pattern: string) => invoke<void>("validate_rule", { kind, pattern }),
  recommendedRules: () => invoke<RuleSet>("recommended_rules"),
  exportJson: (path: string, what: "rules" | "settings") => invoke<void>("export_json", { path, what }),
  importJson: (path: string, what: "rules" | "settings") => invoke<void>("import_json", { path, what }),
  rebuildStart: (mode: RebuildMode, userId: string) => invoke<void>("rebuild_start", { mode, userId }),
  rebuildDraft: () => invoke<RebuildDraft | null>("rebuild_draft"),
  rebuildInspect: () => invoke<Inspection>("rebuild_inspect"),
  rebuildSetDecision: (key: string, decision: Decision) => invoke<void>("rebuild_set_decision", { key, decision }),
  rebuildApprove: (ids: string[], approved: boolean) => invoke<[string, string][]>("rebuild_approve", { ids, approved }),
  rebuildPlan: () => invoke<Plan>("rebuild_plan"),
  rebuildExecute: (token: string, phrase: string, acknowledged: boolean) => invoke<RunRecord>("rebuild_execute", { token, phrase, acknowledged }),
  rebuildCancel: () => invoke<void>("rebuild_cancel"),
  listBackups: () => invoke<BackupManifest[]>("list_backups"),
  restorePreview: (id: string) => invoke<RestorePreview>("restore_preview", { id }),
  restoreBackup: (id: string, token: string) => invoke<RunRecord>("restore_backup", { id, token }),
  exportBackup: (id: string, dest: string) => invoke<string>("export_backup", { id, dest }),
  logs: (limit: number) => invoke<Record<string, string>[]>("get_logs", { limit }),
  diagnostics: () => invoke<string>("diagnostics_report"),
  schedulePreview: (schedule: Schedule) => invoke<string | null>("schedule_preview", { schedule }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  openDataFolder: () => invoke<void>("open_data_folder"),
  launchInSteam: (appId: number) => invoke<void>("launch_in_steam", { appId }),
  checkUpdate: () => invoke<{ current: string; latest: string | null; url: string | null; available: boolean }>("check_update"),
  platformSettings: () => invoke<{ code_name: string; name: string; settings: Record<string, unknown> }[]>("list_platform_settings"),
  setPlatformEnabled: (codeName: string, enabled: boolean) => invoke<void>("update_platform_enabled", { codeName, enabled }),
  inspectAccount: (userId: string) => invoke<Inspection>("inspect_account", { userId }),
  gameArt: () => invoke<Record<string, GameArt>>("game_art"),
  fetchArtwork: (ids: string[] | null, force: boolean) => invoke<FetchSummary>("fetch_artwork", { ids, force }),
  searchArtwork: (query: string) => invoke<{ id: number; name: string; year: number | null }[]>("search_artwork", { query }),
  setArtworkMatch: (entryId: string, gameId: number, gameName: string) => invoke<FetchSummary>("set_artwork_match", { entryId, gameId, gameName }),
  openKeyPage: () => {
    keyPage.opened = true;
    return invoke<void>("open_steamgriddb_key_page");
  },
  checkArtKey: (key: string) => invoke<void>("check_steamgriddb_key", { key }),
  artworkThumb: (gameId: number) => invoke<string | null>("artwork_thumb", { gameId }),
};

/** Set once the user opened the key page: only then is a key looked for in the clipboard. */
export const keyPage = { opened: false };
export const STEAMGRIDDB_KEY_PAGE = "https://www.steamgriddb.com/profile/preferences/api";
/** SteamGridDB API keys are 32 hexadecimal characters. */
export const looksLikeArtKey = (s: string) => /^[0-9a-f]{32}$/i.test(s.trim());

export const STATUS_LABEL: Record<Status, string> = {
  needs_review: "Needs review",
  approved: "Approved",
  rejected: "Rejected",
  ignored: "Ignored",
  possible_duplicate: "Possible duplicate",
  already_in_steam: "Already in Steam",
  ready_to_import: "Ready to import",
  importing: "Importing",
  imported: "In Steam",
  import_failed: "Import failed",
  missing: "Missing from this PC",
  update_available: "Update available",
};

export const STATUS_TONE: Record<Status, "accent" | "ok" | "warn" | "bad" | "muted" | "info"> = {
  needs_review: "accent",
  approved: "ok",
  rejected: "muted",
  ignored: "muted",
  possible_duplicate: "warn",
  already_in_steam: "info",
  ready_to_import: "ok",
  importing: "info",
  imported: "ok",
  import_failed: "bad",
  missing: "bad",
  update_available: "warn",
};

export const displayTitle = (e: Entry) => (e.custom_title?.trim() ? e.custom_title : e.title);
