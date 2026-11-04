import { useMemo, useState } from "react";
import { useApp } from "../App";
import { displayTitle } from "../api";
import { Callout, Chip, Empty, Icon, PageHeader, StatusChip, fileName, plural } from "../ui";

/** What the last scan saw: detections per game folder and everything the rules ignored. */
export const Discover = () => {
  const { library, prefs, refresh, scanning, go } = useApp();
  const [showIgnored, setShowIgnored] = useState(false);
  const s = library?.last_scan;
  const folderGames = useMemo(() => (library?.entries ?? []).filter((e) => e.source === "folder" && e.status !== "ignored"), [library]);
  const ignored = useMemo(
    () => [...folderGames.flatMap((e) => e.excluded.map((x) => ({ ...x, game: displayTitle(e) }))), ...(s?.orphan_excluded ?? []).map((x) => ({ ...x, game: "" }))],
    [folderGames, s]
  );
  const candidates = folderGames.reduce((n, e) => n + e.candidates.length, 0);

  return (
    <div className="space-y-5">
      <PageHeader
        title="Discover Games"
        subtitle={<>Scans {prefs?.scan_roots.filter((r) => r.enabled).map((r) => r.path).join(", ") || "no folders yet"}. Scanning never changes Steam.</>}
        actions={
          <>
            <button type="button" className="btn-ghost" onClick={() => go("settings")}>
              <Icon name="folder" /> Game folders
            </button>
            <button type="button" className="btn-primary" disabled={scanning} onClick={() => void refresh()}>
              <Icon name="refresh" className={scanning ? "animate-spin" : ""} /> Scan now
            </button>
          </>
        }
      />
      {s ? (
        <div className="card flex flex-wrap items-center gap-3 p-4 text-[13px]">
          <Icon name="search" className="text-accent" />
          <b className="tabular-nums">{plural(s.exes_found, "executable")} found</b>
          <Icon name="arrow" className="text-text-3" />
          <b className="tabular-nums text-warn">{s.exes_excluded} ignored</b>
          <Icon name="arrow" className="text-text-3" />
          <b className="tabular-nums text-ok">{plural(candidates, "candidate")} reviewed</b>
          <span className="text-text-3">
            across {plural(s.folders_scanned, "folder")} · {plural(folderGames.length, "game folder")}
          </span>
          <label className="ml-auto flex items-center gap-2 text-xs text-text-2">
            <input type="checkbox" className="check" checked={showIgnored} onChange={(e) => setShowIgnored(e.target.checked)} />
            Show ignored executables
          </label>
        </div>
      ) : null}
      {s?.root_errors.map(([root, e]) => (
        <Callout key={root} tone="warn">
          <b>{root}</b>: {e}
        </Callout>
      ))}

      {showIgnored ? (
        <section className="card p-4">
          <h2 className="h2 mb-3">Ignored executables ({ignored.length})</h2>
          <p className="mb-3 text-xs text-text-3">Kept out of the game list by your exclusion rules. Allow one from the game's details if a rule was wrong.</p>
          <ul className="grid gap-1.5 md:grid-cols-2">
            {ignored.map((x) => (
              <li key={x.path} className="flex items-center gap-2 rounded-md border border-line bg-surface-2 px-2.5 py-1.5 text-xs">
                <Icon name="ban" className="text-text-3" size={13} />
                <span className="font-semibold">{fileName(x.path)}</span>
                {x.game ? <Chip>{x.game}</Chip> : null}
                <span className="ml-auto truncate text-text-3">{x.reason}</span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {folderGames.length === 0 ? (
        <Empty icon="search" title="No scan results yet">
          Press Scan now to look through your game folders.
        </Empty>
      ) : (
        <div className="panel overflow-hidden">
          <table className="w-full text-[13px]">
            <thead className="bg-surface-2 text-left text-xs text-text-3">
              <tr>
                <th className="px-4 py-2.5 font-semibold">Game (name source)</th>
                <th className="px-4 py-2.5 font-semibold">Selected executable</th>
                <th className="px-4 py-2.5 font-semibold">Detection</th>
                <th className="px-4 py-2.5 font-semibold">Ignored</th>
                <th className="px-4 py-2.5 font-semibold">Status</th>
              </tr>
            </thead>
            <tbody>
              {folderGames.map((e) => (
                <tr key={e.id} className="border-t border-line">
                  <td className="px-4 py-2.5">
                    <div className="font-semibold">{displayTitle(e)}</div>
                    <div className="text-xs text-text-3">{e.custom_title ? "Your correction" : e.title_source} · {fileName(e.game_dir)}</div>
                  </td>
                  <td className="px-4 py-2.5">
                    <div className="font-medium">{e.exe ? fileName(e.exe) : "—"}</div>
                    <div className="text-xs text-text-3">{e.candidates.length > 1 ? `${e.candidates.length} candidates` : "1 candidate"}{e.user_exe ? " · your choice" : ""}</div>
                  </td>
                  <td className="px-4 py-2.5">{e.uncertain ? <Chip tone="warn">Uncertain</Chip> : <span className="font-bold text-ok">{e.confidence}%</span>}</td>
                  <td className="px-4 py-2.5 text-text-2">{e.excluded.length || "—"}</td>
                  <td className="px-4 py-2.5">
                    <StatusChip status={e.status} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="text-xs text-text-3">
        Detection % is the best executable's score (0–99) from the documented scoring rules: name match, folder depth, file size, metadata and penalties for tools. It is
        “Uncertain” when the score is under 65 or a second executable scores within 10 points.
      </p>
    </div>
  );
};
