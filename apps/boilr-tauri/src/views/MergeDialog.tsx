// The merge notice: one game is in Steam more than once (the same program, whatever the
// names), or may already be there. The user picks what stays; removals happen at the next sync.
import { useState } from "react";
import { useApp } from "../App";
import { api, displayTitle, type SteamGame } from "../api";
import { Callout, Chip, Cover, Icon, Modal, errorText, plural, useToast } from "../ui";
import type { Item } from "./Games";

type Choice = { id: string; name: string; origin: string; exe: string; start: string; args: string; appId?: number; cover?: string | null; icon?: string | null; steam?: SteamGame };

export const MergeDialog = ({ item, onClose, onDone }: { item: Item; onClose: () => void; onDone: () => Promise<void> }) => {
  const { art, steamGames } = useApp();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const e = item.entry;

  // "Maybe": Steam has a shortcut to another program in this game's folder.
  const matched = e?.duplicate?.matched;
  const maybe = item.state === "maybe";
  const steam = maybe ? steamGames.filter((g) => g.user_id === matched?.user_id && g.app_id === matched?.app_id) : item.steam;
  const choices: Choice[] = steam.map((g) => {
    const a = art[g.entry_id ?? `steam:${g.key}`];
    return {
      id: g.key,
      name: g.name,
      origin: g.made_by ? `In Steam · made by ${g.made_by}` : "In Steam",
      exe: g.exe,
      start: g.start_dir,
      args: g.launch_options,
      appId: g.app_id,
      cover: a?.cover,
      icon: a?.icon,
      steam: g,
    };
  });
  // Your scanned/added copy, when it is not itself one of the Steam shortcuts.
  if (e && !e.import && e.exe) {
    choices.push({ id: e.id, name: displayTitle(e), origin: "On this PC (not in Steam yet)", exe: e.exe, start: e.start_dir, args: e.launch_options, cover: art[e.id]?.cover, icon: art[e.id]?.icon });
  }
  const same = new Set(choices.map((c) => c.exe.trim().toLowerCase())).size === 1;

  const run = async (fn: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await fn();
      toast("ok", done);
      await onDone();
    } catch (x) {
      toast("bad", errorText(x));
    } finally {
      setBusy(false);
    }
  };

  const keep = (c: Choice) => {
    const others = steam.filter((g) => g.key !== c.id).map((g) => g.key);
    return run(async () => {
      if (others.length) await api.setRemoval(others, true);
      if (c.steam) {
        await api.setRemoval([c.steam.key], false);
        // Your game now points at the copy that stays, so it is never added again.
        if (e && c.steam.entry_id !== e.id) await api.linkEntry(e.id, c.steam.key);
      } else if (e) {
        // Keep your copy: it replaces the Steam ones at the next sync.
        if (e.status !== "needs_review") await api.restoreEntries([e.id]);
        const r = await api.approve([e.id], true);
        if (r.errors.length) throw r.errors.map((x) => x[1]).join(" ");
      }
    }, others.length ? `“${c.name}” stays; ${plural(others.length, "other copy", "other copies")} will be removed when you sync.` : `“${c.name}” is kept.`);
  };

  return (
    <Modal title={maybe ? `Is “${item.title}” already in Steam?` : `“${item.title}” is in Steam ${choices.filter((c) => c.steam).length} times`} onClose={onClose} wide footer={<button type="button" className="btn-ghost" onClick={onClose}>Decide later</button>}>
      <div className="space-y-4">
        <Callout tone="info" icon="link">
          {maybe ? (
            <>A Steam shortcut already starts a program in this game's folder. Keep one, or keep both if they really are different games.</>
          ) : (
            <>
              {same ? "They all start the same program" : "They start the same game"}, so they are the same game even with different names. Pick the one to keep. The others are removed from Steam when you sync;
              game files are never touched.
            </>
          )}
        </Callout>
        <ul className="grid gap-3" style={{ gridTemplateColumns: `repeat(${Math.min(choices.length, 3)}, minmax(0, 1fr))` }}>
          {choices.map((c) => (
            <li key={c.id} className="flex flex-col rounded-xl border border-line bg-surface-2 p-3">
              <div className="flex gap-3">
                <div className="aspect-[2/3] w-16 flex-none overflow-hidden rounded-lg border border-line">
                  <Cover title={c.name} src={c.cover} icon={c.icon} />
                </div>
                <div className="min-w-0">
                  <p className="truncate font-semibold" title={c.name}>
                    {c.name}
                  </p>
                  <Chip tone={c.steam ? "info" : "accent"}>{c.origin}</Chip>
                  {c.steam?.removal ? <p className="mt-1 text-xs text-bad">Marked for removal</p> : null}
                </div>
              </div>
              <dl className="mono mt-3 grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5 text-[11px]">
                <dt className="text-text-3">Program</dt>
                <dd className="break-all text-text-2">{c.exe}</dd>
                <dt className="text-text-3">Start in</dt>
                <dd className="break-all text-text-2">{c.start.replace(/"/g, "") || "—"}</dd>
                <dt className="text-text-3">Options</dt>
                <dd className="break-all text-text-2">{c.args || "—"}</dd>
                {c.appId ? (
                  <>
                    <dt className="text-text-3">Steam ID</dt>
                    <dd className="text-text-2">{c.appId}</dd>
                  </>
                ) : null}
              </dl>
              <button type="button" className="btn-primary btn-sm mt-3 self-start" disabled={busy} onClick={() => void keep(c)}>
                <Icon name="check" size={13} /> {c.steam ? "Keep this one" : "Use mine instead"}
              </button>
            </li>
          ))}
        </ul>
        {maybe && e ? (
          <button type="button" className="btn-ghost" disabled={busy} onClick={() => void run(() => api.approve([e.id], true), `“${item.title}” will be added as a separate game when you sync.`)}>
            They are different games: keep both
          </button>
        ) : null}
      </div>
    </Modal>
  );
};
