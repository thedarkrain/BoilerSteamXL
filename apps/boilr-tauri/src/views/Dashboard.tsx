import { useApp } from "../App";
import type { RunRecord, ScanSummary } from "../api";
import { Callout, Chip, Icon, Stat, fmtAgo, fmtDate, plural } from "../ui";

const KIND_LABEL: Record<RunRecord["kind"], string> = { scan: "Refresh", import: "Import", rebuild: "Clean Rebuild", restore: "Restore" };

export const Dashboard = ({ lastSummary }: { lastSummary: ScanSummary | null }) => {
  const { library, prefs, info, go, refresh, scanning, openAddGame, openImport } = useApp();
  const c = library?.counts ?? {};
  const pending = (c.needs_review ?? 0) + (c.possible_duplicate ?? 0) + (c.update_available ?? 0);
  const inSteam = (c.imported ?? 0) + (c.already_in_steam ?? 0);
  const approved = (c.approved ?? 0) + (c.ready_to_import ?? 0);
  const s = lastSummary ?? library?.last_scan ?? null;
  const total = library?.entries.filter((e) => e.status !== "ignored" && e.status !== "rejected").length ?? 0;

  return (
    <div className="space-y-6">
      <section className="hero-glow card relative overflow-hidden p-7">
        <div className="flex flex-wrap items-center justify-between gap-6">
          <div>
            <p className="label">Your library</p>
            <h1 className="mt-1 font-display text-3xl font-semibold tracking-tight">
              {total > 0 ? (
                <>
                  {plural(total, "game")} found, <span className="gradient-text">{plural(pending, "waiting", "waiting")} for you</span>
                </>
              ) : (
                <>Let's find your games</>
              )}
            </h1>
            <p className="mt-2 max-w-xl text-text-2">
              Discover automatically. Add manually. Validate everything. Import safely. Nothing reaches Steam until you approve it and confirm the final plan.
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <button type="button" className="btn-primary" onClick={() => void refresh()} disabled={scanning}>
              <Icon name="refresh" className={scanning ? "animate-spin" : ""} /> {scanning ? "Refreshing…" : "Refresh Library"}
            </button>
            <button type="button" className="btn-ghost" onClick={openAddGame}>
              <Icon name="plus" /> Add Game
            </button>
            <button type="button" className="btn-ghost" onClick={() => go("review")}>
              <Icon name="review" /> Review Pending ({pending})
            </button>
            <button type="button" className="btn-ghost" onClick={() => go("rebuild")}>
              <Icon name="rebuild" /> Clean Rebuild
            </button>
          </div>
        </div>
      </section>

      {info?.simulation ? (
        <Callout tone="warn" icon="shield">
          <b>Simulation mode is on.</b> Imports, rebuilds and restores are fully planned and reported, but nothing is written to Steam. Turn it off in{" "}
          <button type="button" className="link-btn" onClick={() => go("settings")}>
            Settings → Safety
          </button>{" "}
          when you're ready.
        </Callout>
      ) : null}

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Stat icon="review" label="Waiting for review" value={pending} tone="accent" onClick={() => go("review")} />
        <Stat icon="check" label="Approved, ready to import" value={approved} tone="ok" onClick={approved ? openImport : undefined} />
        <Stat icon="steam" label="Already in Steam" value={inSteam} tone="info" onClick={() => go("games")} />
        <Stat icon="alert" label="Possible duplicates" value={c.possible_duplicate ?? 0} tone="warn" onClick={() => go("review")} />
      </div>

      <div className="grid gap-6 xl:grid-cols-[1.4fr_1fr]">
        <section className="card p-5">
          <div className="flex items-center justify-between">
            <h2 className="h2">Last refresh</h2>
            {s ? <span className="text-xs text-text-3">{fmtDate(s.at)}</span> : null}
          </div>
          {s ? (
            <>
              <p className="mt-1 flex items-center gap-2 text-sm">
                {s.cancelled ? <Chip tone="warn">Cancelled</Chip> : <Chip tone="ok">Library refreshed successfully</Chip>}
                <span className="text-text-3">in {(s.duration_ms / 1000).toFixed(1)} s</span>
              </p>
              <ul className="mt-4 grid grid-cols-2 gap-x-6 gap-y-2.5 text-[13px] md:grid-cols-3">
                {[
                  ["folders", "Folders scanned", s.folders_scanned],
                  ["games", "Games found", s.games_found],
                  ["plus", "New games", s.new_games],
                  ["steam", "Already in Steam", s.already_in_steam],
                  ["review", "Require review", s.needs_review],
                  ["ban", "Unwanted executables ignored", s.exes_excluded],
                  ["alert", "Missing", s.missing],
                ].map(([icon, label, value]) => (
                  <li key={label as string} className="flex items-center gap-2">
                    <Icon name={icon as string} className="text-text-3" />
                    <b className="tabular-nums">{value as number}</b>
                    <span className="text-text-2">{label as string}</span>
                  </li>
                ))}
              </ul>
              {s.root_errors.length ? (
                <div className="mt-4 space-y-1">
                  {s.root_errors.map(([root, e]) => (
                    <Callout key={root} tone="warn">
                      <b>{root}</b>: {e}
                    </Callout>
                  ))}
                </div>
              ) : null}
            </>
          ) : (
            <p className="mt-3 text-text-2">Press Refresh Library to scan {prefs?.scan_roots.map((r) => r.path).join(", ") || "your game folders"}.</p>
          )}
        </section>

        <section className="card p-5">
          <div className="flex items-center justify-between">
            <h2 className="h2">Automatic refresh</h2>
            <button type="button" className="link-btn" onClick={() => go("automation")}>
              Change
            </button>
          </div>
          <p className="mt-2 text-[13px] text-text-2">
            {prefs?.schedule.mode === "manual"
              ? "Manual only. Refresh whenever you like."
              : `Next: ${fmtDate(library?.schedule.next_run)} (${prefs?.schedule.mode})`}
          </p>
          <p className="mt-1 text-xs text-text-3">Scheduled refreshes only discover games. They never import anything without your approval.</p>
          <h3 className="label mt-5 mb-2">Recent activity</h3>
          <ul className="space-y-2">
            {(library?.history ?? []).slice(0, 6).map((r) => (
              <li key={r.id} className="flex items-center gap-2 text-[13px]">
                <Icon name={r.kind === "scan" ? "refresh" : r.kind === "rebuild" ? "rebuild" : r.kind === "restore" ? "backups" : "download"} className="text-text-3" />
                <span className="font-medium">{KIND_LABEL[r.kind]}</span>
                {r.simulated ? <Chip tone="warn">simulated</Chip> : null}
                {r.scheduled ? <Chip>scheduled</Chip> : null}
                {r.error ? <Chip tone="bad">failed</Chip> : null}
                <span className="ml-auto text-xs text-text-3">{fmtAgo(r.at)}</span>
              </li>
            ))}
            {!library?.history.length ? <li className="text-[13px] text-text-3">Nothing yet.</li> : null}
          </ul>
        </section>
      </div>
    </div>
  );
};
