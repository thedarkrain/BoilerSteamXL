import { useEffect, useState } from "react";
import { useApp } from "../App";
import { api, type Inspection, type SteamInfo } from "../api";
import { Callout, Chip, Empty, Icon, PageHeader, errorText, fileName, useToast } from "../ui";

/** The account's current non-Steam shortcuts, with duplicates and broken entries marked. */
export const SteamLibrary = () => {
  const { go } = useApp();
  const toast = useToast();
  const [steam, setSteam] = useState<SteamInfo | null>(null);
  const [user, setUser] = useState<string | null>(null);
  const [data, setData] = useState<Inspection | null>(null);

  useEffect(() => {
    api.steamInfo().then((s) => {
      setSteam(s);
      setUser(s.users[0]?.user_id ?? null);
    }).catch((e) => toast("bad", errorText(e)));
  }, [toast]);
  useEffect(() => {
    if (user) api.inspectAccount(user).then(setData).catch((e) => toast("bad", errorText(e)));
  }, [user, toast]);

  const c = data?.counts ?? {};
  return (
    <div className="space-y-5">
      <PageHeader
        title="Steam Library"
        subtitle="Your non-Steam shortcuts as Steam has them now. Read-only here; change them with New Games or Clean Rebuild."
        actions={
          <button type="button" className="btn-primary" onClick={() => go("rebuild")}>
            <Icon name="rebuild" /> Clean Rebuild…
          </button>
        }
      />
      {steam && !steam.valid ? <Callout tone="bad">{steam.error ?? "Steam was not found."} Check Settings → Steam.</Callout> : null}
      {steam?.valid ? (
        <div className="card flex flex-wrap items-center gap-4 p-4 text-[13px]">
          <Icon name="steam" className="text-accent" />
          <span className="mono">{steam.path}</span>
          <Chip tone={steam.running ? "ok" : "muted"}>{steam.running ? "Running" : "Closed"}</Chip>
          <span className="text-text-3">{steam.official_count} official games installed (never modified)</span>
          {steam.users.length > 1 ? (
            <select className="field ml-auto w-auto" value={user ?? ""} onChange={(e) => setUser(e.target.value)} aria-label="Steam account">
              {steam.users.map((u) => (
                <option key={u.user_id} value={u.user_id}>
                  {u.persona ?? u.user_id} ({u.shortcut_count})
                </option>
              ))}
            </select>
          ) : null}
        </div>
      ) : null}
      {data?.error ? <Callout tone="bad">{data.error}</Callout> : null}
      {data ? (
        <div className="flex flex-wrap gap-2 text-xs">
          <Chip>{c.total ?? 0} non-Steam shortcuts</Chip>
          <Chip tone="accent">{c.managed ?? 0} by BoilerSteamXL</Chip>
          <Chip tone="info">{c.boilr ?? 0} by BoilR</Chip>
          <Chip>{c.other ?? 0} other / manual</Chip>
          <Chip tone="warn">{c.duplicates ?? 0} in duplicate groups</Chip>
          <Chip tone="bad">{c.broken ?? 0} broken</Chip>
          <Chip tone="warn">{c.utilities ?? 0} utilities</Chip>
        </div>
      ) : null}
      {data && data.shortcuts.length === 0 ? (
        <Empty icon="steam" title="No non-Steam shortcuts">
          Approve games and import them to see them here.
        </Empty>
      ) : null}
      {data && data.shortcuts.length ? (
        <div className="panel overflow-hidden">
          <table className="w-full text-[13px]">
            <thead className="bg-surface-2 text-left text-xs text-text-3">
              <tr>
                <th className="px-4 py-2.5 font-semibold">Shortcut</th>
                <th className="px-4 py-2.5 font-semibold">Target</th>
                <th className="px-4 py-2.5 font-semibold">Origin</th>
                <th className="px-4 py-2.5 font-semibold">Health</th>
              </tr>
            </thead>
            <tbody>
              {data.shortcuts.map((s) => (
                <tr key={s.key} className="border-t border-line">
                  <td className="px-4 py-2.5">
                    <div className="font-semibold">{s.shortcut.name}</div>
                    <div className="text-xs text-text-3">Steam ID {s.shortcut.app_id}{s.artwork.length ? ` · ${s.artwork.length} artwork files` : ""}</div>
                  </td>
                  <td className="px-4 py-2.5">
                    <div className="mono truncate text-text-2" title={s.shortcut.exe}>
                      {fileName(s.shortcut.exe)}
                    </div>
                    {s.shortcut.launch_options ? <div className="mono text-xs text-text-3">{s.shortcut.launch_options}</div> : null}
                  </td>
                  <td className="px-4 py-2.5">
                    <Chip tone={s.origin === "BoilerSteamXL" ? "accent" : s.origin === "BoilR" ? "info" : "muted"}>{s.origin}</Chip>
                  </td>
                  <td className="space-x-1 px-4 py-2.5">
                    {s.broken ? <Chip tone="bad">Executable missing</Chip> : null}
                    {s.duplicate_group !== null ? <Chip tone="warn">Duplicate</Chip> : null}
                    {s.utility ? <Chip tone="warn" title={s.utility}>Utility</Chip> : null}
                    {!s.broken && s.duplicate_group === null && !s.utility ? <Chip tone="ok">OK</Chip> : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </div>
  );
};
