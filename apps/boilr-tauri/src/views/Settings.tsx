import { useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useApp } from "../App";
import { STEAMGRIDDB_KEY_PAGE, api, keyPage, looksLikeArtKey, type Preferences, type SteamInfo } from "../api";
import type { Settings } from "../settings";
import { Callout, Chip, Field, Icon, Modal, PageHeader, Toggle, errorText, useToast } from "../ui";

const Section = ({ title, icon, children, subtitle }: { title: string; icon: string; subtitle?: string; children: ReactNode }) => (
  <section className="card p-5">
    <h2 className="h2 flex items-center gap-2">
      <Icon name={icon} className="text-accent" /> {title}
    </h2>
    {subtitle ? <p className="mt-0.5 text-xs text-text-3">{subtitle}</p> : null}
    <div className="mt-4 space-y-3">{children}</div>
  </section>
);

const Row = ({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) => (
  <div className="flex items-center justify-between gap-6">
    <div>
      <div className="text-[13px] font-semibold">{label}</div>
      {hint ? <div className="text-xs text-text-3">{hint}</div> : null}
    </div>
    {children}
  </div>
);

export const GameFolders = ({ prefs, onChange }: { prefs: Preferences; onChange: (p: Preferences) => void }) => (
  <div className="space-y-2">
    {prefs.scan_roots.map((r, i) => (
      <div key={i} className="flex items-center gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2">
        <Toggle checked={r.enabled} label={`Scan ${r.path}`} onChange={(v) => onChange({ ...prefs, scan_roots: prefs.scan_roots.map((x, j) => (j === i ? { ...x, enabled: v } : x)) })} />
        <input className="field mono flex-1 py-1.5" value={r.path} onChange={(e) => onChange({ ...prefs, scan_roots: prefs.scan_roots.map((x, j) => (j === i ? { ...x, path: e.target.value } : x)) })} />
        <label className="flex items-center gap-1 text-xs text-text-2">
          Depth
          <input type="number" min={1} max={12} className="field w-16 py-1.5" value={r.max_depth} onChange={(e) => onChange({ ...prefs, scan_roots: prefs.scan_roots.map((x, j) => (j === i ? { ...x, max_depth: Number(e.target.value) } : x)) })} />
        </label>
        <button type="button" className="btn-ghost btn-sm" onClick={() => onChange({ ...prefs, scan_roots: prefs.scan_roots.filter((_, j) => j !== i) })} aria-label="Remove folder">
          <Icon name="trash" size={13} />
        </button>
      </div>
    ))}
    <button
      type="button"
      className="btn-ghost"
      onClick={async () => {
        const p = await open({ directory: true });
        if (typeof p === "string" && !prefs.scan_roots.some((r) => r.path.toLowerCase() === p.toLowerCase())) onChange({ ...prefs, scan_roots: [...prefs.scan_roots, { path: p, enabled: true, max_depth: 8 }] });
      }}
    >
      <Icon name="plus" /> Add game folder
    </button>
  </div>
);

export const SettingsView = ({ onKeySaved }: { onKeySaved?: (s: Settings) => void }) => {
  const { prefs, savePrefs, info, reload } = useApp();
  const toast = useToast();
  const [draft, setDraft] = useState<Preferences | null>(prefs);
  const [steam, setSteam] = useState<SteamInfo | null>(null);
  const [steamPath, setSteamPath] = useState(prefs?.steam_path ?? "");
  const [askSim, setAskSim] = useState(false);
  const [platforms, setPlatforms] = useState<{ code_name: string; name: string; settings: Record<string, unknown> }[]>([]);
  const [grid, setGrid] = useState<Settings | null>(null);
  const [key, setKey] = useState("");
  // Other settings save at once; unsaved game folders and the feed URL stay as typed.
  useEffect(() => setDraft((d) => (d && prefs ? { ...prefs, scan_roots: d.scan_roots, update_url: d.update_url } : prefs)), [prefs]);
  useEffect(() => {
    api.steamInfo().then(setSteam).catch(() => undefined);
    api.platformSettings().then(setPlatforms).catch(() => undefined);
    invoke<Settings>("load_settings").then((s) => {
      setGrid(s);
      setKey(s.steamgrid_db?.auth_key ?? "");
    }).catch(() => undefined);
  }, [prefs?.steam_path]);
  const hasKey = Boolean(grid?.steamgrid_db?.auth_key?.trim());
  const saveKey = async (value: string) => {
    try {
      if (value.trim()) await api.checkArtKey(value);
      const saved = await invoke<Settings>("update_settings", { update: { steamgrid_db: { auth_key: value.trim(), enabled: true } } });
      setGrid(saved);
      setKey(value.trim());
      onKeySaved?.(saved);
      // A removed key stays removed (not taken again from another BoilR config at startup).
      if (prefs && prefs.steamgriddb_key_removed !== !value.trim()) await savePrefs({ ...prefs, steamgriddb_key_removed: !value.trim() });
      toast("ok", value.trim() ? "SteamGridDB key saved. Downloading artwork for your games." : "SteamGridDB key removed.");
    } catch (e) {
      toast("bad", errorText(e));
    }
  };
  const saveKeyRef = useRef(saveKey);
  saveKeyRef.current = saveKey;
  const tried = useRef("");
  // Back from the SteamGridDB page with the key copied: take it from the clipboard (each value once).
  useEffect(() => {
    if (hasKey) return;
    const onFocus = () => {
      if (!keyPage.opened) return;
      void navigator.clipboard
        ?.readText()
        .then((text) => {
          const value = text.trim();
          if (!looksLikeArtKey(value) || value === tried.current) return;
          tried.current = value;
          return saveKeyRef.current(value);
        })
        .catch(() => undefined);
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [hasKey]);
  if (!draft || !prefs) return null;
  const dirtyFolders = JSON.stringify(draft.scan_roots) !== JSON.stringify(prefs.scan_roots);

  return (
    <div className="space-y-5">
      <PageHeader title="Settings" />
      <div className="grid gap-5 xl:grid-cols-2">
        <Section title="Safety" icon="shield" subtitle="Protects your real Steam library.">
          <Row label="Simulation mode" hint="Imports, rebuilds, restores and artwork changes are planned and reported, but nothing is written to Steam.">
            <Toggle checked={prefs.simulation_mode} label="Simulation mode" onChange={(v) => (v ? void savePrefs({ ...prefs, simulation_mode: true }) : setAskSim(true))} />
          </Row>
          {prefs.simulation_mode ? <Callout tone="warn">Simulation mode is on. Your Steam files are only read.</Callout> : <Callout tone="ok">Writing is on. Every write still needs your approval, a confirmed plan and a verified backup.</Callout>}
        </Section>

        <Section title="Steam" icon="steam" subtitle="Three different kinds of folder; don't mix them up.">
          <Field label="1 · Steam installation folder" hint="Where steam.exe and userdata live, usually C:\Program Files (x86)\Steam. Leave empty to detect it.">
            <div className="flex gap-2">
              <input className="field mono" value={steamPath} placeholder={steam?.path} onChange={(e) => setSteamPath(e.target.value)} />
              <button
                type="button"
                className="btn-ghost"
                onClick={async () => {
                  const p = await open({ directory: true });
                  if (typeof p === "string") setSteamPath(p);
                }}
              >
                Browse
              </button>
              <button
                type="button"
                className="btn-primary"
                onClick={async () => {
                  try {
                    if (steamPath.trim()) await api.checkSteamPath(steamPath.trim());
                    if (await savePrefs({ ...prefs, steam_path: steamPath.trim() || null })) toast("ok", "Steam folder saved.");
                  } catch (e) {
                    toast("bad", errorText(e));
                  }
                }}
              >
                Save
              </button>
            </div>
          </Field>
          {steam ? steam.valid ? <Callout tone="ok">Steam found at <span className="mono">{steam.path}</span>.</Callout> : <Callout tone="bad">{steam.error}</Callout> : null}
          <div>
            <span className="mb-1 block text-xs font-semibold text-text-2">2 · Steam library folders (read from Steam; official games, never modified)</span>
            <ul className="mono space-y-0.5 text-text-3">{steam?.library_folders.map((f) => <li key={f}>{f}</li>)}</ul>
          </div>
          <div>
            <span className="mb-1 block text-xs font-semibold text-text-2">Steam accounts to write to</span>
            {steam?.users.map((u) => (
              <label key={u.user_id} className="flex items-center gap-2 py-0.5 text-[13px]">
                <input
                  type="checkbox"
                  className="check"
                  checked={prefs.target_users.length === 0 || prefs.target_users.includes(u.user_id)}
                  onChange={(e) => {
                    const all = steam.users.map((x) => x.user_id);
                    const known = prefs.target_users.filter((id) => all.includes(id));
                    const current = known.length ? known : all;
                    const next = e.target.checked ? [...current, u.user_id] : current.filter((x) => x !== u.user_id);
                    if (next.length === 0) {
                      toast("warn", "Keep at least one Steam account selected.");
                      return;
                    }
                    void savePrefs({ ...prefs, target_users: next.length === all.length ? [] : next });
                  }}
                />
                {u.persona ?? u.user_id} <span className="text-text-3">({u.shortcut_count} shortcuts)</span>
                {u.error ? <Chip tone="bad">damaged: never written</Chip> : null}
              </label>
            ))}
          </div>
        </Section>

        <Section title="3 · Game folders to scan" icon="folder" subtitle="Your non-Steam games, e.g. D:\Games. Not the Steam folder.">
          <GameFolders prefs={draft} onChange={setDraft} />
          <div className="flex justify-end">
            <button type="button" className="btn-primary" disabled={!dirtyFolders} onClick={() => void savePrefs(draft).then((ok) => ok && toast("ok", "Game folders saved. Refresh to scan them."))}>
              Save folders
            </button>
          </div>
        </Section>

        <Section title="Launcher integrations" icon="games" subtitle="From BoilR: also queue games from Epic, GOG Galaxy, itch, Xbox and others. They go through review like everything else.">
          <Row label="Include launcher games when refreshing">
            <Toggle checked={prefs.include_launchers} label="Include launchers" onChange={(v) => void savePrefs({ ...prefs, include_launchers: v })} />
          </Row>
          {prefs.include_launchers ? (
            <div className="grid grid-cols-2 gap-2">
              {platforms.map((p) => (
                <label key={p.code_name} className="flex items-center gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2 text-[13px]">
                  <input
                    type="checkbox"
                    className="check"
                    checked={Boolean(p.settings.enabled)}
                    onChange={async (e) => {
                      await api.setPlatformEnabled(p.code_name, e.target.checked).catch((x) => toast("bad", errorText(x)));
                      setPlatforms(await api.platformSettings());
                    }}
                  />
                  {p.name}
                </label>
              ))}
            </div>
          ) : null}
        </Section>

        <Section title="SteamGridDB artwork" icon="artwork" subtitle="Optional. Without a key, scanning and importing work normally.">
          {!hasKey ? (
            <Callout tone="info" icon="artwork">
              Sign in at SteamGridDB (your Steam login works), then copy the key from{" "}
              <button type="button" className="link-btn mono break-all" onClick={() => void api.openKeyPage()}>
                {STEAMGRIDDB_KEY_PAGE}
              </button>
              . Come back here and it is picked up from the clipboard and saved.
            </Callout>
          ) : null}
          <Field label="API key" hint="Saved as soon as a valid key is pasted. Never written to logs.">
            <div className="flex gap-2">
              <input
                className="field mono"
                type="password"
                value={key}
                onChange={(e) => {
                  setKey(e.target.value);
                  if (looksLikeArtKey(e.target.value)) void saveKey(e.target.value);
                }}
                placeholder="Paste your key"
              />
              <button type="button" className="btn-ghost shrink-0" onClick={() => void api.openKeyPage()}>
                <Icon name="external" /> Get my free key
              </button>
              <button type="button" className="btn-primary" onClick={() => void saveKey(key)}>
                Save
              </button>
            </div>
          </Field>
          <Row label="Download artwork for my games" hint="Covers, heroes, logos and icons for every game in the background; copied into Steam when you import.">
            <Toggle checked={prefs.download_artwork} label="Download artwork" onChange={(v) => void savePrefs({ ...prefs, download_artwork: v })} />
          </Row>
          {grid?.steamgrid_db?.auth_key ? <p className="text-xs text-ok">A key is configured.</p> : null}
        </Section>

        <Section title="Appearance" icon="sparkle">
          <Row label="Theme">
            <div className="flex rounded-lg border border-line bg-surface-2 p-0.5 text-xs font-semibold">
              {(["dark", "light"] as const).map((t) => (
                <button key={t} type="button" onClick={() => void savePrefs({ ...prefs, theme: t })} className={`rounded-md px-3 py-1.5 capitalize ${prefs.theme === t ? "bg-surface-3 text-text" : "text-text-2"}`}>
                  {t}
                </button>
              ))}
            </div>
          </Row>
        </Section>

        <Section title="Updates" icon="download" subtitle="Optional. Checks a release feed and tells you; nothing is installed without you.">
          <Field label="Release feed (GitHub latest-release API URL)">
            <input className="field mono" value={draft.update_url} onChange={(e) => setDraft({ ...draft, update_url: e.target.value })} placeholder="https://api.github.com/repos/<owner>/<repo>/releases/latest" />
          </Field>
          <div className="flex gap-2">
            <button type="button" className="btn-ghost" onClick={() => void savePrefs({ ...prefs, update_url: draft.update_url })}>
              Save
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={() =>
                void api
                  .checkUpdate()
                  .then((u) => toast(u.available ? "warn" : "ok", u.available ? `Version ${u.latest} is available: ${u.url}` : `You have the latest version (${u.current}).`))
                  .catch((e) => toast("bad", errorText(e)))
              }
            >
              Check now
            </button>
          </div>
        </Section>

        <Section title="Backup & transfer settings" icon="backups">
          <p className="text-xs text-text-3">
            Edition: <b>{info?.mode}</b> · data in <span className="mono">{info?.data_dir}</span>
          </p>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              className="btn-ghost"
              onClick={async () => {
                const p = await save({ defaultPath: "BoilerSteamXL-settings.json", filters: [{ name: "Settings", extensions: ["json"] }] });
                if (p) await api.exportJson(p, "settings").then(() => toast("ok", "Settings exported.")).catch((e) => toast("bad", errorText(e)));
              }}
            >
              <Icon name="download" /> Export settings, rules & manual games
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={async () => {
                const p = await open({ filters: [{ name: "Settings", extensions: ["json"] }] });
                if (typeof p === "string") {
                  await api.importJson(p, "settings").then(() => toast("ok", "Settings imported. Imported games wait for review.")).catch((e) => toast("bad", errorText(e)));
                  await reload();
                }
              }}
            >
              <Icon name="upload" /> Import
            </button>
            <button type="button" className="btn-ghost" onClick={() => void api.openDataFolder()}>
              <Icon name="folder" /> Open data folder
            </button>
          </div>
        </Section>
      </div>
      <p className="text-center text-xs text-text-3">
        BoilerSteamXL {info?.version} is based on BoilR by Philip Kristoffersen and contributors (MIT / Apache-2.0). Not affiliated with Valve or the BoilR project.
      </p>

      {askSim ? (
        <Modal
          title="Allow BoilerSteamXL to write to Steam?"
          onClose={() => setAskSim(false)}
          footer={
            <>
              <button type="button" className="btn-ghost" onClick={() => setAskSim(false)}>
                Keep simulation on
              </button>
              <button
                type="button"
                className="btn-danger"
                onClick={async () => {
                  if (await savePrefs({ ...prefs, simulation_mode: false })) toast("ok", "Writing to Steam is enabled.");
                  setAskSim(false);
                }}
              >
                Turn off simulation
              </button>
            </>
          }
        >
          <ul className="space-y-2 text-[13px] text-text-2">
            <li>• Confirmed imports, rebuilds, restores and artwork changes will modify Steam's shortcut files.</li>
            <li>• Every write still needs approved games, a final confirmation of the exact plan, Steam closed, and a verified backup.</li>
            <li>• Game files and saves are never touched.</li>
          </ul>
        </Modal>
      ) : null}
    </div>
  );
};
