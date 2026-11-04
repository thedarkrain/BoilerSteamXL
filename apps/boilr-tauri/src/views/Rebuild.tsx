import clsx from "clsx";
import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { useApp } from "../App";
import {
  api,
  displayTitle,
  type Decision,
  type Entry,
  type InspectedShortcut,
  type Inspection,
  type Plan,
  type RebuildDraft,
  type RebuildMode,
  type RunRecord,
  type SteamInfo,
} from "../api";
import { Callout, Chip, Cover, Icon, Modal, PageHeader, errorText, fileName, plural, useToast } from "../ui";
import { OpRow } from "./OpRow";

const STAGES = ["Choose Mode", "Scan & Compare", "Review & Approve", "Confirm", "Execute & Verify"];
const STAGE_KEY = "bsxl-rebuild-stage";

export const Rebuild = () => {
  const { reload, refresh, info, go } = useApp();
  const toast = useToast();
  const [stage, setStageRaw] = useState(() => Number(sessionStorage.getItem(STAGE_KEY) ?? 0));
  const [draft, setDraft] = useState<RebuildDraft | null>(null);
  const [inspection, setInspection] = useState<Inspection | null>(null);
  const [result, setResult] = useState<RunRecord | null>(null);
  const [loaded, setLoaded] = useState(false);

  const setStage = (s: number) => {
    sessionStorage.setItem(STAGE_KEY, String(s));
    setStageRaw(s);
  };
  const loadDraft = useCallback(async () => {
    const d = await api.rebuildDraft();
    setDraft(d);
    if (d) setInspection(await api.rebuildInspect());
    return d;
  }, []);

  useEffect(() => {
    loadDraft()
      .then((d) => {
        if (!d && stage !== 4) setStage(0);
        if (d && stage === 0) setStage(2); // a saved draft resumes at the review
      })
      .catch((e) => toast("bad", errorText(e)))
      .finally(() => setLoaded(true));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const cancel = async () => {
    await api.rebuildCancel();
    setDraft(null);
    setInspection(null);
    setStage(0);
    toast("ok", "Clean Rebuild cancelled. Nothing was changed.");
  };

  if (!loaded) return <p className="text-text-2">Loading…</p>;

  return (
    <div className="space-y-5">
      <PageHeader
        title="Clean Rebuild Steam Library"
        subtitle="Safely rebuild your non-Steam shortcuts using the games currently installed on your PC."
        actions={
          draft && stage < 4 ? (
            <button type="button" className="btn-ghost" onClick={() => void cancel()}>
              Cancel rebuild
            </button>
          ) : null
        }
      />
      <ol className="flex items-center gap-2">
        {STAGES.map((s, i) => (
          <li key={s} className="flex flex-1 items-center gap-2">
            <span className={clsx("grid h-7 w-7 flex-none place-items-center rounded-full text-xs font-bold", i < stage ? "bg-ok text-white" : i === stage ? "bg-accent text-white" : "bg-surface-3 text-text-3")}>
              {i < stage ? <Icon name="check" size={13} /> : i + 1}
            </span>
            <span className={clsx("text-[13px] font-semibold whitespace-nowrap", i === stage ? "text-text" : "text-text-3")}>{s}</span>
            {i < STAGES.length - 1 ? <span className={clsx("h-px flex-1", i < stage ? "bg-ok/50" : "bg-line")} /> : null}
          </li>
        ))}
      </ol>
      {info?.simulation ? (
        <Callout tone="warn" icon="shield">
          Simulation mode is on: the rebuild runs as a full dry run. Every step and the final report are real, but Steam is not modified.
        </Callout>
      ) : null}

      {stage === 0 ? (
        <ChooseMode
          onStart={async (mode, user) => {
            try {
              await api.rebuildStart(mode, user);
              toast("ok", "Scanning your game folders…");
              await refresh();
              await loadDraft();
              setStage(1);
            } catch (e) {
              toast("bad", errorText(e));
            }
          }}
        />
      ) : null}
      {stage === 1 && draft && inspection ? <Compare inspection={inspection} onBack={() => setStage(0)} onNext={() => setStage(2)} /> : null}
      {stage === 2 && draft && inspection ? (
        <ReviewDecisions draft={draft} inspection={inspection} onChanged={() => void loadDraft()} onBack={() => setStage(1)} onNext={() => setStage(3)} />
      ) : null}
      {stage === 3 && draft ? (
        <Confirm
          onBack={() => setStage(2)}
          onDone={async (r) => {
            setResult(r);
            await reload();
            await loadDraft();
            setStage(4);
          }}
        />
      ) : null}
      {stage === 4 ? (
        <Done
          result={result}
          onNew={() => {
            setResult(null);
            setStage(0);
          }}
          go={go}
        />
      ) : null}
      {stage > 0 && stage < 4 && !draft ? (
        <Callout>
          No rebuild in progress.{" "}
          <button type="button" className="link-btn" onClick={() => setStage(0)}>
            Start one
          </button>
        </Callout>
      ) : null}
    </div>
  );
};

const ChooseMode = ({ onStart }: { onStart: (mode: RebuildMode, user: string) => Promise<void> }) => {
  const [steam, setSteam] = useState<SteamInfo | null>(null);
  const [user, setUser] = useState("");
  const [mode, setMode] = useState<RebuildMode>("managed");
  const [understood, setUnderstood] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.steamInfo().then((s) => {
      setSteam(s);
      setUser(s.users.find((u) => !u.error)?.user_id ?? "");
    });
  }, []);
  const MODES: { key: RebuildMode; title: string; text: string; advanced?: boolean }[] = [
    { key: "managed", title: "Rebuild Managed Games", text: "Recreates only shortcuts BoilerSteamXL created. Everything else stays exactly as it is." },
    {
      key: "all_non_steam",
      title: "Rebuild All Non-Steam Games",
      text: "Lets you remove and replace every non-Steam shortcut of this account, including ones you made by hand or with BoilR and other tools.",
      advanced: true,
    },
  ];
  return (
    <div className="grid gap-5 xl:grid-cols-[1.4fr_1fr]">
      <div className="space-y-4">
        <div className="card p-5">
          <h2 className="h2">Steam account</h2>
          {steam && !steam.valid ? <Callout tone="bad">{steam.error}</Callout> : null}
          <div className="mt-3 grid gap-2 md:grid-cols-2">
            {steam?.users.map((u) => (
              <button
                key={u.user_id}
                type="button"
                disabled={Boolean(u.error)}
                onClick={() => setUser(u.user_id)}
                className={clsx("flex items-center gap-3 rounded-lg border p-3 text-left", user === u.user_id ? "border-accent bg-accent/8" : "border-line bg-surface-2 hover:border-line-strong", u.error && "opacity-50")}
              >
                <Icon name="user" className="text-text-3" />
                <span className="min-w-0 flex-1">
                  <span className="block font-semibold">{u.persona ?? `Account ${u.user_id}`}</span>
                  <span className="text-xs text-text-3">{u.error ? "shortcuts.vdf damaged: will not be modified" : `${plural(u.shortcut_count, "non-Steam shortcut")}`}</span>
                </span>
                {u.most_recent ? <Chip tone="accent">last used</Chip> : null}
              </button>
            ))}
          </div>
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          {MODES.map((m) => (
            <button
              key={m.key}
              type="button"
              onClick={() => setMode(m.key)}
              className={clsx("card relative p-5 text-left transition", mode === m.key ? (m.advanced ? "border-warn/70" : "border-accent/70") : "hover:border-line-strong")}
            >
              <div className="flex items-center gap-2">
                <span className={clsx("grid h-4 w-4 place-items-center rounded-full border-2", mode === m.key ? "border-accent bg-accent" : "border-line-strong")} />
                <span className="font-semibold">{m.title}</span>
                {m.advanced ? <Chip tone="warn">Advanced</Chip> : <Chip tone="ok">Recommended</Chip>}
              </div>
              <p className="mt-2 text-[13px] text-text-2">{m.text}</p>
            </button>
          ))}
        </div>
        {mode === "all_non_steam" ? (
          <Callout tone="warn">
            <b>Potentially destructive.</b> Shortcuts created outside BoilerSteamXL can be removed, and replaced shortcuts get new Steam IDs: collections, per-game
            settings and play history tied to the old IDs may not carry over. You decide every shortcut individually; undecided ones are left untouched.
            <label className="mt-2 flex items-center gap-2">
              <input type="checkbox" className="check" checked={understood} onChange={(e) => setUnderstood(e.target.checked)} /> I understand
            </label>
          </Callout>
        ) : null}
        <div className="flex justify-end">
          <button
            type="button"
            className="btn-primary"
            disabled={!user || busy || (mode === "all_non_steam" && !understood)}
            onClick={async () => {
              setBusy(true);
              await onStart(mode, user);
              setBusy(false);
            }}
          >
            <Icon name="search" /> {busy ? "Scanning…" : "Scan & Compare"}
          </button>
        </div>
      </div>
      <div className="card h-fit p-5">
        <h2 className="h2">Guarantees</h2>
        <ul className="mt-3 space-y-2.5 text-[13px]">
          {[
            "Game installation files are never touched.",
            "Save files are never touched.",
            "Official Steam games are never touched.",
            "Every replacement needs your approval in this rebuild.",
            "A verified backup is made before anything changes.",
            "Rollback is available from that backup.",
            "Steam is never force-closed.",
          ].map((g) => (
            <li key={g} className="flex gap-2">
              <Icon name="shield" className="mt-0.5 text-ok" /> {g}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
};

const flagChips = (s: InspectedShortcut) => (
  <>
    {s.broken ? <Chip tone="bad">Executable missing</Chip> : null}
    {s.duplicate_group !== null ? <Chip tone="warn">Duplicate</Chip> : null}
    {s.utility ? <Chip tone="warn" title={s.utility}>Utility</Chip> : null}
    {!s.in_scope ? <Chip>Not in this rebuild</Chip> : null}
  </>
);

const Compare = ({ inspection, onBack, onNext }: { inspection: Inspection; onBack: () => void; onNext: () => void }) => {
  const { library, art } = useApp();
  const [focus, setFocus] = useState<{ side: "left" | "right"; id: string } | null>(null);
  const proposed = useMemo(() => (library?.entries ?? []).filter((e) => !["ignored", "rejected"].includes(e.status) && e.exe), [library]);
  const c = inspection.counts;
  const linkedRight = focus?.side === "left" ? inspection.shortcuts.find((s) => s.key === focus.id)?.suggestion?.entry_id : undefined;
  const linkedLeft = focus?.side === "right" ? inspection.shortcuts.filter((s) => s.suggestion?.entry_id === focus.id).map((s) => s.key) : [];
  const unmatched = proposed.filter((e) => !inspection.shortcuts.some((s) => s.suggestion?.entry_id === e.id));
  return (
    <div className="space-y-4">
      <div className="card grid grid-cols-2 gap-3 p-4 text-center md:grid-cols-4 xl:grid-cols-8">
        {[
          ["Existing shortcuts", c.total],
          ["In this rebuild", c.in_scope],
          ["Valid installations", proposed.length],
          ["Duplicates", c.duplicates],
          ["Broken", c.broken],
          ["Excluded utilities", c.utilities],
          ["Unmatched existing", c.unmatched],
          ["Need review", library?.entries.filter((e) => e.status === "needs_review" || e.uncertain).length ?? 0],
        ].map(([label, value]) => (
          <div key={label as string}>
            <div className="font-display text-xl font-semibold">{(value as number) ?? 0}</div>
            <div className="text-[11px] text-text-3">{label as string}</div>
          </div>
        ))}
      </div>
      {inspection.error ? <Callout tone="bad">{inspection.error}</Callout> : null}
      <p className="text-xs text-text-3">Select an item to highlight its match on the other side. Matches are suggestions; a matching name alone never counts as the same installation.</p>
      <div className="grid gap-4 lg:grid-cols-2">
        <section className="panel min-h-0">
          <h3 className="border-b border-line px-4 py-3 font-semibold">Current Steam library</h3>
          <ul className="scroll-quiet max-h-[52vh] space-y-1 overflow-y-auto p-2">
            {inspection.shortcuts.map((s) => (
              <li key={s.key}>
                <button
                  type="button"
                  onClick={() => setFocus({ side: "left", id: s.key })}
                  className={clsx(
                    "w-full rounded-lg border px-3 py-2 text-left transition",
                    focus?.id === s.key || linkedLeft.includes(s.key) ? "border-accent bg-accent/8" : "border-transparent hover:bg-surface-2",
                    !s.in_scope && "opacity-55"
                  )}
                >
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span className="font-semibold">{s.shortcut.name}</span>
                    <Chip tone={s.origin === "BoilerSteamXL" ? "accent" : s.origin === "BoilR" ? "info" : "muted"}>{s.origin}</Chip>
                    {flagChips(s)}
                  </div>
                  <div className="mono mt-0.5 truncate text-text-3">{s.shortcut.exe}</div>
                  {s.suggestion ? (
                    <div className="mt-1 text-xs text-text-2">
                      → {s.suggestion.title} <span className="text-text-3">({s.suggestion.evidence})</span>
                    </div>
                  ) : (
                    <div className="mt-1 text-xs text-warn">No verified replacement</div>
                  )}
                </button>
              </li>
            ))}
            {inspection.shortcuts.length === 0 ? <li className="p-4 text-text-3">This account has no non-Steam shortcuts.</li> : null}
          </ul>
        </section>
        <section className="panel min-h-0">
          <h3 className="border-b border-line px-4 py-3 font-semibold">Proposed new library</h3>
          <ul className="scroll-quiet max-h-[52vh] space-y-1 overflow-y-auto p-2">
            {proposed.map((e) => (
              <li key={e.id}>
                <button
                  type="button"
                  onClick={() => setFocus({ side: "right", id: e.id })}
                  className={clsx("flex w-full items-center gap-3 rounded-lg border px-3 py-2 text-left transition", focus?.id === e.id || linkedRight === e.id ? "border-accent bg-accent/8" : "border-transparent hover:bg-surface-2")}
                >
                  <div className="h-10 w-7 flex-none overflow-hidden rounded">
                    <Cover title={displayTitle(e)} src={art[e.id]?.cover} icon={art[e.id]?.icon} className="!p-1 [&_span]:!text-[10px]" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="font-semibold">{displayTitle(e)}</span>
                      {e.uncertain ? <Chip tone="warn">Uncertain</Chip> : null}
                      {e.duplicate ? <Chip tone="warn">Duplicate evidence</Chip> : null}
                    </div>
                    <div className="truncate text-xs text-text-3">
                      {fileName(e.exe)} · name from {e.custom_title ? "your correction" : e.title_source.toLowerCase()}
                    </div>
                  </div>
                  {unmatched.includes(e) ? <Chip tone="ok">New</Chip> : null}
                </button>
              </li>
            ))}
          </ul>
        </section>
      </div>
      <div className="flex justify-between">
        <button type="button" className="btn-ghost" onClick={onBack}>
          Back
        </button>
        <button type="button" className="btn-primary" onClick={onNext}>
          Review & Approve <Icon name="arrow" />
        </button>
      </div>
    </div>
  );
};

const decisionOf = (draft: RebuildDraft, key: string): Decision => draft.decisions[key] ?? { action: "unresolved" };

const ReviewDecisions = ({
  draft,
  inspection,
  onChanged,
  onBack,
  onNext,
}: {
  draft: RebuildDraft;
  inspection: Inspection;
  onChanged: () => void;
  onBack: () => void;
  onNext: () => void;
}) => {
  const { library, openAddGame } = useApp();
  const toast = useToast();
  const [removeAsk, setRemoveAsk] = useState<InspectedShortcut[] | null>(null);
  const entries = useMemo(() => (library?.entries ?? []).filter((e) => !["ignored", "rejected"].includes(e.status) && e.exe), [library]);
  const byId = useMemo(() => new Map(entries.map((e) => [e.id, e])), [entries]);
  const scoped = inspection.shortcuts.filter((s) => s.in_scope);
  const mapped = new Set(Object.values(draft.decisions).flatMap((d) => (d.action === "replace" ? [d.entry_id] : [])));
  const newGames = entries.filter(
    (e) => !mapped.has(e.id) && (Boolean(draft.approved[e.id]) || (!e.import && e.status !== "imported" && e.status !== "already_in_steam"))
  );

  const call = async (fn: () => Promise<unknown>) => {
    try {
      const r = await fn();
      if (Array.isArray(r) && r.length) toast("bad", (r as [string, string][]).map((x) => x[1]).join(" "));
      onChanged();
    } catch (e) {
      toast("bad", errorText(e));
    }
  };
  const setDecision = (key: string, d: Decision) => call(() => api.rebuildSetDecision(key, d));
  const approve = (id: string, on: boolean) => call(() => api.rebuildApprove([id], on));
  const isApproved = (id: string) => Boolean(draft.approved[id]);

  const applyStrong = async () => {
    for (const s of scoped) {
      if (decisionOf(draft, s.key).action === "unresolved" && s.suggestion?.strong && !mapped.has(s.suggestion.entry_id)) {
        await api.rebuildSetDecision(s.key, { action: "replace", entry_id: s.suggestion.entry_id }).catch(() => undefined);
        mapped.add(s.suggestion.entry_id);
      }
    }
    onChanged();
    toast("ok", "Evidence-based matches applied. Name-only matches were left for you. Approve each replacement next.");
  };
  const approveEligibleReplacements = async () => {
    const ids = scoped
      .map((s) => decisionOf(draft, s.key))
      .flatMap((d) => (d.action === "replace" ? [d.entry_id] : []))
      .filter((id) => {
        const e = byId.get(id);
        return e && !e.uncertain && !isApproved(id);
      });
    await call(() => api.rebuildApprove(ids, true));
    toast("ok", `${plural(ids.length, "replacement")} approved. Uncertain ones still need you.`);
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className="btn-ghost" onClick={() => void applyStrong()}>
          <Icon name="sparkle" /> Apply evidence-based matches
        </button>
        <button type="button" className="btn-ghost" onClick={() => void approveEligibleReplacements()}>
          <Icon name="check" /> Approve eligible replacements
        </button>
        <button
          type="button"
          className="btn-ghost"
          onClick={() => setRemoveAsk(scoped.filter((s) => (s.broken || s.utility) && decisionOf(draft, s.key).action === "unresolved" && !s.suggestion))}
        >
          <Icon name="trash" /> Remove broken & utilities…
        </button>
        <button type="button" className="btn-ghost" onClick={openAddGame}>
          <Icon name="plus" /> Add a missing game
        </button>
        <span className="ml-auto text-xs text-text-3">Decisions are saved as you go.</span>
      </div>

      {inspection.groups.filter((g) => g.keys.some((k) => scoped.some((s) => s.key === k))).map((g, gi) => {
        const members = g.keys.map((k) => inspection.shortcuts.find((s) => s.key === k)).filter((s): s is InspectedShortcut => Boolean(s));
        return (
          <section key={gi} className="panel border-warn/30 p-4">
            <div className="mb-3 flex items-center gap-2">
              <Icon name="alert" className="text-warn" />
              <h3 className="font-semibold">Duplicate group: {plural(members.length, "shortcut")} start the same executable</h3>
              <span className="text-xs text-text-3">{g.reason}</span>
            </div>
            <div className="grid gap-2" style={{ gridTemplateColumns: `repeat(${members.length}, minmax(0, 1fr))` }}>
              {members.map((m) => (
                <div key={m.key} className={clsx("rounded-lg border p-3", m.key === g.recommended_keep ? "border-ok/50 bg-ok/5" : "border-line bg-surface-2")}>
                  <div className="flex items-center gap-1.5">
                    <span className="font-semibold">{m.shortcut.name}</span>
                    {m.key === g.recommended_keep ? <Chip tone="ok">Recommended to keep</Chip> : null}
                  </div>
                  <div className="text-xs text-text-3">
                    {m.origin} · {m.broken ? "executable missing" : "executable exists"} · {m.artwork.length ? `${m.artwork.length} artwork files` : "no artwork"}
                  </div>
                  <dl className="mono mt-2 grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5 text-[11px]">
                    {([
                      ["Target", m.shortcut.exe],
                      ["Start in", m.shortcut.start_dir],
                      ["Options", m.shortcut.launch_options],
                      ["App ID", String(m.shortcut.app_id)],
                    ] as const).map(([label, value], i) => {
                      // Values that differ between the duplicates are what tells them apart.
                      const differs = members.some((x) => [x.shortcut.exe, x.shortcut.start_dir, x.shortcut.launch_options, String(x.shortcut.app_id)][i] !== value);
                      return (
                        <Fragment key={label}>
                          <dt className="text-text-3">{label}</dt>
                          <dd className={clsx("break-all", differs ? "text-warn" : "text-text-2")}>{value || "—"}</dd>
                        </Fragment>
                      );
                    })}
                  </dl>
                  {members.some((x) => x.key !== m.key && !x.in_scope) ? (
                    <p className="mt-1 text-[11px] text-text-3">Shortcuts outside this rebuild stay as they are.</p>
                  ) : null}
                  <button
                    type="button"
                    className="btn-ghost btn-sm mt-2"
                    disabled={!members.some((x) => x.key !== m.key && x.in_scope)}
                    onClick={async () => {
                      for (const other of members.filter((x) => x.key !== m.key && x.in_scope)) await api.rebuildSetDecision(other.key, { action: "remove", confirmed: true }).catch(() => undefined);
                      if (m.in_scope && decisionOf(draft, m.key).action !== "replace") await api.rebuildSetDecision(m.key, { action: "keep" }).catch(() => undefined);
                      onChanged();
                    }}
                  >
                    Keep this one, remove the other{members.length > 2 ? "s" : ""}
                  </button>
                </div>
              ))}
            </div>
          </section>
        );
      })}

      <div className="panel overflow-hidden">
        <div className="grid grid-cols-[1fr_1fr_300px] gap-4 border-b border-line bg-surface-2 px-4 py-2.5 text-xs font-semibold text-text-3">
          <span>A · Existing shortcut</span>
          <span>B · Proposed replacement</span>
          <span>C · Decision</span>
        </div>
        {scoped.length === 0 ? <p className="p-6 text-text-3">No shortcuts are part of this rebuild. New games below can still be added.</p> : null}
        {scoped.map((s) => {
          const d = decisionOf(draft, s.key);
          const suggested = s.suggestion && !mapped.has(s.suggestion.entry_id) ? s.suggestion.entry_id : undefined;
          const replacementId = d.action === "replace" ? d.entry_id : suggested;
          const replacement = replacementId ? byId.get(replacementId) : undefined;
          return (
            <div key={s.key} className={clsx("grid grid-cols-[1fr_1fr_300px] items-start gap-4 border-b border-line px-4 py-3", d.action === "unresolved" && "bg-warn/[0.03]")}>
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="font-semibold">{s.shortcut.name}</span>
                  {flagChips(s)}
                </div>
                <div className="mono truncate text-text-3" title={s.shortcut.exe}>
                  {s.shortcut.exe}
                </div>
                {s.shortcut.launch_options ? <div className="mono text-xs text-warn">args: {s.shortcut.launch_options}</div> : null}
                <div className="text-[11px] text-text-3">
                  ID {s.shortcut.app_id} · {s.artwork.length ? `${s.artwork.length} artwork files` : "no artwork"}
                </div>
              </div>
              <div className="min-w-0">
                <select
                  className="field py-1.5"
                  value={replacementId ?? ""}
                  onChange={(e) => void setDecision(s.key, e.target.value ? { action: "replace", entry_id: e.target.value } : { action: "unresolved" })}
                  aria-label={`Replacement for ${s.shortcut.name}`}
                >
                  <option value="">No replacement</option>
                  {entries.map((e) => (
                    <option key={e.id} value={e.id} disabled={mapped.has(e.id) && e.id !== replacementId}>
                      {displayTitle(e)} — {fileName(e.exe)}
                    </option>
                  ))}
                </select>
                {replacement ? (
                  <div className="mt-1.5 text-xs">
                    <div className="mono truncate text-text-3">{replacement.exe}</div>
                    <div className="text-text-3">
                      Name from {replacement.custom_title ? "your correction" : replacement.title_source.toLowerCase()}
                      {s.suggestion?.entry_id === replacement.id ? ` · suggested (${s.suggestion.evidence})` : ""}
                      {replacement.uncertain ? " · executable uncertain" : ""}
                    </div>
                  </div>
                ) : (
                  <p className="mt-1.5 text-xs text-warn">No verified replacement. Keep it, remove it, or pick a game.</p>
                )}
              </div>
              <div className="space-y-2">
                <div className="flex flex-wrap gap-1">
                  {(["keep", "replace", "remove", "unresolved"] as const).map((a) => (
                    <button
                      key={a}
                      type="button"
                      disabled={a === "replace" && !replacementId}
                      onClick={() =>
                        void setDecision(
                          s.key,
                          a === "replace" ? { action: "replace", entry_id: replacementId! } : a === "remove" ? { action: "remove", confirmed: false } : { action: a }
                        )
                      }
                      className={clsx(
                        "rounded-md border px-2 py-1 text-xs font-semibold disabled:opacity-40",
                        d.action === a ? (a === "remove" ? "border-bad bg-bad/15 text-bad" : "border-accent bg-accent/15 text-text") : "border-line text-text-2 hover:border-line-strong"
                      )}
                    >
                      {a === "unresolved" ? "Skip" : a[0].toUpperCase() + a.slice(1)}
                    </button>
                  ))}
                </div>
                {d.action === "replace" ? (
                  <label className="flex items-center gap-2 text-xs">
                    <input type="checkbox" className="check" checked={isApproved(d.entry_id)} onChange={(e) => void approve(d.entry_id, e.target.checked)} />
                    {isApproved(d.entry_id) ? <span className="text-ok">Replacement approved</span> : <span className="text-warn">Approve this replacement</span>}
                  </label>
                ) : null}
                {d.action === "remove" ? (
                  d.confirmed ? (
                    <p className="text-xs text-bad">Will be removed without replacement (kept in the backup).</p>
                  ) : (
                    <button type="button" className="btn-danger btn-sm" onClick={() => void setDecision(s.key, { action: "remove", confirmed: true })}>
                      Confirm removal without replacement
                    </button>
                  )
                ) : null}
                {d.action === "unresolved" ? <p className="text-xs text-text-3">Undecided: left exactly as it is.</p> : null}
              </div>
            </div>
          );
        })}
      </div>

      <section className="panel p-4">
        <h3 className="font-semibold">New games to add ({newGames.length})</h3>
        <p className="mb-3 text-xs text-text-3">Approved games that replace no shortcut are created as new shortcuts. Each needs its own approval.</p>
        <ul className="grid gap-1.5 md:grid-cols-2">
          {newGames.map((e: Entry) => (
            <li key={e.id} className="flex items-center gap-3 rounded-lg border border-line bg-surface-2 px-3 py-2">
              <input type="checkbox" className="check" checked={isApproved(e.id)} onChange={(ev) => void approve(e.id, ev.target.checked)} aria-label={`Approve ${displayTitle(e)}`} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px] font-semibold">{displayTitle(e)}</span>
                <span className="mono block truncate text-text-3">{e.exe}</span>
              </span>
              {e.uncertain ? <Chip tone="warn">Uncertain</Chip> : null}
              {e.duplicate ? <Chip tone="warn">Duplicate?</Chip> : null}
            </li>
          ))}
        </ul>
      </section>

      <div className="flex justify-between">
        <button type="button" className="btn-ghost" onClick={onBack}>
          Back
        </button>
        <button type="button" className="btn-primary" onClick={onNext}>
          Preview final plan <Icon name="arrow" />
        </button>
      </div>

      {removeAsk ? (
        <Modal
          title={`Remove ${plural(removeAsk.length, "shortcut")} without replacement?`}
          onClose={() => setRemoveAsk(null)}
          footer={
            <>
              <button type="button" className="btn-ghost" onClick={() => setRemoveAsk(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn-danger"
                disabled={!removeAsk.length}
                onClick={async () => {
                  for (const s of removeAsk) await api.rebuildSetDecision(s.key, { action: "remove", confirmed: true }).catch(() => undefined);
                  setRemoveAsk(null);
                  onChanged();
                }}
              >
                Remove these {removeAsk.length}
              </button>
            </>
          }
        >
          {removeAsk.length === 0 ? (
            <p className="text-text-2">No undecided broken or utility shortcuts without a suggested replacement.</p>
          ) : (
            <ul className="space-y-1 text-[13px]">
              {removeAsk.map((s) => (
                <li key={s.key}>
                  <b>{s.shortcut.name}</b> <span className="text-text-3">— {s.broken ? "executable missing" : s.utility}</span>
                </li>
              ))}
            </ul>
          )}
        </Modal>
      ) : null}
    </div>
  );
};

const Confirm = ({ onBack, onDone }: { onBack: () => void; onDone: (r: RunRecord) => Promise<void> }) => {
  const { info } = useApp();
  const toast = useToast();
  const [plan, setPlan] = useState<Plan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [phrase, setPhrase] = useState("");
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const load = () => api.rebuildPlan().then(setPlan).catch((e) => setError(errorText(e)));
  useEffect(() => {
    void load();
  }, []);
  const expected = info?.rebuild_phrase ?? "REBUILD MY NON-STEAM LIBRARY";
  const s = plan?.summary ?? {};
  const changes = plan?.ops.filter((o) => o.kind !== "keep" && o.kind !== "unresolved") ?? [];
  const ready = plan && plan.blocked.length === 0 && changes.length > 0 && ack && phrase === expected && !busy;

  const execute = async () => {
    if (!plan) return;
    setBusy(true);
    try {
      const r = await api.rebuildExecute(plan.token, phrase, ack);
      await onDone(r);
    } catch (e) {
      toast("bad", errorText(e));
      await load();
    } finally {
      setBusy(false);
    }
  };

  if (error) return <Callout tone="bad">{error}</Callout>;
  if (!plan) return <p className="text-text-2">Building the plan…</p>;
  return (
    <div className="grid gap-5 xl:grid-cols-[1fr_380px]">
      <div className="space-y-4">
        <div className="card grid grid-cols-2 items-center gap-6 p-5 md:grid-cols-[auto_auto_1fr]">
          <div>
            <div className="label">Before</div>
            <div className="font-display text-3xl font-semibold">{s.before ?? 0}</div>
            <div className="text-xs text-text-3">non-Steam shortcuts</div>
          </div>
          <div>
            <div className="label">After</div>
            <div className="gradient-text font-display text-3xl font-semibold">{s.after ?? 0}</div>
            <div className="text-xs text-text-3">non-Steam shortcuts</div>
          </div>
          <ul className="col-span-2 grid grid-cols-2 gap-x-6 gap-y-1 text-[13px] md:col-span-1">
            <li><b>{s.keep ?? 0}</b> retained unchanged</li>
            <li><b>{s.replace ?? 0}</b> replaced</li>
            <li><b>{s.create ?? 0}</b> new shortcuts created</li>
            <li><b>{s.remove ?? 0}</b> removed without replacement</li>
            <li><b>{s.unresolved ?? 0}</b> undecided, left unmodified</li>
            <li><b>{s.still_review ?? 0}</b> games still awaiting review</li>
            <li><b>{s.duplicates_created ?? 0}</b> duplicates created</li>
            <li className={plan.blocked.length ? "text-bad" : ""}><b>{plan.blocked.length}</b> unresolved conflicts</li>
          </ul>
        </div>
        {plan.blocked.length ? (
          <Callout tone="bad">
            Resolve these before the rebuild can run:
            <ul className="mt-1 list-disc pl-5">
              {plan.blocked.map((b) => (
                <li key={b.id}>
                  <b>{b.title}</b>: {b.reason}
                </li>
              ))}
            </ul>
          </Callout>
        ) : null}
        {plan.warnings.map((w) => (
          <Callout key={w} tone="warn">
            {w}
          </Callout>
        ))}
        <div>
          <h3 className="label mb-2">Every planned operation ({plan.ops.length})</h3>
          <ul className="space-y-1.5">
            {[...changes, ...plan.ops.filter((o) => o.kind === "keep" || o.kind === "unresolved")].map((op, i) => (
              <OpRow key={i} op={op} />
            ))}
          </ul>
        </div>
      </div>
      <div className="card h-fit space-y-4 p-5">
        <h2 className="h2">Confirm the rebuild</h2>
        <ul className="space-y-1.5 text-[13px] text-text-2">
          <li className="flex gap-2"><Icon name="shield" className="mt-0.5 text-ok" /> A timestamped backup of shortcuts.vdf, artwork and collections is created and verified first. If it fails, nothing happens.</li>
          <li className="flex gap-2"><Icon name="alert" className="mt-0.5 text-warn" /> Replaced shortcuts can get new Steam IDs. Artwork is copied to the new ID; collections, per-game Steam settings and play history tied to old IDs may not carry over.</li>
          <li className="flex gap-2"><Icon name="steam" className="mt-0.5 text-text-3" /> Steam must be closed. It is never force-closed.</li>
        </ul>
        <label className="flex items-start gap-2 text-[13px]">
          <input type="checkbox" className="check mt-0.5" checked={ack} onChange={(e) => setAck(e.target.checked)} />I understand the risks and want to apply exactly this plan.
        </label>
        <label className="block">
          <span className="text-xs text-text-2">
            Type <b className="mono text-text">{expected}</b>
          </span>
          <input className="field mono mt-1" value={phrase} onChange={(e) => setPhrase(e.target.value)} spellCheck={false} />
        </label>
        <div className="flex gap-2">
          <button type="button" className="btn-ghost" onClick={onBack}>
            Back
          </button>
          <button type="button" className="btn-danger flex-1" disabled={!ready} onClick={() => void execute()}>
            {busy ? "Working…" : plan.simulation ? "Run Approved Rebuild (simulation)" : "Execute Approved Rebuild"}
          </button>
        </div>
      </div>
    </div>
  );
};

const Done = ({ result, onNew, go }: { result: RunRecord | null; onNew: () => void; go: (v: "steam" | "backups") => void }) => {
  if (!result) {
    return (
      <Callout>
        The last rebuild finished. See Backups & History for its report.{" "}
        <button type="button" className="link-btn" onClick={onNew}>
          Start a new rebuild
        </button>
      </Callout>
    );
  }
  const c = result.counts;
  return (
    <div className="card space-y-5 p-6">
      <div className="flex items-center gap-3">
        <span className="grid h-12 w-12 place-items-center rounded-full bg-ok/15 text-ok">
          <Icon name="check" size={24} />
        </span>
        <div>
          <h2 className="h2">{result.simulated ? "Simulated rebuild complete. Steam was not modified." : "Rebuild complete and verified"}</h2>
          <p className="text-[13px] text-text-2">{result.simulated ? "Turn off Simulation mode in Settings to apply it for real." : "Restart Steam to see your clean library."}</p>
        </div>
      </div>
      <ul className="grid grid-cols-2 gap-3 md:grid-cols-5">
        {[
          ["Created", c.create],
          ["Replaced", c.replace],
          ["Retained", c.keep],
          ["Removed", c.remove],
          ["Left undecided", c.unresolved],
        ].map(([l, v]) => (
          <li key={l as string} className="rounded-lg border border-line bg-surface-2 p-3">
            <div className="font-display text-2xl font-semibold">{(v as number) ?? 0}</div>
            <div className="text-xs text-text-3">{l as string}</div>
          </li>
        ))}
      </ul>
      {result.backup ? (
        <p className="text-[13px]">
          Backup: <span className="mono">{result.backup}</span> · restore available
        </p>
      ) : null}
      <details className="text-[13px]">
        <summary className="cursor-pointer text-text-2">Operation log ({result.details.length})</summary>
        <ul className="mono mt-2 space-y-0.5 text-text-3">
          {result.details.map((d, i) => (
            <li key={i}>{d}</li>
          ))}
        </ul>
      </details>
      <div className="flex flex-wrap gap-2">
        <button type="button" className="btn-primary" onClick={() => go("steam")}>
          View New Library
        </button>
        <button type="button" className="btn-ghost" onClick={() => go("backups")}>
          View Operation Log
        </button>
        {result.backup ? (
          <button type="button" className="btn-ghost" onClick={() => go("backups")}>
            Restore Previous Library
          </button>
        ) : null}
        <button type="button" className="btn-ghost ml-auto" onClick={onNew}>
          New rebuild
        </button>
      </div>
    </div>
  );
};
