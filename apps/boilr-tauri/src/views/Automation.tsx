import clsx from "clsx";
import { useEffect, useState } from "react";
import { useApp } from "../App";
import { api, type Schedule } from "../api";
import { Callout, Chip, Field, Icon, PageHeader, Toggle, fmtDate } from "../ui";

const DAYS = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
const MODES: { mode: Schedule["mode"]; label: string; text: string }[] = [
  { mode: "manual", label: "Manual only", text: "Refresh when you press the button." },
  { mode: "daily", label: "Daily", text: "Every day at a set time." },
  { mode: "weekly", label: "Weekly", text: "On the days you choose." },
  { mode: "custom", label: "Custom", text: "Every few days." },
];

export const Automation = () => {
  const { prefs, savePrefs, library } = useApp();
  const [s, setS] = useState<Schedule | null>(prefs?.schedule ?? null);
  const [next, setNext] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setS(prefs?.schedule ?? null), [prefs]);
  useEffect(() => {
    if (!s) return;
    api
      .schedulePreview(s)
      .then((n) => {
        setNext(n);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, [s]);
  if (!prefs || !s) return null;
  const dirty = JSON.stringify(s) !== JSON.stringify(prefs.schedule);
  const scheduled = (library?.history ?? []).filter((r) => r.kind === "scan" && r.scheduled);

  return (
    <div className="space-y-5">
      <PageHeader title="Automation" subtitle="Refresh means discovering changes. Import only ever happens when you approve and confirm it." />
      <div className="grid gap-5 xl:grid-cols-[1.4fr_1fr]">
        <div className="card space-y-5 p-5">
          <div className="grid gap-2 md:grid-cols-4">
            {MODES.map((m) => (
              <button
                key={m.mode}
                type="button"
                onClick={() => setS({ ...s, mode: m.mode })}
                className={clsx("rounded-lg border p-3 text-left", s.mode === m.mode ? "border-accent bg-accent/8" : "border-line bg-surface-2 hover:border-line-strong")}
              >
                <div className="font-semibold">{m.label}</div>
                <div className="text-xs text-text-3">{m.text}</div>
              </button>
            ))}
          </div>
          {s.mode !== "manual" ? (
            <div className="grid gap-4 md:grid-cols-3">
              <Field label="Time">
                <input type="time" className="field" value={s.time} onChange={(e) => setS({ ...s, time: e.target.value })} />
              </Field>
              {s.mode === "custom" ? (
                <Field label="Every (days)">
                  <input type="number" min={1} max={365} className="field" value={s.interval_days} onChange={(e) => setS({ ...s, interval_days: Number(e.target.value) })} />
                </Field>
              ) : null}
              {s.mode === "weekly" ? (
                <div className="md:col-span-2">
                  <span className="mb-1 block text-xs font-semibold text-text-2">Days</span>
                  <div className="flex flex-wrap gap-1">
                    {DAYS.map((d) => (
                      <button
                        key={d}
                        type="button"
                        onClick={() => setS({ ...s, days: s.days.includes(d) ? s.days.filter((x) => x !== d) : [...s.days, d] })}
                        className={clsx("rounded-md border px-2.5 py-1.5 text-xs font-semibold capitalize", s.days.includes(d) ? "border-accent bg-accent/15" : "border-line text-text-2")}
                      >
                        {d.slice(0, 3)}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
          <div className="space-y-3">
            {[
              ["run_missed", "Run a missed refresh when the PC is back on", "Windows starts it as soon as possible after a missed time."],
              ["scan_on_startup", "Refresh when BoilerSteamXL opens", "Discovery only; nothing is imported."],
            ].map(([key, label, hint]) => (
              <div key={key} className="flex items-center justify-between gap-4">
                <div>
                  <div className="text-[13px] font-semibold">{label}</div>
                  <div className="text-xs text-text-3">{hint}</div>
                </div>
                <Toggle checked={Boolean(s[key as "run_missed"])} label={label} onChange={(v) => setS({ ...s, [key]: v })} />
              </div>
            ))}
            <div className="flex items-center justify-between gap-4">
              <div>
                <div className="text-[13px] font-semibold">Notify me about new games</div>
                <div className="text-xs text-text-3">One Windows notification per batch of new games, never repeated for the same games.</div>
              </div>
              <Toggle checked={prefs.notifications} label="Notifications" onChange={(v) => void savePrefs({ ...prefs, notifications: v })} />
            </div>
          </div>
          {error ? <Callout tone="bad">{error}</Callout> : null}
          <div className="flex items-center justify-between border-t border-line pt-4">
            <span className="text-[13px] text-text-2">{s.mode === "manual" ? "No scheduled refresh." : `Next refresh: ${fmtDate(next)}`}</span>
            <button type="button" className="btn-primary" disabled={!dirty || Boolean(error)} onClick={() => void savePrefs({ ...prefs, schedule: s })}>
              Save schedule
            </button>
          </div>
        </div>
        <div className="space-y-4">
          <div className="card space-y-2 p-5 text-[13px]">
            <h2 className="h2">Status</h2>
            <p className="flex items-center gap-2">
              Windows task <span className="mono text-xs">BoilerSteamXL Library Refresh</span>
              <Chip tone={library?.schedule.installed ? "ok" : "muted"}>{library?.schedule.installed ? "registered" : "not registered"}</Chip>
            </p>
            <p>Next run: {fmtDate(library?.schedule.next_run)}</p>
            <p>Last scheduled refresh: {fmtDate(library?.schedule.last_scheduled)}</p>
            <p>Last successful refresh: {fmtDate(library?.last_scan?.at)}</p>
            <p className="text-xs text-text-3">The task runs BoilerSteamXL headless, so the window does not need to be open. Only one refresh runs at a time.</p>
          </div>
          <div className="card p-5">
            <h2 className="h2 flex items-center gap-2">
              <Icon name="terminal" /> Command line
            </h2>
            <ul className="mono mt-3 space-y-1.5 text-text-2">
              <li>BoilerSteamXL.exe --scan</li>
              <li>BoilerSteamXL.exe --sync</li>
              <li>BoilerSteamXL.exe --dry-run</li>
              <li>BoilerSteamXL.exe --status</li>
            </ul>
            <p className="mt-2 text-xs text-text-3">--sync only writes games you already confirmed that were waiting for Steam to close.</p>
          </div>
          <div className="card p-5">
            <h2 className="h2">Scheduled runs</h2>
            <ul className="mt-2 space-y-1.5 text-[13px]">
              {scheduled.slice(0, 8).map((r) => (
                <li key={r.id} className="flex justify-between gap-2">
                  <span>{fmtDate(r.at)}</span>
                  <span className="text-text-3">
                    {r.counts.new ?? 0} new · {r.counts.needs_review ?? 0} to review{r.error ? ` · ${r.error}` : ""}
                  </span>
                </li>
              ))}
              {scheduled.length === 0 ? <li className="text-text-3">None yet.</li> : null}
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
};
