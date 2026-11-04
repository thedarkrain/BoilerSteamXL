import clsx from "clsx";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import logo from "./assets/logo.png";
import { api, pendingRename, syncEligible, type AppInfo, type ArtProgress, type GameArt, type LibraryView, type Preferences, type ScanProgress, type ScanSummary, type SteamGame } from "./api";
import { toImageSrc } from "./lib/format";
import { Icon, ToastProvider, errorText, plural, useToast } from "./ui";
import { Dashboard } from "./views/Dashboard";
import { Games } from "./views/Games";
import { Review } from "./views/Review";
import { Discover } from "./views/Discover";
import { SteamLibrary } from "./views/SteamLibrary";
import { Rebuild } from "./views/Rebuild";
import { Rules } from "./views/Rules";
import { Automation } from "./views/Automation";
import { Backups } from "./views/Backups";
import { SettingsView } from "./views/Settings";
import { FirstRun } from "./views/FirstRun";
import { AddGame } from "./views/AddGame";
import { ImportDialog } from "./views/ImportDialog";
import { ArtworkView } from "./components/ArtworkView";
import type { Settings } from "./settings";
import { invoke } from "@tauri-apps/api/core";

export type View =
  | "dashboard"
  | "games"
  | "review"
  | "discover"
  | "steam"
  | "rebuild"
  | "artwork"
  | "rules"
  | "automation"
  | "backups"
  | "settings";

type NavItem = { view: View; label: string; icon: string };
/** All most people need: their games (on this PC and in Steam) and settings. */
const NAV: NavItem[] = [
  { view: "games", label: "My Games", icon: "games" },
  { view: "settings", label: "Settings", icon: "settings" },
];
/** Everything else, folded away under "More tools". */
const TOOLS: NavItem[] = [
  { view: "review", label: "Review queue", icon: "review" },
  { view: "dashboard", label: "Overview", icon: "dashboard" },
  { view: "discover", label: "Discover Games", icon: "search" },
  { view: "steam", label: "Steam Library", icon: "steam" },
  { view: "artwork", label: "Artwork Manager", icon: "artwork" },
  { view: "rebuild", label: "Clean Rebuild", icon: "rebuild" },
  { view: "rules", label: "Exclusion Rules", icon: "rules" },
  { view: "automation", label: "Automation", icon: "automation" },
  { view: "backups", label: "Backups & History", icon: "backups" },
];

type AppContextValue = {
  info: AppInfo | null;
  prefs: Preferences | null;
  library: LibraryView | null;
  go: (view: View) => void;
  reload: () => Promise<void>;
  savePrefs: (prefs: Preferences) => Promise<boolean>;
  refresh: () => Promise<ScanSummary | null>;
  scanning: boolean;
  openAddGame: () => void;
  openImport: () => void;
  /** Non-Steam shortcuts in Steam now. */
  steamGames: SteamGame[];
  /** Adds the clear-cut new games and opens the sync confirmation. */
  startSync: () => Promise<void>;
  /** Artwork per entry id, ready to use as image sources. */
  art: Record<string, { cover: string | null; icon: string | null; hero: string | null; logo: string | null; source: string | null; match: string | null }>;
  reloadArt: () => Promise<void>;
  fetchArt: (ids?: string[] | null, force?: boolean) => Promise<void>;
  artProgress: ArtProgress | null;
  hasArtKey: boolean;
};

const AppContext = createContext<AppContextValue | null>(null);
export const useApp = () => {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside provider");
  return ctx;
};

const Shell = () => {
  const toast = useToast();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [prefs, setPrefs] = useState<Preferences | null>(null);
  const [library, setLibrary] = useState<LibraryView | null>(null);
  const [view, setViewState] = useState<View>(() => ([...NAV, ...TOOLS].some((n) => `#${n.view}` === location.hash) ? (location.hash.slice(1) as View) : "games"));
  const [moreOpen, setMoreOpen] = useState(false);
  const setView = useCallback((v: View) => {
    history.replaceState(null, "", `#${v}`);
    setViewState(v);
  }, []);
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [boilrSettings, setBoilrSettings] = useState<Settings | null>(null);
  const [startupDone, setStartupDone] = useState(false);
  const [rawArt, setRawArt] = useState<Record<string, GameArt>>({});
  const [artProgress, setArtProgress] = useState<ArtProgress | null>(null);
  const hasArtKey = Boolean(boilrSettings?.steamgrid_db?.auth_key?.trim());

  const reloadArt = useCallback(async () => {
    try {
      setRawArt(await api.gameArt());
    } catch {
      /* artwork is optional */
    }
  }, []);
  const fetchArt = useCallback(
    async (ids: string[] | null = null, force = false) => {
      if (!hasArtKey) return;
      try {
        await api.fetchArtwork(ids, force);
      } catch (e) {
        toast("bad", errorText(e));
      } finally {
        setArtProgress(null);
        await reloadArt();
      }
    },
    [hasArtKey, reloadArt, toast]
  );
  const art = useMemo(() => {
    const out: AppContextValue["art"] = {};
    for (const [id, a] of Object.entries(rawArt)) {
      // The version makes a re-downloaded image (same file name) load again instead of the cached one.
      const v = a.version ?? undefined;
      out[id] = { cover: toImageSrc(a.cover, v), icon: toImageSrc(a.icon, v), hero: toImageSrc(a.hero, v), logo: toImageSrc(a.logo, v), source: a.source, match: a.match_name };
    }
    return out;
  }, [rawArt]);

  const [steamGames, setSteamGames] = useState<SteamGame[]>([]);
  const reload = useCallback(async () => {
    const [i, p, l, g] = await Promise.all([api.appInfo(), api.getPrefs(), api.library(), api.steamGames().catch(() => [] as SteamGame[])]);
    setInfo(i);
    setPrefs(p);
    setLibrary(l);
    setSteamGames(g);
  }, []);

  const lastArtReload = useRef(0);
  useEffect(() => {
    reload().catch((e) => toast("bad", errorText(e)));
    invoke<Settings>("load_settings").then(setBoilrSettings).catch(() => undefined);
    const stop = listen<ScanProgress>("scan-progress", (e) => setProgress(e.payload));
    const stopArt = listen<ArtProgress>("artwork-progress", (e) => {
      setArtProgress(e.payload.done >= e.payload.total ? null : e.payload);
      // Covers show up as they arrive, but at most every few seconds (each reload reads Steam).
      if (Date.now() - lastArtReload.current > 3000) {
        lastArtReload.current = Date.now();
        void reloadArt();
      }
    });
    void reloadArt();
    return () => {
      void stop.then((f) => f());
      void stopArt.then((f) => f());
    };
  }, [reload, toast, reloadArt]);

  // Download missing artwork in the background whenever the library changes.
  const entryCount = library?.entries.length ?? 0;
  useEffect(() => {
    if (hasArtKey && prefs?.download_artwork && prefs.first_run_complete && entryCount > 0) void fetchArt();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasArtKey, prefs?.download_artwork, prefs?.first_run_complete, entryCount, library?.last_scan?.at]);

  useEffect(() => {
    document.documentElement.dataset.theme = prefs?.theme === "light" ? "light" : "dark";
  }, [prefs?.theme]);

  const savePrefs = useCallback(
    async (next: Preferences) => {
      try {
        setPrefs(await api.savePrefs(next));
        setInfo(await api.appInfo());
        return true;
      } catch (e) {
        toast("bad", errorText(e));
        return false;
      }
    },
    [toast]
  );

  const refresh = useCallback(async () => {
    if (scanning) return null;
    setScanning(true);
    setProgress(null);
    try {
      const result = await api.refresh();
      setSummary(result);
      await reload();
      return result;
    } catch (e) {
      toast("bad", errorText(e));
      return null;
    } finally {
      setScanning(false);
      setProgress(null);
    }
  }, [scanning, reload, toast]);

  // "Scan on startup" (refresh = discovery only; nothing is imported).
  useEffect(() => {
    if (!startupDone && prefs?.first_run_complete && prefs.schedule.scan_on_startup) {
      setStartupDone(true);
      void refresh();
    }
  }, [prefs, startupDone, refresh]);

  const pending = (library?.counts.needs_review ?? 0) + (library?.counts.possible_duplicate ?? 0) + (library?.counts.update_available ?? 0);
  const toAdd = (library?.entries ?? []).filter((e) => e.status === "approved" || e.status === "ready_to_import" || syncEligible(e)).length;
  const renames = (library?.entries ?? []).filter((e) => !(e.status === "approved" || e.status === "ready_to_import") && pendingRename(e, steamGames)).length;
  const changes = toAdd + renames + steamGames.filter((g) => g.removal).length;

  const startSync = useCallback(async () => {
    // The clear-cut new games the user left in are approved now; the confirmation shows every change.
    if (library?.entries.some(syncEligible)) {
      try {
        await api.approveEligible();
      } catch (e) {
        toast("warn", errorText(e));
      }
      await reload();
    }
    setImportOpen(true);
  }, [library, reload, toast]);

  const ctx = useMemo<AppContextValue>(
    () => ({
      info,
      prefs,
      library,
      go: setView,
      reload,
      savePrefs,
      refresh,
      scanning,
      openAddGame: () => setAddOpen(true),
      openImport: () => setImportOpen(true),
      steamGames,
      startSync,
      art,
      reloadArt,
      fetchArt,
      artProgress,
      hasArtKey,
    }),
    [info, prefs, library, reload, savePrefs, refresh, scanning, steamGames, startSync, art, reloadArt, fetchArt, artProgress, hasArtKey]
  );

  if (!info || !prefs) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4">
        <img src={logo} alt="" className="h-16 w-16 animate-pulse" />
        <p className="text-text-2">Starting BoilerSteamXL…</p>
      </div>
    );
  }

  if (!prefs.first_run_complete) {
    return (
      <AppContext.Provider value={ctx}>
        <FirstRun />
      </AppContext.Provider>
    );
  }

  return (
    <AppContext.Provider value={ctx}>
      <div className="flex h-full">
        <aside className="flex w-60 flex-none flex-col border-r border-line bg-bg-2">
          <div className="flex items-center gap-2.5 px-5 pt-5 pb-4">
            <img src={logo} alt="" className="h-8 w-8 rounded-lg" />
            <div className="leading-tight">
              <div className="font-display text-[15px] font-bold">
                BoilerSteam<span className="gradient-text">XL</span>
              </div>
              <div className="text-[11px] text-text-3">v{info.version}</div>
            </div>
          </div>
          <nav aria-label="Main" className="scroll-quiet flex-1 overflow-y-auto px-3 pb-3">
            {[...NAV, ...(moreOpen || TOOLS.some((t) => t.view === view) ? TOOLS : [])].map((item, i) => (
              <div key={item.view}>
                {i === NAV.length ? <div className="label mt-4 mb-1.5 px-2.5">More tools</div> : null}
                <button
                  type="button"
                  onClick={() => setView(item.view)}
                  aria-current={view === item.view ? "page" : undefined}
                  className={clsx(
                    "group relative mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] font-medium transition-colors",
                    view === item.view ? "bg-surface-3 text-text" : "text-text-2 hover:bg-surface-2 hover:text-text"
                  )}
                >
                  {view === item.view ? <span className="absolute top-2 bottom-2 left-0 w-[3px] rounded-full bg-accent" /> : null}
                  <Icon name={item.icon} className={view === item.view ? "text-accent" : ""} />
                  <span className="flex-1 text-left">{item.label}</span>
                  {item.view === "review" && pending > 0 ? (
                    <span className="rounded-full bg-accent px-1.5 text-[11px] font-bold text-white">{pending}</span>
                  ) : null}
                </button>
              </div>
            ))}
            {!TOOLS.some((t) => t.view === view) ? (
              <button type="button" className="mt-3 w-full px-2.5 text-left text-xs text-text-3 hover:text-text" onClick={() => setMoreOpen(!moreOpen)}>
                {moreOpen ? "Hide more tools" : "More tools…"}
              </button>
            ) : null}
          </nav>
          <div className="space-y-2 border-t border-line p-3">
            {info.simulation ? (
              <button type="button" onClick={() => setView("settings")} className="flex w-full items-center gap-2 rounded-lg border border-warn/30 bg-warn/10 px-2.5 py-2 text-left text-xs text-warn">
                <Icon name="shield" />
                <span>
                  <b>Simulation mode</b>
                  <br />
                  <span className="text-text-2">Steam is never modified</span>
                </span>
              </button>
            ) : null}
            <div className="flex items-center gap-2 px-1 text-xs text-text-3">
              <span className={clsx("h-2 w-2 rounded-full", info.steam_running ? "bg-ok" : "bg-text-3")} />
              Steam {info.steam_running ? "is running" : "is closed"}
              {info.mode !== "installed" ? <span className="ml-auto">Portable</span> : null}
            </div>
          </div>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex items-center gap-3 border-b border-line bg-bg/80 px-6 py-3 backdrop-blur">
            {scanning ? (
              <div className="flex min-w-0 flex-1 items-center gap-3">
                <div className="h-1.5 w-40 overflow-hidden rounded-full bg-surface-3">
                  <div className="progress-indeterminate h-full w-full" />
                </div>
                <span className="truncate text-xs text-text-2">
                  {progress ? `${plural(progress.folders_scanned, "folder")} scanned · ${plural(progress.exes_found, "executable")} · ${progress.current}` : "Scanning your game folders…"}
                </span>
                <button type="button" className="btn-ghost btn-sm" onClick={() => void api.cancelRefresh()}>
                  Cancel
                </button>
              </div>
            ) : (
              <div className="flex min-w-0 flex-1 items-center gap-2 text-xs text-text-3">
                {library?.last_scan ? <>Last refresh {new Date(library.last_scan.at).toLocaleString()}</> : <>Not refreshed yet</>}
                {artProgress ? (
                  <span className="ml-3 flex items-center gap-2 text-text-2">
                    <Icon name="artwork" className="animate-pulse text-accent" />
                    Artwork {artProgress.done}/{artProgress.total} · {artProgress.current}
                  </span>
                ) : null}
              </div>
            )}
            <button type="button" className="btn-ghost" onClick={() => setAddOpen(true)}>
              <Icon name="plus" /> Add Game
            </button>
            <button type="button" className="btn-ghost" onClick={() => void refresh()} disabled={scanning}>
              <Icon name="refresh" className={scanning ? "animate-spin" : ""} /> {scanning ? "Scanning…" : "Scan for games"}
            </button>
            <button type="button" className="btn-primary" onClick={() => void startSync()} disabled={scanning || changes === 0} title={changes ? undefined : "Nothing to change in Steam"}>
              <Icon name="steam" /> Sync with Steam{changes ? ` (${changes})` : ""}
            </button>
          </header>

          <main className="scroll-quiet min-h-0 flex-1 overflow-y-auto">
            {view === "artwork" ? (
              <ArtworkView settings={boilrSettings} />
            ) : (
              <div key={view} className="animate-in mx-auto max-w-[1500px] px-6 py-6">
                {view === "dashboard" && <Dashboard lastSummary={summary} />}
                {view === "games" && <Games />}
                {view === "review" && <Review />}
                {view === "discover" && <Discover />}
                {view === "steam" && <SteamLibrary />}
                {view === "rebuild" && <Rebuild />}
                {view === "rules" && <Rules />}
                {view === "automation" && <Automation />}
                {view === "backups" && <Backups />}
                {view === "settings" && <SettingsView onKeySaved={(s) => setBoilrSettings(s)} />}
              </div>
            )}
          </main>
        </div>
      </div>
      {addOpen ? <AddGame onClose={() => setAddOpen(false)} /> : null}
      {importOpen ? <ImportDialog onClose={() => setImportOpen(false)} /> : null}
    </AppContext.Provider>
  );
};

const App = () => (
  <ToastProvider>
    <Shell />
  </ToastProvider>
);

export default App;
