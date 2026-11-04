import clsx from "clsx";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../App";
import { api, type BackupManifest, type RestorePreview } from "../api";
import { Callout, Chip, Empty, Icon, Modal, PageHeader, errorText, fmtDate, plural, useToast } from "../ui";

type Tab = "backups" | "history" | "logs" | "diagnostics";

export const Backups = () => {
  const { library, info, reload } = useApp();
  const toast = useToast();
  const [tab, setTab] = useState<Tab>("backups");
  const [backups, setBackups] = useState<BackupManifest[] | null>(null);
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [logs, setLogs] = useState<Record<string, string>[]>([]);
  const [report, setReport] = useState("");

  const load = () => api.listBackups().then(setBackups).catch((e) => toast("bad", errorText(e)));
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (tab === "logs") void api.logs(300).then(setLogs);
    if (tab === "diagnostics") void api.diagnostics().then(setReport);
  }, [tab]);

  const restore = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      const r = await api.restoreBackup(preview.backup_id, preview.token);
      toast(r.simulated ? "warn" : "ok", r.simulated ? "Simulation: the restore was planned but Steam was not modified." : "Backup restored. Restart Steam to see the change.");
      setPreview(null);
      setConfirm(false);
      await reload();
      await load();
    } catch (e) {
      toast("bad", errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-5">
      <PageHeader
        title="Backups & History"
        subtitle="Every import, rebuild and restore is backed up first. Backups are never deleted automatically."
        actions={
          <button type="button" className="btn-ghost" onClick={() => void api.openDataFolder()}>
            <Icon name="folder" /> Open data folder
          </button>
        }
      />
      <div className="flex rounded-lg border border-line bg-surface-2 p-0.5 text-xs font-semibold w-fit">
        {(["backups", "history", "logs", "diagnostics"] as Tab[]).map((t) => (
          <button key={t} type="button" onClick={() => setTab(t)} className={clsx("rounded-md px-3 py-1.5 capitalize", tab === t ? "bg-surface-3 text-text" : "text-text-2")}>
            {t}
          </button>
        ))}
      </div>

      {tab === "backups" ? (
        backups && backups.length === 0 ? (
          <Empty icon="backups" title="No backups yet">
            A backup is created automatically before the first import or rebuild.
          </Empty>
        ) : (
          <ul className="space-y-2">
            {(backups ?? []).map((b) => (
              <li key={b.id} className="card flex flex-wrap items-center gap-3 p-4">
                <Icon name="backups" className="text-accent" />
                <div className="min-w-0 flex-1">
                  <div className="font-semibold">
                    {fmtDate(b.created)} <span className="mono text-xs text-text-3">{b.id}</span>
                  </div>
                  <div className="text-xs text-text-3">
                    Before {b.kind.replace(/-/g, " ")} · {b.users.map((u) => `account ${u.user_id}: ${plural(u.shortcuts.length, "shortcut")}${u.grid_files.length ? `, ${u.grid_files.length} artwork files` : ""}`).join(" · ")}
                  </div>
                </div>
                {b.verified ? <Chip tone="ok">verified</Chip> : <Chip tone="bad">unverified</Chip>}
                <button
                  type="button"
                  className="btn-ghost btn-sm"
                  onClick={async () => {
                    const dest = await open({ directory: true });
                    if (typeof dest === "string") await api.exportBackup(b.id, dest).then((p) => toast("ok", `Exported to ${p}`)).catch((e) => toast("bad", errorText(e)));
                  }}
                >
                  <Icon name="download" size={13} /> Export
                </button>
                <button
                  type="button"
                  className="btn-primary btn-sm"
                  disabled={!b.verified}
                  onClick={() => {
                    setConfirm(false);
                    void api.restorePreview(b.id).then(setPreview).catch((e) => toast("bad", errorText(e)));
                  }}
                >
                  Restore…
                </button>
              </li>
            ))}
          </ul>
        )
      ) : null}

      {tab === "history" ? (
        <ul className="space-y-2">
          {(library?.history ?? []).map((r) => (
            <li key={r.id} className="card p-4">
              <details>
                <summary className="flex cursor-pointer flex-wrap items-center gap-2">
                  <b className="capitalize">{r.kind}</b>
                  <span className="text-xs text-text-3">{fmtDate(r.at)}</span>
                  {r.simulated ? <Chip tone="warn">simulated</Chip> : null}
                  {r.scheduled ? <Chip>scheduled</Chip> : null}
                  {r.error ? <Chip tone="bad">failed</Chip> : <Chip tone="ok">ok</Chip>}
                  {r.user_ids.length ? <span className="text-xs text-text-3">account {r.user_ids.join(", ")}</span> : null}
                  <span className="ml-auto text-xs text-text-3">
                    {Object.entries(r.counts)
                      .filter(([, v]) => v)
                      .map(([k, v]) => `${k.replace(/_/g, " ")} ${v}`)
                      .join(" · ")}
                  </span>
                </summary>
                {r.error ? <Callout tone="bad">{r.error}</Callout> : null}
                {r.backup ? <p className="mt-2 text-xs">Backup: <span className="mono">{r.backup}</span></p> : null}
                <ul className="mono mt-2 space-y-0.5 text-text-3">
                  {r.details.map((d, i) => (
                    <li key={i}>{d}</li>
                  ))}
                </ul>
              </details>
            </li>
          ))}
          {!library?.history.length ? <Empty icon="backups" title="No history yet" /> : null}
        </ul>
      ) : null}

      {tab === "logs" ? (
        <div className="panel mono scroll-quiet max-h-[65vh] overflow-y-auto p-3">
          {logs.map((l, i) => (
            <div key={i} className={clsx("py-0.5", l.level === "error" ? "text-bad" : l.level === "warn" ? "text-warn" : "text-text-2")}>
              <span className="text-text-3">{l.ts}</span> [{l.event}] {l.message}
            </div>
          ))}
          {logs.length === 0 ? <p className="text-text-3">The log is empty.</p> : null}
        </div>
      ) : null}

      {tab === "diagnostics" ? (
        <div className="space-y-3">
          <p className="text-[13px] text-text-2">Personal details are masked: your Windows user name, Steam account ids and API keys are never included.</p>
          <pre className="panel mono scroll-quiet max-h-[60vh] overflow-auto p-4 whitespace-pre-wrap">{report || "Generating…"}</pre>
          <button type="button" className="btn-ghost" onClick={() => void navigator.clipboard.writeText(report).then(() => toast("ok", "Copied."))}>
            Copy report
          </button>
          <p className="text-xs text-text-3">Data folder: <span className="mono">{info?.data_dir}</span></p>
        </div>
      ) : null}

      {preview ? (
        <Modal
          title="Restore this backup?"
          // A running restore cannot be stopped: the dialog stays until it ends.
          onClose={() => {
            if (busy) return;
            setPreview(null);
            setConfirm(false);
          }}
          wide
          footer={
            <>
              <label className="mr-auto flex items-center gap-2 text-[13px]">
                <input type="checkbox" className="check" checked={confirm} onChange={(e) => setConfirm(e.target.checked)} /> I reviewed what will be lost and want to restore.
              </label>
              <button
                type="button"
                className="btn-ghost"
                disabled={busy}
                onClick={() => {
                  setPreview(null);
                  setConfirm(false);
                }}
              >
                Cancel
              </button>
              <button type="button" className="btn-danger" disabled={!confirm || busy} onClick={() => void restore()}>
                {busy ? "Restoring…" : "Restore Previous Steam Library"}
              </button>
            </>
          }
        >
          <div className="space-y-4">
            {preview.warnings.map((w) => (
              <Callout key={w} tone="warn">
                {w}
              </Callout>
            ))}
            <p className="text-[13px] text-text-2">The current state is backed up first, so this restore can be undone too. Steam must be closed.</p>
            {preview.users.map((u) => (
              <div key={u.user_id} className="grid gap-3 md:grid-cols-2">
                <div className="panel p-3">
                  <h3 className="mb-1 font-semibold text-bad">
                    Lost from account {u.user_id} ({u.lost.length})
                  </h3>
                  <p className="mb-2 text-xs text-text-3">In Steam now ({u.current_count}), not in the backup.</p>
                  <ul className="space-y-0.5 text-[13px]">{u.lost.map((s) => <li key={s.app_id + s.exe}>{s.name}</li>)}</ul>
                </div>
                <div className="panel p-3">
                  <h3 className="mb-1 font-semibold text-ok">Brought back ({u.regained.length})</h3>
                  <p className="mb-2 text-xs text-text-3">In the backup ({u.backup_count}), not in Steam now.</p>
                  <ul className="space-y-0.5 text-[13px]">{u.regained.map((s) => <li key={s.app_id + s.exe}>{s.name}</li>)}</ul>
                </div>
              </div>
            ))}
          </div>
        </Modal>
      ) : null}
    </div>
  );
};
