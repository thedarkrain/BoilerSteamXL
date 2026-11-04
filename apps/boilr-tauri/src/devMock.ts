// Browser-only stand-in for the Tauri backend (`npm run dev`), so the interface can be
// clicked through and screenshotted without Rust or Steam. Never loaded inside Tauri.
// All data here is SAMPLE data; the app shows a banner saying so.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { BackupManifest, Decision, Entry, Inspection, InspectedShortcut, Op, Plan, Preferences, RebuildDraft, RunRecord, Status } from "./api";

const now = () => new Date().toISOString();
const ago = (h: number) => new Date(Date.now() - h * 3600_000).toISOString();

let prefs: Preferences = {
  schema_version: 1,
  first_run_complete: new URLSearchParams(location.search).get("firstrun") !== "1",
  steam_path: null,
  steamgriddb_key_removed: false,
  scan_roots: [{ path: "D:\\Games", enabled: true, max_depth: 6 }],
  rules: {
    allowlist: [],
    rules: [
      ["d-redist", "dir_name", "_Redist", false, "Redistributable installers"],
      ["d-commonredist", "dir_name", "_CommonRedist", false, "Steam-style shared redistributables"],
      ["d-ue-redist", "dir_name", "Engine/Extras/Redist", false, "Unreal Engine prerequisites"],
      ["d-directx", "dir_name", "DirectX", false, "DirectX installers"],
      ["d-installers", "dir_name", "Installers", false, "Installer folders"],
      ["d-support", "dir_name", "Support", true, "Support tools inside a game"],
      ["f-quicksfv", "file_name", "QuickSFV.exe", false, "File verification utility"],
      ["f-unins", "file_name", "unins*.exe", false, "Inno Setup uninstaller"],
      ["f-vcredist", "file_name", "vc_redist*.exe", false, "Visual C++ runtime installer"],
      ["f-dxsetup", "file_name", "DXSETUP.exe", false, "DirectX installer"],
      ["f-setup", "file_name", "setup.exe", false, "Installer"],
      ["f-langsel", "file_name", "Language Selector.exe", false, "Language selection utility"],
      ["c-language_selector", "category", "language_selector", false, "Any language selector"],
      ["c-crash_reporter", "category", "crash_reporter", false, "Any crash reporter"],
    ].map(([id, kind, pattern, inside, note]) => ({ id, kind, pattern, enabled: true, builtin: true, inside_game_only: inside, note })) as Preferences["rules"]["rules"],
  },
  schedule: { mode: "weekly", time: "09:00", days: ["monday", "thursday"], interval_days: 3, run_missed: true, scan_on_startup: false },
  simulation_mode: true,
  target_users: [],
  include_launchers: false,
  notifications: true,
  theme: "dark",
  download_artwork: true,
  check_updates: false,
  update_url: "",
};

const G = "D:\\Games\\PLUG&PLAY";
const cand = (path: string, score: number, mb: number, reasons: string[]) => ({ path, file_name: path.split("\\").pop() ?? path, score, size: mb * 1048576, reasons: ["40 base score", ...reasons], description: null, arguments: null });
const entry = (p: Partial<Entry> & Pick<Entry, "id" | "title" | "game_dir">): Entry => ({
  source: "folder",
  launcher: null,
  title_source: "Folder name",
  custom_title: null,
  root: "D:\\Games",
  exe: p.candidates?.[0]?.path ?? "",
  user_exe: false,
  start_dir: p.game_dir,
  user_start_dir: false,
  launch_options: "",
  candidates: [],
  excluded: [],
  confidence: 90,
  uncertain: false,
  status: "needs_review",
  approval: null,
  duplicate: null,
  import: null,
  linked: null,
  user_linked: false,
  first_seen: ago(30),
  last_seen: now(),
  tags: [],
  collection: "",
  notes: "",
  icon: "",
  note: null,
  ...p,
});

let entries: Entry[] = [
  entry({
    id: "fs:mvc",
    title: "Marvel vs. Capcom",
    game_dir: `${G}\\marvel-vs-capcom`,
    candidates: [cand(`${G}\\marvel-vs-capcom\\game.exe`, 69, 8, ["+5 generic game executable name; the title comes from the folder", "+10 in the game's main folder", "+6 file size 8.0 MB", "+8 ..."])],
    excluded: [
      { path: `${G}\\marvel-vs-capcom\\_Redist\\vcredist_x64.exe`, rule_id: "d-redist", reason: "Ignored: inside \"_Redist\" folder" },
      { path: `${G}\\marvel-vs-capcom\\Engine\\Binaries\\Win64\\CrashReportClient.exe`, rule_id: "c-crash_reporter", reason: "Ignored: crash reporter" },
    ],
    confidence: 69,
  }),
  entry({
    id: "fs:awo",
    title: "A Way Out",
    game_dir: `${G}\\A Way Out`,
    candidates: [cand(`${G}\\A Way Out\\AWayOut.exe`, 97, 20, ["+35 name matches the game folder", "+10 in the game's main folder", "+10 file size 20.0 MB"])],
    excluded: [
      { path: `${G}\\A Way Out\\QuickSFV.EXE`, rule_id: "f-quicksfv", reason: "Ignored: file verification utility" },
      { path: `${G}\\A Way Out\\unins000.exe`, rule_id: "f-unins", reason: "Ignored: inno setup uninstaller" },
      { path: `${G}\\A Way Out\\_CommonRedist\\DirectX\\Jun2010\\DXSETUP.exe`, rule_id: "d-commonredist", reason: "Ignored: inside \"_CommonRedist\" folder" },
    ],
    confidence: 97,
    status: "possible_duplicate",
    duplicate: { confidence: "medium", reason: "A Steam shortcut already starts a program in this game's folder: \"A Way Out Copy\"", matched: { kind: "shortcut", name: "A Way Out Copy", path: `"${G}\\A Way Out\\AWayOut.exe"`, user_id: "48151623" } },
  }),
  entry({
    id: "fs:haze",
    title: "Haze1",
    game_dir: `${G}\\Haze1`,
    candidates: [
      cand(`${G}\\Haze1\\Binaries\\Win64\\Game-Win64-Shipping.exe`, 58, 30, ["+5 generic game executable name; the title comes from the folder", "-4 2 folder(s) deep (Binaries\\Win64)", "+10 file size 30.0 MB"]),
      cand(`${G}\\Haze1\\Binaries\\Win64\\HazeLauncher.exe`, 52, 4, ["+25 name contains the game name", "-4 2 folder(s) deep", "+6 file size", "-8 looks like a launcher"]),
    ],
    excluded: [{ path: `${G}\\Haze1\\Language Selector.exe`, rule_id: "f-langsel", reason: "Ignored: language selection utility" }],
    confidence: 58,
    uncertain: true,
  }),
  entry({ id: "fs:halo", title: "Halo", game_dir: "D:\\Games\\Halo", candidates: [cand("D:\\Games\\Halo\\haloce.exe", 96, 5, ["+28 name is the game's initials", "+10 in the game's main folder", "+6 file size 5.0 MB"])], confidence: 96 }),
  entry({ id: "fs:codwaw", title: "Call of Duty: World at War", game_dir: "D:\\Games\\call.of.duty.world.at.war", candidates: [cand("D:\\Games\\call.of.duty.world.at.war\\CoDWaW.exe", 99, 10, ["+28 name is the game's initials", "+10 in the game's main folder", "+10 file size 10.0 MB"])], confidence: 99, status: "approved", approval: { fingerprint: "x", at: ago(1), allow_duplicate: false } }),
  entry({ id: "fs:hk", title: "Hollow Knight", game_dir: "D:\\Games\\Hollow.Knight.v1.5.78-FitGirl.Repack", candidates: [cand("D:\\Games\\Hollow.Knight.v1.5.78-FitGirl.Repack\\hollow_knight.exe", 99, 15, ["+35 name matches the game folder", "+15 Unity player (has hollow_knight_Data)"])], confidence: 99, status: "imported", import: { app_id: 3112849234, identity: "bsxl-1", user_ids: ["48151623"], at: ago(48), name: "Hollow Knight", exe: "", launch_options: "" } }),
  entry({ id: "fs:w3", title: "The Witcher 3: Wild Hunt", game_dir: "D:\\Games\\the-witcher-3-wild-hunt", candidates: [cand("D:\\Games\\the-witcher-3-wild-hunt\\bin\\x64\\witcher3.exe", 94, 60, ["+25 name contains the game name", "-4 2 folder(s) deep (bin\\x64)", "+12 file size 60.0 MB"])], confidence: 94, status: "imported", import: { app_id: 2849123001, identity: "bsxl-2", user_ids: ["48151623"], at: ago(48), name: "The Witcher 3: Wild Hunt", exe: "", launch_options: "" } }),
  entry({ id: "fs:re4", title: "Resident Evil 4", game_dir: "D:\\Games\\resident_evil_4", candidates: [cand("D:\\Games\\resident_evil_4\\re4.exe", 88, 40, ["+28 name is the game's initials"])], confidence: 88 }),
  entry({ id: "fs:other", title: "Other Game", game_dir: `${G}\\Other Game`, candidates: [cand(`${G}\\Other Game\\start.exe`, 61, 3, ["+5 generic game executable name; the title comes from the folder", "+10 in the game's main folder", "+6 file size 3.0 MB"])], confidence: 61, uncertain: true }),
  entry({ id: "fs:okami", title: "Ōkami HD", game_dir: "D:\\Games\\Ōkami HD", candidates: [cand("D:\\Games\\Ōkami HD\\okami.exe", 95, 12, ["+35 name matches the game folder"])], confidence: 95 }),
  entry({ id: "fs:batman", title: "Batman: Arkham City", game_dir: "D:\\Games\\batman_arkham_city", candidates: [cand("D:\\Games\\batman_arkham_city\\Binaries\\Win32\\BatmanAC.exe", 86, 30, ["+25 name contains the game name"])], confidence: 86, status: "already_in_steam", duplicate: { confidence: "high", reason: "Steam already has a shortcut to this executable: \"Batman AC\" (created by BoilR)", matched: { kind: "shortcut", name: "Batman AC", path: "", boilr: true } }, linked: { kind: "shortcut", name: "Batman AC", path: "" } }),
  entry({ id: "fs:nfs", title: "Need for Speed: Most Wanted", game_dir: "D:\\Games\\need-for-speed-most-wanted", candidates: [cand("D:\\Games\\need-for-speed-most-wanted\\speed.exe", 72, 6, ["+10 in the game's main folder"])], confidence: 72, status: "rejected" }),
  entry({ id: "manual:1", source: "manual", title: "Retro Emulator", title_source: "Added manually", game_dir: "E:\\Emulators\\RetroArch", exe: "E:\\Emulators\\RetroArch\\retroarch.exe", user_exe: true, confidence: 100, candidates: [] }),
];

const shortcuts = [
  { name: "My Emulator", exe: "\"E:\\Emulators\\emu.exe\"", origin: "Other / manual", managed: false, boilr: false, exists: true, args: "-fullscreen" },
  { name: "A Way Out", exe: `"${G}\\A Way Out\\AWayOut.exe"`, origin: "BoilR", managed: false, boilr: true, exists: true, args: "" },
  { name: "A Way Out Copy", exe: `"${G}\\A Way Out\\AWayOut.exe"`, origin: "Other / manual", managed: false, boilr: false, exists: true, args: "" },
  { name: "Old Game", exe: "\"C:\\Gone\\old.exe\"", origin: "Other / manual", managed: false, boilr: false, exists: false, args: "" },
  { name: "QuickSFV", exe: `"${G}\\A Way Out\\QuickSFV.EXE"`, origin: "Other / manual", managed: false, boilr: false, exists: true, args: "" },
  { name: "Marvel_vs_Capcom_Launcher", exe: `"${G}\\marvel-vs-capcom\\game.exe"`, origin: "Other / manual", managed: false, boilr: false, exists: true, args: "" },
  { name: "Hollow Knight", exe: "\"D:\\Games\\Hollow.Knight.v1.5.78-FitGirl.Repack\\hollow_knight.exe\"", origin: "BoilerSteamXL", managed: true, boilr: false, exists: true, args: "" },
  { name: "The Witcher 3: Wild Hunt", exe: "\"D:\\Games\\the-witcher-3-wild-hunt\\bin\\x64\\witcher3.exe\"", origin: "BoilerSteamXL", managed: true, boilr: false, exists: true, args: "" },
];
const owner: Record<string, string> = { "Hollow Knight": "fs:hk", "The Witcher 3: Wild Hunt": "fs:w3" };
let removals = new Set<string>();
const steamGames = () =>
  shortcuts.map((s, i) => ({ key: `48151623#${3000000000 + i}#${i}`, user_id: "48151623", app_id: 3000000000 + i, name: s.name, exe: s.exe.replace(/"/g, ""), start_dir: "", launch_options: s.args, made_by: s.managed ? "BoilerSteamXL" : s.boilr ? "BoilR" : "", exe_exists: s.exists, entry_id: owner[s.name] ?? null, removal: removals.has(s.name) }));
const suggestFor: Record<string, [string, string, boolean]> = {
  "A Way Out": ["fs:awo", "same executable", true],
  "A Way Out Copy": ["fs:awo", "same executable", true],
  Marvel_vs_Capcom_Launcher: ["fs:mvc", "same executable", true],
  "Hollow Knight": ["fs:hk", "identity", true],
  "The Witcher 3: Wild Hunt": ["fs:w3", "identity", true],
};

let history: RunRecord[] = [
  { id: "s1", kind: "scan", at: ago(2), simulated: false, scheduled: true, user_ids: [], counts: { folders: 214, games: 13, new: 4, needs_review: 6 }, details: [], backup: null, error: null },
  { id: "i1", kind: "import", at: ago(48), simulated: false, scheduled: false, user_ids: ["48151623"], counts: { create: 2 }, details: ["Create Hollow Knight → 48151623", "Create The Witcher 3: Wild Hunt → 48151623"], backup: "2026-11-02_201500-import", error: null },
];
let draft: RebuildDraft | null = null;
const backups: BackupManifest[] = [
  { id: "2026-11-02_201500-import", created: ago(48), kind: "import", app_version: "1.0.0", verified: true, users: [{ user_id: "48151623", had_file: true, shortcuts: [], grid_files: [], collections: false }] },
];

const counts = () => entries.reduce<Partial<Record<Status, number>>>((acc, e) => ({ ...acc, [e.status]: (acc[e.status] ?? 0) + 1 }), {});
const lastScan = {
  at: ago(2), folders_scanned: 214, exes_found: 38, exes_excluded: 12, games_found: 13, new_games: 4, already_in_steam: 3, needs_review: 6, missing: 0, duplicates: 1,
  roots_scanned: ["D:\\Games"], root_errors: [] as [string, string][], cancelled: false, duration_ms: 1840, newly_discovered: [],
  orphan_excluded: [{ path: "D:\\Games\\_CommonRedist\\vcredist\\2019\\VC_redist.x64.exe", rule_id: "d-commonredist", reason: "Ignored: inside \"_CommonRedist\" folder" }],
};
const find = (id: string) => entries.find((e) => e.id === id);
const set = (id: string, patch: Partial<Entry>) => (entries = entries.map((e) => (e.id === id ? { ...e, ...patch } : e)));

const view = (e: Entry) => ({ app_id: 2000000000 + e.id.length * 7919, name: e.custom_title || e.title, exe: `"${e.exe}"`, start_dir: `"${e.start_dir}"`, launch_options: e.launch_options, icon: "" });
const planImport = (): Plan => {
  const approved = entries.filter((e) => e.status === "approved" || e.status === "ready_to_import");
  const removed: Op[] = steamGames()
    .filter((g) => g.removal)
    .map((g) => ({ kind: "remove", user_id: g.user_id, shortcut_key: g.key, entry_id: g.entry_id, old: { app_id: g.app_id, name: g.name, exe: `"${g.exe}"`, start_dir: "", launch_options: "", icon: "" }, notes: ["Only the Steam shortcut is removed; the game's files are not touched. It stays in the backup."] }));
  const renamed: Op[] = steamGames()
    .filter((g) => g.entry_id && !g.removal)
    .flatMap((g) => {
      const e = find(g.entry_id!);
      const t = e?.custom_title?.trim();
      if (!e || !t || e.synced_title === t || g.name === t) return [];
      const old = { app_id: g.app_id, name: g.name, exe: `"${g.exe}"`, start_dir: "", launch_options: g.launch_options, icon: "" };
      return [{ kind: "update" as const, user_id: g.user_id, shortcut_key: g.key, entry_id: e.id, old, new: { ...old, name: t }, notes: ["Only the name in Steam changes; the Steam ID, artwork, play time and settings stay."] }];
    });
  const ops: Op[] = [...removed, ...renamed, ...approved.map((e) => ({ kind: "create" as const, user_id: "48151623", entry_id: e.id, new: view(e), notes: [] }))];
  return {
    kind: "import", user_ids: ["48151623"], ops, blocked: [], warnings: [], simulation: prefs.simulation_mode, token: "mock",
    summary: { before: shortcuts.length, after: shortcuts.length + ops.length - 2 * removed.length, create: ops.length - removed.length - renamed.length, update: renamed.length, remove: removed.length, duplicates_created: 0, still_review: (counts().needs_review ?? 0) + (counts().possible_duplicate ?? 0), excluded: 12 },
  };
};

const inspection = (): Inspection => {
  const mode = draft?.mode ?? "all_non_steam";
  const items: InspectedShortcut[] = shortcuts.map((s, i) => {
    const sug = suggestFor[s.name];
    return {
      key: `48151623#${3000000000 + i}#${i}`,
      in_scope: mode === "all_non_steam" || s.managed,
      origin: s.origin,
      broken: !s.exists,
      utility: s.name === "QuickSFV" ? "Ignored: file verification utility" : null,
      duplicate_group: s.name.startsWith("A Way Out") ? 0 : null,
      suggestion: sug ? { entry_id: sug[0], title: find(sug[0])?.title ?? "", evidence: sug[1], strong: sug[2] } : null,
      artwork: s.name === "Marvel_vs_Capcom_Launcher" ? ["cover.png", "hero.png"] : [],
      shortcut: { user_id: "48151623", index: i, app_id: 3000000000 + i, name: s.name, exe: s.exe, start_dir: "", launch_options: s.args, icon: "", tags: [], identity: "", managed: s.managed, boilr: s.boilr, exe_exists: s.exists },
    };
  });
  return {
    user_id: "48151623", mode, shortcuts: items, error: null,
    groups: [{ keys: [items[1].key, items[2].key], recommended_keep: items[1].key, reason: "Recommended: the one whose executable exists, that BoilerSteamXL manages and that has artwork." }],
    counts: { total: items.length, in_scope: items.filter((x) => x.in_scope).length, managed: 2, boilr: 1, other: 5, broken: 1, duplicates: 2, utilities: 1, with_artwork: 1, unmatched: 3 },
  };
};

const planRebuild = (): Plan => {
  const ins = inspection();
  const d = draft!;
  const ops: Op[] = [];
  const blocked: Plan["blocked"] = [];
  const used = new Set<string>();
  for (const s of ins.shortcuts) {
    const dec: Decision = s.in_scope ? d.decisions[s.key] ?? { action: "unresolved" } : { action: "keep" };
    const old = { app_id: s.shortcut.app_id, name: s.shortcut.name, exe: s.shortcut.exe, start_dir: "", launch_options: s.shortcut.launch_options, icon: "" };
    if (dec.action === "keep") ops.push({ kind: "keep", user_id: "48151623", shortcut_key: s.key, old, notes: s.in_scope ? [] : ["Not part of this rebuild (not created by BoilerSteamXL)."] });
    else if (dec.action === "unresolved") ops.push({ kind: "unresolved", user_id: "48151623", shortcut_key: s.key, old, notes: ["No decision: left exactly as it is."] });
    else if (dec.action === "remove") {
      if (!dec.confirmed) blocked.push({ id: s.key, title: s.shortcut.name, reason: "Removing without a replacement needs its own confirmation." });
      else ops.push({ kind: "remove", user_id: "48151623", shortcut_key: s.key, old, notes: ["Removed without a replacement. Its launch command is kept in the backup."] });
    } else {
      const e = find(dec.entry_id);
      if (!e || !d.approved[dec.entry_id]) blocked.push({ id: s.key, title: s.shortcut.name, reason: "The replacement game is not approved in this rebuild (or changed after approval)." });
      else {
        used.add(e.id);
        ops.push({ kind: "replace", user_id: "48151623", shortcut_key: s.key, entry_id: e.id, old, new: view(e), notes: [`Steam ID changes from ${old.app_id} to ${view(e).app_id}: artwork is copied to the new ID; collections and per-game Steam settings tied to the old ID may not carry over.`] });
      }
    }
  }
  for (const id of Object.keys(d.approved)) if (!used.has(id)) { const e = find(id); if (e) ops.push({ kind: "create", user_id: "48151623", entry_id: id, new: view(e), notes: [] }); }
  const c = (k: Op["kind"]) => ops.filter((o) => o.kind === k).length;
  return {
    kind: "rebuild", mode: d.mode, user_ids: ["48151623"], ops, blocked, warnings: [], simulation: prefs.simulation_mode, token: "mock-rebuild",
    summary: { before: shortcuts.length, after: shortcuts.length + c("create") - c("remove"), keep: c("keep"), create: c("create"), replace: c("replace"), update: 0, remove: c("remove"), unresolved: c("unresolved"), blocked: blocked.length, duplicates_created: 0, still_review: counts().needs_review ?? 0, out_of_scope: 0 },
  };
};

const banner = document.createElement("div");
banner.textContent = "Browser preview with SAMPLE data. No real games or Steam files are involved.";
banner.style.cssText = "position:fixed;bottom:0;left:50%;transform:translateX(-50%);z-index:100;background:#ffb547;color:#111;font:600 11px Segoe UI,system-ui;padding:3px 12px;border-radius:8px 8px 0 0;pointer-events:none";
document.body.appendChild(banner);

mockIPC(async (cmd, args) => {
  const a = (args ?? {}) as Record<string, any>;
  await new Promise((r) => setTimeout(r, 60));
  switch (cmd) {
    case "app_info":
      return { version: "1.0.0", mode: "installed", data_dir: "C:\\Users\\<user>\\AppData\\Roaming\\BoilerSteamXL", simulation: prefs.simulation_mode, steam_running: false, first_run_complete: prefs.first_run_complete, rebuild_phrase: "REBUILD MY NON-STEAM LIBRARY" };
    case "get_prefs":
      return prefs;
    case "save_prefs":
      prefs = a.prefs;
      return prefs;
    case "steam_info":
      return { path: "C:\\Program Files (x86)\\Steam", valid: true, error: null, users: [{ user_id: "48151623", persona: "Player One", most_recent: true, shortcut_count: shortcuts.length, error: null }], library_folders: ["C:\\Program Files (x86)\\Steam", "D:\\SteamLibrary"], official_count: 42, running: false };
    case "check_steam_path":
      if (String(a.path).toLowerCase().includes("games")) throw "D:\\Games has no userdata folder, so it is not a Steam installation.";
      return null;
    case "suggest_scan_roots":
      return ["D:\\Games"];
    case "get_library":
      return { entries, last_scan: lastScan, history, counts: counts(), has_rebuild_draft: Boolean(draft), schedule: { installed: true, next_run: new Date(Date.now() + 86400000 * 2).toISOString(), last_scheduled: ago(2) } };
    case "refresh_library":
      await new Promise((r) => setTimeout(r, 1200));
      history = [{ id: `s${Date.now()}`, kind: "scan", at: now(), simulated: false, scheduled: false, user_ids: [], counts: { folders: 214, games: entries.length, new: 0, needs_review: counts().needs_review ?? 0 }, details: [], backup: null, error: null }, ...history];
      lastScan.at = now();
      return lastScan;
    case "cancel_refresh":
      return null;
    case "approve": {
      const errors: [string, string][] = [];
      for (const id of a.ids as string[]) {
        const e = find(id);
        if (e?.status === "possible_duplicate" && !a.allowDuplicate) errors.push([id, "This may be a duplicate; confirm that you want a separate shortcut."]);
        else set(id, { status: "approved", approval: { fingerprint: "x", at: now(), allow_duplicate: Boolean(a.allowDuplicate) } });
      }
      return { errors };
    }
    case "approve_eligible":
      entries.filter((e) => e.status === "needs_review" && !e.uncertain && !e.duplicate && e.exe).forEach((e) => set(e.id, { status: "approved" }));
      return { errors: [] };
    case "reject":
      (a.ids as string[]).forEach((id) => set(id, { status: "rejected", approval: null }));
      return null;
    case "ignore":
      (a.ids as string[]).forEach((id) => set(id, { status: "ignored", approval: null }));
      return null;
    case "restore_entries":
      (a.ids as string[]).forEach((id) => set(id, { status: "needs_review", approval: null, user_linked: false }));
      return null;
    case "edit_entry": {
      const e = find(a.id)!;
      const edit = a.edit as Record<string, any>;
      const patch: Partial<Entry> = {};
      if (edit.title !== undefined) patch.custom_title = edit.title !== e.title ? edit.title : null;
      if (edit.exe && edit.exe !== e.exe) Object.assign(patch, { exe: edit.exe, user_exe: true, status: e.status === "approved" ? "needs_review" : e.status, approval: null });
      if (edit.launch_options !== undefined && edit.launch_options !== e.launch_options) Object.assign(patch, { launch_options: edit.launch_options, status: e.status === "approved" ? "needs_review" : e.status, approval: null });
      for (const k of ["start_dir", "tags", "collection", "notes"]) if (edit[k] !== undefined) (patch as any)[k] = edit[k];
      set(a.id, patch);
      return find(a.id);
    }
    case "reset_override": {
      const e = find(a.id)!;
      set(a.id, { exe: e.candidates[0]?.path ?? "", user_exe: false });
      return null;
    }
    case "steam_games":
      return steamGames();
    case "set_removal":
      for (const g of steamGames().filter((x) => (a.keys as string[]).includes(x.key))) a.remove ? removals.add(g.name) : removals.delete(g.name);
      return null;
    case "list_shortcuts":
      return shortcuts.map((s, i) => ({ key: `k${i}`, user_id: "48151623", name: s.name, exe: s.exe }));
    case "link_entry":
      set(a.id, { status: "already_in_steam", user_linked: true, linked: { kind: "shortcut", name: "Linked shortcut", path: "" } });
      return null;
    case "check_manual": {
      const exe = String(a.exe);
      const dup = entries.find((e) => e.exe.toLowerCase() === exe.toLowerCase());
      const folder = exe.split("\\").slice(-2, -1)[0] ?? "Game";
      return {
        exists: true,
        is_exe: exe.toLowerCase().endsWith(".exe"),
        suggested_title: folder.replace(/[-_.]/g, " "),
        duplicates: dup ? [{ kind: "entry", name: dup.title, path: dup.exe, entry_id: dup.id }] : [],
        preview: { app_id: 2412345678, name: a.title || folder, exe: `"${exe}"`, start_dir: "", launch_options: a.launchOptions ?? "", icon: "" },
      };
    }
    case "add_manual": {
      const id = `manual:${Date.now()}`;
      entries = [...entries, entry({ id, source: "manual", title: a.edit.title, title_source: "Added manually", game_dir: String(a.edit.exe).replace(/\\[^\\]+$/, ""), exe: a.edit.exe, user_exe: true, launch_options: a.edit.launch_options ?? "", confidence: 100 })];
      return id;
    }
    case "remove_manual":
      entries = entries.filter((e) => e.id !== a.id);
      return null;
    case "analyze_folder":
      return { games: [{ game_dir: a.path, title: "My Missed Game", title_source: "Folder name", root: a.path, candidates: [cand(`${a.path}\\MissedGame.exe`, 92, 25, ["+35 name matches the game folder"]), cand(`${a.path}\\Config.exe`, 30, 1, ["-20 looks like a settings tool"])], excluded: [], confidence: 92, uncertain: false }], excluded: [] };
    case "plan_import":
      return planImport();
    case "execute_import": {
      const p = planImport();
      const rec: RunRecord = { id: `i${Date.now()}`, kind: "import", at: now(), simulated: prefs.simulation_mode, scheduled: false, user_ids: ["48151623"], counts: p.summary, details: p.ops.map((o) => `Create ${o.new?.name}`), backup: prefs.simulation_mode ? null : "2026-11-04_223000-import", error: null };
      if (!prefs.simulation_mode) {
        p.ops.filter((o) => o.kind === "create").forEach((o) => set(o.entry_id!, { status: "imported" }));
        p.ops.filter((o) => o.kind === "update").forEach((o) => {
          set(o.entry_id!, { synced_title: o.new!.name });
          const sc = shortcuts.find((s) => s.name === o.old?.name);
          if (sc) sc.name = o.new!.name;
          if (sc) owner[o.new!.name] = o.entry_id!;
        });
        p.ops.filter((o) => o.kind === "remove").forEach((o) => {
          if (o.entry_id) set(o.entry_id, { status: "rejected", import: null });
          shortcuts.splice(shortcuts.findIndex((s) => s.name === o.old?.name), 1);
        });
        removals = new Set();
      }
      history = [rec, ...history];
      return { record: rec, deferred: false };
    }
    case "close_steam":
      return null;
    case "preview_rule":
      return entries.flatMap((e) => e.excluded.filter((x) => x.rule_id === a.rule.id).map((x) => ({ path: x.path, game: e.title })));
    case "validate_rule":
      if (a.kind === "regex") try { new RegExp(a.pattern); } catch { throw "Not a valid regular expression."; }
      return null;
    case "recommended_rules":
      return prefs.rules;
    case "rebuild_start":
      draft = { mode: a.mode, user_id: a.userId, started_at: now(), decisions: {}, approved: {} };
      return null;
    case "rebuild_draft":
      return draft;
    case "game_art": {
      // Sample artwork: the app logo stands in for a game's own icon.
      const icon = `${location.origin}/src/assets/logo.png`;
      return Object.fromEntries(entries.slice(0, 6).map((e) => [e.id, { cover: null, hero: null, logo: null, icon, source: "executable", match_name: null, match_chosen: false }]));
    }
    case "fetch_artwork":
    case "set_artwork_match":
      return { checked: 0, downloaded: 0, not_found: 0, failed: 0 };
    case "open_steamgriddb_key_page":
    case "check_steamgriddb_key":
      return null;
    case "search_artwork":
      return [1, 2, 3, 4, 5].map((id) => ({ id, name: id === 1 ? String(a.query) : `${String(a.query)} ${id}`, year: 2015 + id }));
    case "rebuild_inspect":
    case "inspect_account":
      return inspection();
    case "rebuild_set_decision":
      draft = { ...draft!, decisions: { ...draft!.decisions, [a.key]: a.decision } };
      return null;
    case "rebuild_approve": {
      const approved = { ...draft!.approved };
      for (const id of a.ids as string[]) a.approved ? (approved[id] = "fp") : delete approved[id];
      draft = { ...draft!, approved };
      return [];
    }
    case "rebuild_plan":
      return planRebuild();
    case "rebuild_execute": {
      const p = planRebuild();
      const rec: RunRecord = { id: `r${Date.now()}`, kind: "rebuild", at: now(), simulated: prefs.simulation_mode, scheduled: false, user_ids: ["48151623"], counts: p.summary, details: p.ops.filter((o) => o.kind !== "keep" && o.kind !== "unresolved").map((o) => `${o.kind}: ${o.old?.name ?? ""} → ${o.new?.name ?? ""}`), backup: "2026-11-04_223000-rebuild", error: null };
      history = [rec, ...history];
      return rec;
    }
    case "rebuild_cancel":
      draft = null;
      return null;
    case "list_backups":
      return backups;
    case "restore_preview":
      return { backup_id: a.id, warnings: [], token: "t", users: [{ user_id: "48151623", current_count: shortcuts.length, backup_count: 6, lost: [{ app_id: 1, name: "The Witcher 3: Wild Hunt", exe: "", start_dir: "", launch_options: "", icon: "" }, { app_id: 2, name: "Hollow Knight", exe: "", start_dir: "", launch_options: "", icon: "" }], regained: [] }] };
    case "restore_backup":
      return { id: "x", kind: "restore", at: now(), simulated: prefs.simulation_mode, scheduled: false, user_ids: [], counts: {}, details: [] };
    case "get_logs":
      return [
        { ts: ago(2), level: "info", event: "scan", message: "214 folders, 13 games, 4 new, 12 ignored executables" },
        { ts: ago(48), level: "info", event: "import", message: "2 change(s) written, backup 2026-11-02_201500-import" },
      ];
    case "diagnostics_report":
      return "BoilerSteamXL 1.0.0 diagnostic report\nSimulation mode: on (no Steam writes)\n\n[Steam]\nPath: C:\\Program Files (x86)\\Steam\nValid: true\nAccount …623: 8 shortcuts\n\n[Scan folders]\nD:\\Games (enabled: true, depth 6, reachable: true)\n\n(sample data)";
    case "schedule_preview":
      return a.schedule.mode === "manual" ? null : new Date(Date.now() + 86400000).toISOString();
    case "list_platform_settings":
      return [{ code_name: "epic_games", name: "Epic", settings: { enabled: true } }, { code_name: "gog", name: "GOG", settings: { enabled: true } }, { code_name: "itch", name: "Itch", settings: { enabled: false } }];
    case "load_settings":
      return { steamgrid_db: { enabled: true, auth_key: null }, steam: {} };
    case "update_settings":
      return { steamgrid_db: { enabled: true, auth_key: "set" }, steam: {} };
    case "list_steam_accounts":
      return [{ user_id: "48151623", shortcut_count: shortcuts.length }];
    case "list_artwork":
      return shortcuts.map((s, i) => ({ app_id: 3000000000 + i, name: s.name, from_boilr: s.boilr, images: {}, never_download: [] }));
    case "plugin:dialog|open":
      return a.options?.directory ? "D:\\Games\\My Missed Game" : "D:\\Games\\My Missed Game\\MissedGame.exe";
    case "plugin:dialog|save":
      return "C:\\Users\\Public\\export.json";
    case "plugin:event|listen":
      return 1;
    default:
      return null;
  }
});
