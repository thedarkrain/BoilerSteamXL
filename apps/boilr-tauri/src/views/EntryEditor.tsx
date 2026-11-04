// Entry details shared by Review & Approve and My Games: executable choice with the
// scoring explanation, editable fields, and the decision actions.
import clsx from "clsx";
import { useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../App";
import { api, displayTitle, type Entry } from "../api";
import { Callout, Chip, Cover, Field, Icon, Modal, StatusChip, errorText, fileName, fmtSize, useToast } from "../ui";

export const browseExe = async (start?: string): Promise<string | null> => {
  const picked = await open({ multiple: false, directory: false, defaultPath: start, filters: [{ name: "Programs", extensions: ["exe", "bat", "cmd", "lnk"] }] });
  return typeof picked === "string" ? picked : null;
};

export const CandidatePicker = ({ entry, onChanged }: { entry: Entry; onChanged: () => void }) => {
  const toast = useToast();
  const { prefs, savePrefs } = useApp();
  const [showExcluded, setShowExcluded] = useState(false);
  const [open_, setOpen] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const choose = async (exe: string) => {
    try {
      await api.editEntry(entry.id, { exe });
      onChanged();
    } catch (e) {
      toast("bad", errorText(e));
    }
  };
  const allowAndUse = async (exe: string) => {
    if (!prefs) return;
    if (!(await savePrefs({ ...prefs, rules: { ...prefs.rules, allowlist: [...prefs.rules.allowlist, exe] } }))) return;
    await choose(exe);
    toast("ok", `${fileName(exe)} is allowlisted and selected. It will not be excluded again.`);
  };
  const isChosen = (p: string) => p.toLowerCase().replace(/\//g, "\\") === entry.exe.toLowerCase().replace(/\//g, "\\");
  const custom = entry.exe && !entry.candidates.some((c) => isChosen(c.path));
  const shown = showAll ? entry.candidates : entry.candidates.filter((c, i) => isChosen(c.path) || (i === 0 && !entry.exe));
  const others = entry.candidates.length - shown.length;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="label">Executable</h3>
        <div className="flex gap-2">
          {entry.user_exe && entry.source === "folder" ? (
            <button type="button" className="link-btn" onClick={() => void api.resetOverride(entry.id).then(onChanged).catch((e) => toast("bad", errorText(e)))}>
              Reset to detected
            </button>
          ) : null}
          <button
            type="button"
            className="btn-ghost btn-sm"
            onClick={async () => {
              const p = await browseExe(entry.game_dir);
              if (p) await choose(p);
            }}
          >
            <Icon name="folder" size={13} /> Browse…
          </button>
        </div>
      </div>
      {custom ? (
        <div className="rounded-lg border border-accent/50 bg-accent/8 px-3 py-2.5">
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <Icon name="check" className="text-accent" /> {fileName(entry.exe)} <Chip tone="accent">your choice</Chip>
          </div>
          <p className="mono mt-1 truncate text-text-3" title={entry.exe}>
            {entry.exe}
          </p>
        </div>
      ) : null}
      {shown.map((c) => {
        const chosen = isChosen(c.path);
        const i = entry.candidates.indexOf(c);
        return (
          <div key={c.path} className={clsx("rounded-lg border px-3 py-2.5 transition", chosen ? "border-accent/60 bg-accent/8" : "border-line bg-surface-2 hover:border-line-strong")}>
            <div className="flex items-center gap-2">
              <button type="button" onClick={() => void choose(c.path)} className="flex min-w-0 flex-1 items-center gap-2 text-left" aria-pressed={chosen}>
                <span className={clsx("grid h-4 w-4 flex-none place-items-center rounded-full border-2", chosen ? "border-accent bg-accent" : "border-line-strong")}>
                  {chosen ? <span className="h-1.5 w-1.5 rounded-full bg-white" /> : null}
                </span>
                <span className="truncate text-[13px] font-semibold">{c.file_name}</span>
                {i === 0 ? <Chip tone="ok">Recommended</Chip> : null}
                {chosen && entry.user_exe ? <Chip tone="accent">your choice</Chip> : null}
              </button>
              <span className="text-xs text-text-3 tabular-nums">{fmtSize(c.size)}</span>
              <span className={clsx("w-12 text-right text-xs font-bold tabular-nums", c.score >= 75 ? "text-ok" : c.score >= 55 ? "text-warn" : "text-text-3")}>{c.score} pts</span>
              <button type="button" className="text-text-3 hover:text-text" onClick={() => setOpen(open_ === c.path ? null : c.path)} aria-label="Why">
                <Icon name="info" size={14} />
              </button>
            </div>
            <p className="mono mt-1 truncate pl-6 text-text-3" title={c.path}>
              {c.path}
            </p>
            {open_ === c.path ? (
              <ul className="mt-2 space-y-0.5 pl-6 text-xs text-text-2">
                {c.description ? <li>File description: “{c.description}”</li> : null}
                {c.reasons.map((r) => (
                  <li key={r} className="tabular-nums">
                    {r}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
        );
      })}
      {others > 0 || showAll ? (
        <button type="button" className="link-btn" onClick={() => setShowAll(!showAll)}>
          {showAll ? "Show only the selected executable" : `Wrong file? Show ${others} other executable${others === 1 ? "" : "s"}`}
        </button>
      ) : null}
      {entry.candidates.length === 0 && !entry.exe ? <Callout tone="warn">No usable executable was found. Browse for the game's executable.</Callout> : null}
      {entry.excluded.length ? (
        <div>
          <button type="button" className="link-btn" onClick={() => setShowExcluded(!showExcluded)}>
            {showExcluded ? "Hide" : "Show"} {entry.excluded.length} ignored executable{entry.excluded.length === 1 ? "" : "s"}
          </button>
          {showExcluded ? (
            <ul className="mt-2 space-y-1">
              {entry.excluded.map((x) => (
                <li key={x.path} className="flex items-center gap-2 rounded-md border border-line bg-bg-2 px-2.5 py-1.5 text-xs">
                  <Icon name="ban" className="text-text-3" size={13} />
                  <span className="min-w-0 flex-1">
                    <span className="font-semibold">{fileName(x.path)}</span> <span className="text-text-3">— {x.reason}</span>
                  </span>
                  <button type="button" className="link-btn !text-xs" onClick={() => void allowAndUse(x.path)}>
                    Allow & use
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </div>
  );
};

export const EntryForm = ({ entry, onChanged }: { entry: Entry; onChanged: () => void }) => {
  const toast = useToast();
  const { fetchArt, hasArtKey } = useApp();
  const [title, setTitle] = useState(displayTitle(entry));
  const [args, setArgs] = useState(entry.launch_options);
  const [dir, setDir] = useState(entry.start_dir);
  const [notes, setNotes] = useState(entry.notes);
  const [more, setMore] = useState(false);
  useEffect(() => {
    setTitle(displayTitle(entry));
    setNotes(entry.notes);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entry.id]);
  // Choosing another executable moves the start folder (and can change the options): follow it.
  useEffect(() => {
    setArgs(entry.launch_options);
    setDir(entry.start_dir);
  }, [entry.id, entry.launch_options, entry.start_dir]);
  const dirty = title !== displayTitle(entry) || args !== entry.launch_options || dir !== entry.start_dir || notes !== entry.notes;
  const save = async () => {
    try {
      await api.editEntry(entry.id, {
        title,
        ...(args !== entry.launch_options ? { launch_options: args } : {}),
        ...(dir !== entry.start_dir ? { start_dir: dir } : {}),
        notes,
      });
      // In Steam, its artwork is Steam's own (changed in the Artwork Manager); the name goes in with the next sync.
      const newName = title.trim() !== displayTitle(entry);
      const renamed = newName && !hasSteamShortcut(entry);
      toast(
        "ok",
        newName && hasSteamShortcut(entry)
          ? "Saved. Press Sync with Steam and Steam shows this name."
          : renamed && hasArtKey
            ? "Saved. Looking up artwork for the new name…"
            : "Saved. Your corrections are kept on every refresh."
      );
      onChanged();
      // A new name means a new SteamGridDB search (fetch_artwork notices the rename).
      if (renamed) void fetchArt([entry.id]);
    } catch (e) {
      toast("bad", errorText(e));
    }
  };
  return (
    <div className="space-y-3">
      <Field label="Game name" hint={<>Wrong name? Type the real one; artwork is searched with it.{entry.custom_title ? ` Detected: ${entry.title}` : ""}</>}>
        <input className="field" value={title} onChange={(e) => setTitle(e.target.value)} />
      </Field>
      <button type="button" className="link-btn" onClick={() => setMore(!more)}>
        {more ? "Fewer options" : "More options (launch options, start folder, notes)"}
      </button>
      {more ? (
        <>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Launch options" hint="Changing these needs a new approval.">
            <input className="field mono" value={args} onChange={(e) => setArgs(e.target.value)} placeholder="-windowed" />
          </Field>
          <Field label="Start in">
            <input className="field mono" value={dir} onChange={(e) => setDir(e.target.value)} />
          </Field>
        </div>
        <Field label="Notes">
          <textarea className="field min-h-14" value={notes} onChange={(e) => setNotes(e.target.value)} />
        </Field>
        </>
      ) : null}
      <div className="flex justify-end">
        <button type="button" className="btn-primary btn-sm" disabled={!dirty} onClick={() => void save()}>
          Save changes
        </button>
      </div>
    </div>
  );
};

export const DecisionPanel = ({ entry, onChanged }: { entry: Entry; onChanged: () => void }) => {
  const toast = useToast();
  const { steamGames } = useApp();
  const inSteam = steamGames.filter((g) => g.entry_id === entry.id);
  const removing = inSteam.some((g) => g.removal);
  const [linking, setLinking] = useState(false);
  const [confirmDup, setConfirmDup] = useState(false);
  useEffect(() => {
    setConfirmDup(false);
    setLinking(false);
  }, [entry.id]);
  const run = async (fn: () => Promise<unknown>, msg: string) => {
    try {
      const r = (await fn()) as { errors?: [string, string][] } | undefined;
      if (r?.errors?.length) toast("bad", r.errors.map((e) => e[1]).join(" "));
      else toast("ok", msg);
      onChanged();
    } catch (e) {
      toast("bad", errorText(e));
    }
  };
  const s = entry.status;
  const reviewable = ["needs_review", "possible_duplicate", "import_failed", "update_available", "approved"].includes(s);
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <StatusChip status={s} />
        {entry.source === "folder" ? (
          <span className="text-xs text-text-3">{entry.uncertain ? "Detection uncertain" : `Detection ${entry.confidence}%`}</span>
        ) : (
          <span className="text-xs text-text-3">{entry.launcher ?? "Added manually"}</span>
        )}
      </div>
      {entry.note ? <Callout tone="warn">{entry.note}</Callout> : null}
      {entry.linked && s === "already_in_steam" ? (
        <Callout tone="info" icon="steam">
          Already in Steam as “{entry.linked.name}”{entry.linked.boilr ? " (made by BoilR)" : ""}, so it is never added twice.
          <div className="mono mt-1 truncate text-text-3" title={entry.linked.path}>
            {entry.linked.path}
          </div>
        </Callout>
      ) : entry.duplicate ? (
        <Callout tone={entry.duplicate.confidence === "high" ? "info" : "warn"} icon="link">
          {entry.duplicate.reason}
          <div className="mono mt-1 truncate text-text-3" title={entry.duplicate.matched.path}>
            {entry.duplicate.matched.path}
          </div>
        </Callout>
      ) : null}
      {entry.import ? (
        <p className="text-xs text-text-3">
          In Steam as “{entry.import.name}” (account{entry.import.user_ids.length > 1 ? "s" : ""} {entry.import.user_ids.join(", ")}).
        </p>
      ) : null}

      <div className="grid grid-cols-2 gap-2">
        {reviewable && s !== "approved" ? (
          s === "possible_duplicate" ? (
            confirmDup ? (
              <button type="button" className="btn-primary col-span-2" onClick={() => void run(() => api.approve([entry.id], true), "It will be added as a separate shortcut when you sync.")}>
                <Icon name="check" /> Yes, add a separate shortcut
              </button>
            ) : (
              <button type="button" className="btn-ghost col-span-2" onClick={() => setConfirmDup(true)}>
                <Icon name="check" /> Add to Steam anyway…
              </button>
            )
          ) : (
            <button type="button" className="btn-primary col-span-2" disabled={!entry.exe} onClick={() => void run(() => api.approve([entry.id]), `${displayTitle(entry)} will be added when you sync.`)}>
              <Icon name="check" /> Add to Steam
            </button>
          )
        ) : null}
        {s === "approved" || s === "ready_to_import" ? (
          <button
            type="button"
            className="btn-ghost col-span-2"
            onClick={() => void (entry.import ? run(() => api.restoreEntries([entry.id]), "The change will not be made.") : run(() => api.reject([entry.id]), "It will not be added to Steam."))}
          >
            <Icon name="x" /> {entry.import ? "Don't change it in Steam" : "Don't add to Steam"}
          </button>
        ) : null}
        {!entry.import && !["rejected", "ignored"].includes(s) ? (
          <>
            <button type="button" className="btn-ghost" onClick={() => void run(() => api.reject([entry.id]), "It will not be added to Steam. Later scans keep it that way.")}>
              <Icon name="x" /> Don't add
            </button>
            <button type="button" className="btn-ghost" onClick={() => void run(() => api.ignore([entry.id]), "Ignored permanently. A rule was added.")}>
              <Icon name="ban" /> Ignore forever
            </button>
            <button type="button" className="btn-ghost col-span-2" onClick={() => setLinking(true)}>
              <Icon name="link" /> Already in Steam…
            </button>
          </>
        ) : null}
        {["rejected", "ignored", "already_in_steam"].includes(s) && !entry.import ? (
          <button type="button" className="btn-ghost col-span-2" onClick={() => void run(() => api.restoreEntries([entry.id]), s === "already_in_steam" ? "Unlinked. Check it before it is added." : "It is back on the list for Steam.")}>
            <Icon name="back" /> {s === "already_in_steam" ? "Not the same game: unlink" : "Add to Steam again"}
          </button>
        ) : null}
        {inSteam.length ? (
          <button
            type="button"
            className={clsx("col-span-2", removing ? "btn-ghost" : "btn-ghost !text-bad")}
            onClick={() => void run(() => api.setRemoval(inSteam.map((g) => g.key), !removing), removing ? "It stays in Steam." : "It will be removed from Steam when you sync. Its files are not touched.")}
          >
            <Icon name={removing ? "back" : "trash"} /> {removing ? "Keep in Steam" : "Remove from Steam"}
          </button>
        ) : null}
        {entry.source === "manual" && !entry.import ? (
          <button type="button" className="btn-ghost col-span-2 !text-bad" onClick={() => void run(() => api.removeManual(entry.id), "Removed from BoilerSteamXL.")}>
            <Icon name="trash" /> Remove this manual game
          </button>
        ) : null}
      </div>
      {linking ? <LinkDialog entry={entry} onClose={() => setLinking(false)} onLinked={onChanged} /> : null}
    </div>
  );
};

const LinkDialog = ({ entry, onClose, onLinked }: { entry: Entry; onClose: () => void; onLinked: () => void }) => {
  const toast = useToast();
  const [list, setList] = useState<{ key: string; user_id: string; name: string; exe: string }[] | null>(null);
  const [q, setQ] = useState(displayTitle(entry).split(" ")[0] ?? "");
  useEffect(() => {
    api.listShortcuts().then(setList).catch((e) => toast("bad", errorText(e)));
  }, [toast]);
  const shown = (list ?? []).filter((s) => `${s.name} ${s.exe}`.toLowerCase().includes(q.toLowerCase()));
  return (
    <Modal title={`Link “${displayTitle(entry)}” to a Steam shortcut`} onClose={onClose}>
      <input className="field mb-3" placeholder="Search shortcuts" value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
      <ul className="space-y-1.5">
        {shown.map((s) => (
          <li key={s.key}>
            <button
              type="button"
              className="w-full rounded-lg border border-line bg-surface-2 px-3 py-2 text-left hover:border-accent/60"
              onClick={async () => {
                try {
                  await api.linkEntry(entry.id, s.key);
                  toast("ok", `Linked to “${s.name}”.`);
                  onLinked();
                  onClose();
                } catch (e) {
                  toast("bad", errorText(e));
                }
              }}
            >
              <div className="text-[13px] font-semibold">
                {s.name} <span className="font-normal text-text-3">· account {s.user_id}</span>
              </div>
              <div className="mono truncate text-text-3">{s.exe}</div>
            </button>
          </li>
        ))}
        {list && shown.length === 0 ? <li className="text-text-3">No matching shortcuts.</li> : null}
      </ul>
    </Modal>
  );
};

/** A SteamGridDB game's cover, loaded on its own so search results appear at once. */
const MatchCover = ({ gameId, name }: { gameId: number; name: string }) => {
  const [src, setSrc] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    api.artworkThumb(gameId).then((s) => live && setSrc(s)).catch(() => live && setSrc(null));
    return () => {
      live = false;
    };
  }, [gameId]);
  return (
    <div className={clsx("aspect-[2/3] overflow-hidden rounded-lg border border-line transition group-hover:border-accent group-hover:shadow-lg", src === undefined && "animate-pulse bg-surface-3")}>
      {src === undefined ? null : <Cover title={name} src={src} />}
    </div>
  );
};

/** The game has a shortcut in Steam, whose artwork Steam keeps (BoilerSteamXL's cache is not used). */
const hasSteamShortcut = (entry: Entry) => Boolean(entry.import || entry.linked?.kind === "shortcut");

/** Which SteamGridDB game the artwork comes from, with a way to choose another. */
export const ArtworkPicker = ({ entry }: { entry: Entry }) => {
  const { art, hasArtKey, fetchArt, reloadArt, go } = useApp();
  const toast = useToast();
  const [open_, setOpen] = useState(false);
  const [query, setQuery] = useState(displayTitle(entry));
  const [results, setResults] = useState<{ id: number; name: string; year: number | null }[] | null>(null);
  const [busy, setBusy] = useState(false);
  const a = art[entry.id];
  const search = async (q = query) => {
    setBusy(true);
    try {
      setResults(await api.searchArtwork(q));
    } catch (e) {
      toast("bad", errorText(e));
    } finally {
      setBusy(false);
    }
  };
  if (!hasArtKey) {
    return (
      <p className="text-xs text-text-3">
        Artwork needs a SteamGridDB key.{" "}
        <button type="button" className="link-btn !text-xs" onClick={() => go("settings")}>
          Add it in Settings
        </button>
      </p>
    );
  }
  if (hasSteamShortcut(entry)) {
    return (
      <p className="text-xs text-text-3">
        This game is in Steam: change its artwork in{" "}
        <button type="button" className="link-btn !text-xs" onClick={() => go("artwork")}>
          More tools → Artwork Manager
        </button>
      </p>
    );
  }
  return (
    <div className="space-y-1.5 text-xs">
      <div className="flex items-center justify-between gap-2 text-text-3">
        <span className="truncate">
          Artwork: {a?.match ? `SteamGridDB · ${a.match}` : a?.cover ? a.source : "not found yet. Correct the game name above, or use Change…"}
        </span>
        <span className="flex gap-2">
          <button type="button" className="link-btn !text-xs" onClick={() => void fetchArt([entry.id], true)}>
            Refresh
          </button>
          <button
            type="button"
            className="link-btn !text-xs"
            onClick={() => {
              setOpen(true);
              setQuery(displayTitle(entry));
              setResults(null);
              void search(displayTitle(entry));
            }}
          >
            Change…
          </button>
        </span>
      </div>
      {open_ ? (
        <Modal title={`Artwork for “${displayTitle(entry)}”`} onClose={() => setOpen(false)} wide>
          <div className="mb-3 flex gap-2">
            <input className="field" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void search()} autoFocus />
            <button type="button" className="btn-primary" disabled={busy || !query.trim()} onClick={() => void search()}>
              <Icon name="search" /> Search
            </button>
          </div>
          <p className="mb-2 text-xs text-text-3">Pick the exact game and edition. Its cover, hero, logo and icon are downloaded and used for Steam when you sync.</p>
          <ul className="grid grid-cols-3 gap-3 sm:grid-cols-4">
            {(results ?? []).map((r) => (
              <li key={r.id}>
                <button
                  type="button"
                  disabled={busy}
                  className="group w-full text-left"
                  onClick={async () => {
                    setBusy(true);
                    try {
                      await api.setArtworkMatch(entry.id, r.id, r.name);
                      await reloadArt();
                      toast("ok", `Artwork from “${r.name}”.`);
                      setOpen(false);
                    } catch (e) {
                      toast("bad", errorText(e));
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  <MatchCover gameId={r.id} name={r.name} />
                  <span className="mt-1.5 block truncate text-[13px] font-semibold" title={r.name}>
                    {r.name}
                  </span>
                  <span className="block text-xs text-text-3">{r.year ?? "—"}</span>
                </button>
              </li>
            ))}
            {results && results.length === 0 ? <li className="col-span-full text-text-3">No games found. Try a shorter name.</li> : null}
          </ul>
        </Modal>
      ) : null}
    </div>
  );
};

/** Modal with everything about one game (used by My Games). */
export const EntryEditor = ({ entry, onClose, footerExtra }: { entry: Entry; onClose: () => void; footerExtra?: ReactNode }) => {
  const { reload, art } = useApp();
  const a = art[entry.id];
  const changed = () => void reload();
  return (
    <Modal title={displayTitle(entry)} onClose={onClose} wide footer={<div className="flex w-full items-center justify-between gap-2">{footerExtra}</div>}>
      {a?.hero ? (
        <div className="relative -mx-5 -mt-4 mb-4 h-20 overflow-hidden">
          <img src={a.hero} alt="" className="h-full w-full object-cover" />
          <div className="absolute inset-0 bg-gradient-to-t from-surface to-transparent" />
          {a.logo ? <img src={a.logo} alt="" className="absolute bottom-3 left-5 max-h-12 max-w-[40%] object-contain drop-shadow-xl" /> : null}
        </div>
      ) : null}
      <div className="grid gap-5 md:grid-cols-[minmax(0,1fr)_15rem]">
        <div className="min-w-0 space-y-4">
          <p className="mono truncate text-text-3" title={entry.game_dir}>
            {entry.game_dir}
          </p>
          <CandidatePicker entry={entry} onChanged={changed} />
          <EntryForm entry={entry} onChanged={changed} />
        </div>
        <div className="min-w-0 space-y-3">
          <div className="mx-auto aspect-[2/3] w-24 overflow-hidden rounded-xl border border-line shadow-lg">
            <Cover title={displayTitle(entry)} src={a?.cover} icon={a?.icon} />
          </div>
          <ArtworkPicker entry={entry} />
          <DecisionPanel entry={entry} onChanged={changed} />
        </div>
      </div>
    </Modal>
  );
};
