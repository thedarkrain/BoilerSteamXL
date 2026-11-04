import clsx from "clsx";
import { useMemo, useState } from "react";
import { useApp } from "../App";
import { STEAMGRIDDB_KEY_PAGE, SYNC_LABEL, api, displayTitle, pendingRename, samePath, syncState, type Entry, type SteamGame, type SyncState } from "../api";
import { Callout, Chip, Cover, Empty, Icon, Modal, PageHeader, errorText, useToast, type Tone } from "../ui";
import { EntryEditor } from "./EntryEditor";
import { MergeDialog } from "./MergeDialog";

/** One tile: a library game (maybe also in Steam), or a Steam shortcut no library game owns. */
export type Item = { id: string; title: string; sub: string; state: SyncState; entry?: Entry; steam: SteamGame[] };

const TONE: Record<SyncState, Tone> = { in_steam: "info", rename: "accent", add: "ok", check: "warn", duplicate: "warn", maybe: "warn", remove: "bad", skipped: "muted", missing: "bad" };
type Filter = "all" | "steam" | "add" | "check" | "skipped";
const FILTERS: { key: Filter; label: string; match: (s: SyncState) => boolean }[] = [
  { key: "all", label: "All", match: (s) => s !== "skipped" },
  { key: "steam", label: "In Steam", match: (s) => s === "in_steam" || s === "rename" || s === "remove" || s === "duplicate" },
  { key: "add", label: "Will be added", match: (s) => s === "add" },
  { key: "check", label: "Check these", match: (s) => s === "check" || s === "missing" || s === "duplicate" || s === "maybe" },
  { key: "skipped", label: "Not added", match: (s) => s === "skipped" },
];
const SOURCE_LABEL = { folder: "On this PC", manual: "Added by you", launcher: "Launcher" } as const;

export const Games = () => {
  const { library, reload, art, hasArtKey, steamGames, go } = useApp();
  const toast = useToast();
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [merging, setMerging] = useState<string | null>(null);

  // One tile per game: shortcuts that start the same program are the same game, whatever their names.
  const items = useMemo<Item[]>(() => {
    const byPath = new Map<string, SteamGame[]>();
    for (const g of steamGames) byPath.set(samePath(g.exe), [...(byPath.get(samePath(g.exe)) ?? []), g]);
    const claimed = new Set<string>();
    const stateOf = (base: SyncState, steam: SteamGame[]) => {
      const staying = steam.filter((g) => !g.removal).length;
      if (staying > 1) return "duplicate";
      if (steam.length && staying === 0) return "remove";
      return base;
    };
    const out: Item[] = (library?.entries ?? []).flatMap((e): Item[] => {
      const state0 = syncState(e, false);
      const base = state0 === "in_steam" && pendingRename(e, steamGames) ? "rename" : state0;
      const own = steamGames.filter((g) => g.entry_id === e.id);
      // A game you chose not to add does not hide the Steam shortcuts that share its program.
      const same = e.exe && base !== "skipped" ? byPath.get(samePath(e.exe)) ?? [] : [];
      const steam = [...new Map([...own, ...same].map((g) => [g.key, g])).values()];
      // Gone from this PC and not in Steam: nothing to do, so nothing to show.
      if (base === "missing" && steam.length === 0) return [];
      steam.forEach((g) => claimed.add(g.key));
      return [{ id: e.id, title: displayTitle(e), sub: e.launcher ?? SOURCE_LABEL[e.source], state: stateOf(steam.length && base === "check" ? "in_steam" : base, steam), entry: e, steam }];
    });
    for (const g of steamGames) {
      if (claimed.has(g.key)) continue;
      const group = (byPath.get(samePath(g.exe)) ?? [g]).filter((x) => !claimed.has(x.key));
      group.forEach((x) => claimed.add(x.key));
      const sub = group.length > 1 ? `${group.length} Steam shortcuts` : g.exe_exists ? "Steam shortcut" : "Steam shortcut · file missing";
      out.push({ id: `steam:${g.key}`, title: g.name, sub, state: stateOf("in_steam", group), steam: group });
    }
    return out.sort((a, b) => a.title.localeCompare(b.title));
  }, [library, steamGames]);

  const count = (f: Filter) => items.filter((i) => FILTERS.find((x) => x.key === f)!.match(i.state)).length;
  const shown = useMemo(() => {
    const f = FILTERS.find((x) => x.key === filter) ?? FILTERS[0];
    const q = query.trim().toLowerCase();
    return items.filter((i) => f.match(i.state) && (!q || `${i.title} ${i.entry?.exe ?? i.steam[0]?.exe ?? ""}`.toLowerCase().includes(q)));
  }, [items, filter, query]);

  const act = async (item: Item, fn: () => Promise<unknown>) => {
    setBusy(item.id);
    try {
      await fn();
      await reload();
    } catch (e) {
      toast("bad", errorText(e));
    } finally {
      setBusy(null);
    }
  };

  /** The one thing each tile offers; everything else is in its details. */
  const quick = (i: Item): { label: string; run: () => void } | null => {
    const keys = i.steam.map((g) => g.key);
    switch (i.state) {
      case "add":
        return i.entry ? { label: "Don't add", run: () => void act(i, () => api.reject([i.entry!.id])) } : null;
      case "skipped":
        return i.entry ? { label: "Add to Steam", run: () => void act(i, () => api.restoreEntries([i.entry!.id])) } : null;
      case "in_steam":
      case "rename":
        return keys.length ? { label: "Remove from Steam", run: () => void act(i, () => api.setRemoval(keys, true)) } : null;
      case "remove":
        return { label: "Keep in Steam", run: () => void act(i, () => api.setRemoval(keys, false)) };
      case "missing":
        return keys.length ? { label: "Remove from Steam", run: () => void act(i, () => api.setRemoval(keys, true)) } : null;
      case "duplicate":
      case "maybe":
        return { label: "Merge…", run: () => setMerging(i.id) };
      default:
        return { label: "Check", run: () => setSelected(i.id) };
    }
  };

  const current = items.find((i) => i.id === selected) ?? null;
  const toMerge = items.filter((i) => i.state === "duplicate" || i.state === "maybe");
  const mergeItem = items.find((i) => i.id === merging) ?? null;

  return (
    <div className="space-y-5">
      <PageHeader
        title="My Games"
        subtitle={
          <>
            Your games on this PC and in Steam. Change, add or remove what you like, then press <b className="text-text">Sync with Steam</b>.
          </>
        }
      />
      {toMerge.length ? (
        <Callout tone="warn" icon="link">
          <b>{toMerge.length === 1 ? `“${toMerge[0].title}” is` : `${toMerge.length} games are`} already in Steam</b> (same program, maybe under another name). Nothing is added twice.{" "}
          <button type="button" className="link-btn" onClick={() => setMerging(toMerge[0].id)}>
            Choose how to merge
          </button>
        </Callout>
      ) : null}
      {!hasArtKey ? (
        <Callout tone="info" icon="artwork">
          For covers and icons,{" "}
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              void api.openKeyPage();
              go("settings");
            }}
          >
            get your free SteamGridDB key
          </button>{" "}
          (<span className="mono">{STEAMGRIDDB_KEY_PAGE}</span>) and copy it. BoilerSteamXL picks it up and saves it.
        </Callout>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative w-64">
          <Icon name="search" className="absolute top-2.5 left-3 text-text-3" />
          <input className="field pl-9" placeholder="Search" aria-label="Search games" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <div className="flex flex-wrap rounded-lg border border-line bg-surface-2 p-0.5">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              type="button"
              aria-pressed={filter === f.key}
              onClick={() => setFilter(f.key)}
              className={clsx("rounded-md px-2.5 py-1.5 text-xs font-semibold", filter === f.key ? "bg-surface-3 text-text" : "text-text-2 hover:text-text")}
            >
              {f.label} <span className="text-text-3">{count(f.key)}</span>
            </button>
          ))}
        </div>
      </div>

      {shown.length === 0 ? (
        <Empty icon="games" title="No games here">
          Press “Scan for games” to look in your game folders, or add one with “Add Game”.
        </Empty>
      ) : (
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-5">
          {shown.map((i) => {
            const a = art[i.entry ? i.id : `steam:${i.steam[0]?.key}`];
            const q = quick(i);
            return (
              <li key={i.id} className={clsx("group", (i.state === "skipped" || i.state === "remove") && "opacity-60 hover:opacity-100")}>
                <button
                  type="button"
                  onClick={() => (i.state === "duplicate" || i.state === "maybe" ? setMerging(i.id) : setSelected(i.id))}
                  className="block w-full text-left"
                  aria-label={`${i.title}: ${SYNC_LABEL[i.state]}`}
                >
                  <div className="relative aspect-[2/3] overflow-hidden rounded-xl border border-line shadow-lg transition duration-200 group-hover:-translate-y-1 group-hover:border-accent/60">
                    <Cover title={i.title} src={a?.cover} icon={a?.icon} />
                    <div className="absolute top-2 left-2 rounded-full bg-black/60 backdrop-blur-sm">
                      <Chip tone={TONE[i.state]}>{SYNC_LABEL[i.state]}</Chip>
                    </div>
                  </div>
                  <p className="mt-2 truncate font-semibold" title={i.title}>
                    {i.title}
                  </p>
                  <p className="truncate text-xs text-text-3">{i.sub}</p>
                </button>
                {q ? (
                  <button type="button" className="link-btn mt-0.5 !text-xs" disabled={busy === i.id} onClick={q.run}>
                    {q.label}
                  </button>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}

      {mergeItem ? (
        <MergeDialog
          item={mergeItem}
          onClose={() => setMerging(null)}
          onDone={async () => {
            await reload();
            // Straight on to the next game that needs merging, if any.
            const next = toMerge.find((x) => x.id !== mergeItem.id);
            setMerging(next ? next.id : null);
          }}
        />
      ) : null}
      {current?.entry ? (
        <EntryEditor
          entry={current.entry}
          onClose={() => setSelected(null)}
          footerExtra={
            <div className="flex gap-1.5">
              {current.entry.import ? (
                <button type="button" className="btn-ghost btn-sm" onClick={() => void api.launchInSteam(current.entry!.import!.app_id).catch((x) => toast("bad", errorText(x)))}>
                  <Icon name="play" size={13} /> Play
                </button>
              ) : null}
              <button type="button" className="btn-ghost btn-sm" onClick={() => void api.reveal(current.entry!.exe || current.entry!.game_dir).catch((x) => toast("bad", errorText(x)))}>
                <Icon name="folder" size={13} /> Open folder
              </button>
            </div>
          }
        />
      ) : current ? (
        <SteamShortcutDialog item={current} onClose={() => setSelected(null)} onChange={(remove) => void act(current, () => api.setRemoval(current.steam.map((g) => g.key), remove))} />
      ) : null}
    </div>
  );
};

/** A shortcut that is in Steam but was not made from one of your scanned or added games. */
const SteamShortcutDialog = ({ item, onClose, onChange }: { item: Item; onClose: () => void; onChange: (remove: boolean) => void }) => {
  const { art } = useApp();
  const toast = useToast();
  const g = item.steam[0];
  const a = art[`steam:${g.key}`];
  return (
    <Modal
      title={item.title}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn-ghost" onClick={() => void api.reveal(g.exe).catch((x) => toast("bad", errorText(x)))} disabled={!g.exe_exists}>
            <Icon name="folder" /> Open folder
          </button>
          {g.removal ? (
            <button type="button" className="btn-primary" onClick={() => onChange(false)}>
              Keep in Steam
            </button>
          ) : (
            <button type="button" className="btn-danger" onClick={() => onChange(true)}>
              <Icon name="trash" /> Remove from Steam
            </button>
          )}
        </>
      }
    >
      <div className="flex gap-4">
        <div className="aspect-[2/3] w-24 flex-none overflow-hidden rounded-lg border border-line">
          <Cover title={item.title} src={a?.cover} icon={a?.icon} />
        </div>
        <div className="min-w-0 space-y-2 text-[13px]">
          <p className="text-text-2">This shortcut is in Steam already{g.exe_exists ? "" : ", but its program no longer exists"}.</p>
          <p className="mono break-all text-text-3">{g.exe}</p>
          {g.removal ? <Callout tone="warn">It will be removed from Steam when you sync. The game's files are not touched.</Callout> : null}
        </div>
      </div>
    </Modal>
  );
};
