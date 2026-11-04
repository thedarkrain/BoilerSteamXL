import clsx from "clsx";
import { useEffect, useMemo, useState } from "react";
import { useApp } from "../App";
import { api, displayTitle, type Entry, type Status } from "../api";
import { Chip, Cover, Empty, Icon, PageHeader, StatusChip, errorText, fileName, plural, useToast } from "../ui";
import { ArtworkPicker, CandidatePicker, DecisionPanel, EntryForm } from "./EntryEditor";

type Tab = "pending" | "duplicates" | "approved" | "missing" | "decided";
const TABS: { key: Tab; label: string; match: (s: Status) => boolean }[] = [
  { key: "pending", label: "Needs review", match: (s) => s === "needs_review" || s === "possible_duplicate" || s === "update_available" || s === "import_failed" },
  { key: "duplicates", label: "Duplicates", match: (s) => s === "possible_duplicate" || s === "already_in_steam" },
  { key: "approved", label: "Approved", match: (s) => s === "approved" || s === "ready_to_import" },
  { key: "missing", label: "Missing", match: (s) => s === "missing" },
  { key: "decided", label: "Rejected / ignored", match: (s) => s === "rejected" || s === "ignored" },
];

const isEligible = (e: Entry) => e.status === "needs_review" && !e.uncertain && !e.duplicate && !!e.exe;

export const Review = () => {
  const { library, reload, openImport, art } = useApp();
  const toast = useToast();
  const [tab, setTab] = useState<Tab>("pending");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<"confidence" | "title" | "status">("confidence");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());

  const list = useMemo(() => {
    const t = TABS.find((x) => x.key === tab) ?? TABS[0];
    const q = query.trim().toLowerCase();
    const items = (library?.entries ?? []).filter((e) => t.match(e.status) && (!q || `${displayTitle(e)} ${e.exe}`.toLowerCase().includes(q)));
    return items.sort((a, b) =>
      sort === "title" ? displayTitle(a).localeCompare(displayTitle(b)) : sort === "status" ? a.status.localeCompare(b.status) : Number(a.uncertain) - Number(b.uncertain) || b.confidence - a.confidence
    );
  }, [library, tab, query, sort]);

  useEffect(() => {
    if (!list.some((e) => e.id === selectedId)) setSelectedId(list[0]?.id ?? null);
  }, [list, selectedId]);
  useEffect(() => setChecked(new Set()), [tab, query]);

  const selected = library?.entries.find((e) => e.id === selectedId) ?? null;
  const counts = (key: Tab) => (library?.entries ?? []).filter((e) => (TABS.find((t) => t.key === key) ?? TABS[0]).match(e.status)).length;
  const approvedCount = (library?.counts.approved ?? 0) + (library?.counts.ready_to_import ?? 0);
  const eligibleCount = (library?.entries ?? []).filter(isEligible).length;

  const bulk = async (fn: () => Promise<unknown>, msg: string) => {
    try {
      const r = (await fn()) as { errors?: [string, string][] } | undefined;
      await reload();
      setChecked(new Set());
      if (r?.errors?.length) toast("warn", `${msg} ${plural(r.errors.length, "game")} skipped: ${r.errors[0][1]}`);
      else toast("ok", msg);
    } catch (e) {
      toast("bad", errorText(e));
    }
  };
  const checkedEntries = list.filter((e) => checked.has(e.id));
  const approveSelected = () => {
    const ok = checkedEntries.filter(isEligible).map((e) => e.id);
    const skipped = checkedEntries.length - ok.length;
    void bulk(() => api.approve(ok), `${plural(ok.length, "game")} approved.${skipped ? ` ${skipped} uncertain or possible duplicates need individual review.` : ""}`);
  };

  return (
    <div className="flex h-[calc(100vh-7.5rem)] flex-col gap-4">
      <PageHeader
        title="New Games"
        subtitle="Every game must be approved before it can be imported. Bulk approval only includes clear-cut detections."
        actions={
          <>
            <button type="button" className="btn-ghost" disabled={!eligibleCount} onClick={() => void bulk(() => api.approveEligible(), "Clear-cut games approved.")}>
              <Icon name="check" /> Approve all eligible ({eligibleCount})
            </button>
            <button type="button" className="btn-primary" disabled={!approvedCount} onClick={openImport}>
              <Icon name="download" /> Import {approvedCount} approved…
            </button>
          </>
        }
      />
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(300px,360px)_minmax(0,1fr)_minmax(280px,340px)] gap-4">
        {/* Left: queue */}
        <section className="panel flex min-h-0 flex-col">
          <div className="space-y-2 border-b border-line p-3">
            <div className="flex flex-wrap gap-1">
              {TABS.map((t) => (
                <button
                  key={t.key}
                  type="button"
                  onClick={() => setTab(t.key)}
                  className={clsx("rounded-md px-2 py-1 text-xs font-semibold", tab === t.key ? "bg-surface-3 text-text" : "text-text-3 hover:text-text")}
                >
                  {t.label} <span className="text-text-3">{counts(t.key)}</span>
                </button>
              ))}
            </div>
            <div className="flex gap-2">
              <input className="field py-1.5" placeholder="Filter" value={query} onChange={(e) => setQuery(e.target.value)} />
              <select className="field w-auto py-1.5" value={sort} onChange={(e) => setSort(e.target.value as typeof sort)} aria-label="Sort">
                <option value="confidence">Confidence</option>
                <option value="title">Title</option>
                <option value="status">Status</option>
              </select>
            </div>
            {checked.size ? (
              <div className="flex flex-wrap items-center gap-1.5 rounded-lg bg-surface-2 p-2 text-xs">
                <b>{checkedEntries.length} selected</b>
                <button type="button" className="btn-ghost btn-sm" onClick={approveSelected}>
                  Approve
                </button>
                <button type="button" className="btn-ghost btn-sm" onClick={() => void bulk(() => api.reject(checkedEntries.map((e) => e.id)), "Rejected.")}>
                  Reject
                </button>
                <button type="button" className="btn-ghost btn-sm" onClick={() => void bulk(() => api.ignore(checkedEntries.map((e) => e.id)), "Ignored permanently.")}>
                  Ignore
                </button>
                <button type="button" className="link-btn ml-auto !text-xs" onClick={() => setChecked(new Set())}>
                  Clear
                </button>
              </div>
            ) : list.length ? (
              <button type="button" className="link-btn !text-xs" onClick={() => setChecked(new Set(list.map((e) => e.id)))}>
                Select all {list.length}
              </button>
            ) : null}
          </div>
          <ul className="scroll-quiet min-h-0 flex-1 overflow-y-auto p-2">
            {list.length === 0 ? (
              <Empty icon="check" title="Nothing here">
                {tab === "pending" ? "All caught up. Refresh the library to look for new games." : null}
              </Empty>
            ) : null}
            {list.map((e) => (
              <li key={e.id}>
                <div className={clsx("flex items-center gap-2.5 rounded-lg px-2 py-2 transition", e.id === selectedId ? "bg-surface-3" : "hover:bg-surface-2")}>
                  <input
                    type="checkbox"
                    className="check"
                    checked={checked.has(e.id)}
                    onChange={(ev) => {
                      const next = new Set(checked);
                      if (ev.target.checked) next.add(e.id);
                      else next.delete(e.id);
                      setChecked(next);
                    }}
                    aria-label={`Select ${displayTitle(e)}`}
                  />
                  <button type="button" className="flex min-w-0 flex-1 items-center gap-2.5 text-left" onClick={() => setSelectedId(e.id)} aria-current={e.id === selectedId || undefined}>
                  <div className="h-11 w-8 flex-none overflow-hidden rounded-md">
                    <Cover title={displayTitle(e)} src={art[e.id]?.cover} icon={art[e.id]?.icon} className="!p-1 [&_span]:!text-[11px]" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] font-semibold">{displayTitle(e)}</div>
                    <div className="truncate text-xs text-text-3">{e.exe ? fileName(e.exe) : "No executable"}</div>
                  </div>
                  <div className="flex flex-col items-end gap-1">
                    {e.source === "folder" ? (
                      e.uncertain ? <Chip tone="warn">Uncertain</Chip> : <span className="text-xs font-bold text-ok tabular-nums">{e.confidence}%</span>
                    ) : (
                      <Chip>{e.source === "manual" ? "Manual" : e.launcher}</Chip>
                    )}
                    {e.status !== "needs_review" ? <StatusChip status={e.status} /> : null}
                  </div>
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </section>

        {/* Center: details */}
        <section className="panel scroll-quiet min-h-0 overflow-y-auto p-5">
          {selected ? (
            <div className="space-y-6">
              <div>
                <h2 className="font-display text-2xl font-semibold">{displayTitle(selected)}</h2>
                <p className="mt-1 text-xs text-text-3">
                  Name from: <b className="text-text-2">{selected.custom_title ? "your correction" : selected.title_source}</b>
                  {selected.game_dir ? <> · folder </> : null}
                  <button type="button" className="mono text-accent-2 hover:underline" onClick={() => void api.reveal(selected.game_dir)}>
                    {selected.game_dir}
                  </button>
                </p>
              </div>
              <CandidatePicker key={`c-${selected.id}`} entry={selected} onChanged={() => void reload()} />
              <EntryForm key={`f-${selected.id}`} entry={selected} onChanged={() => void reload()} />
            </div>
          ) : (
            <Empty icon="review" title="Select a game" />
          )}
        </section>

        {/* Right: validation and decision */}
        <section className="panel scroll-quiet min-h-0 overflow-y-auto p-4">
          {selected ? (
            <div className="space-y-4">
              <div className="mx-auto aspect-[2/3] w-40 overflow-hidden rounded-xl border border-line shadow-lg">
                <Cover title={displayTitle(selected)} src={art[selected.id]?.cover} icon={art[selected.id]?.icon} />
              </div>
              <ArtworkPicker entry={selected} />
              <DecisionPanel key={`d-${selected.id}`} entry={selected} onChanged={() => void reload()} />
            </div>
          ) : null}
        </section>
      </div>
    </div>
  );
};
