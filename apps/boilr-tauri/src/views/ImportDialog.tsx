import { useEffect, useRef, useState } from "react";
import { useApp } from "../App";
import { api, type Plan } from "../api";
import { Callout, Chip, Icon, Modal, errorText, plural, useToast } from "../ui";
import { OpRow } from "./OpRow";

/** The ops as compared before a write: artwork found meanwhile (icon path, notes) is not a change. */
const sameOps = (ops: Plan["ops"]) => JSON.stringify(ops.map((o) => ({ ...o, notes: [], new: o.new && { ...o.new, icon: "" } })));

/** "Sync with Steam": the exact changes, and nothing happens before the button is pressed. */
export const ImportDialog = ({ onClose }: { onClose: () => void }) => {
  const { reload, info } = useApp();
  const toast = useToast();
  const [plan, setPlan] = useState<Plan | null>(null);
  const [busy, setBusy] = useState(false);
  const [steamRunning, setSteamRunning] = useState(info?.steam_running ?? false);

  const load = () => api.planImport().then(setPlan).catch((e) => toast("bad", errorText(e)));
  useEffect(() => {
    void load();
    // Steam may have been started or closed since the app last looked.
    api.appInfo().then((i) => setSteamRunning(i.steam_running)).catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const s = plan?.summary ?? {};
  const changes = (s.create ?? 0) + (s.update ?? 0) + (s.remove ?? 0);

  const [stage, setStage] = useState<string | null>(null);
  /** Steam was closed by us for an import that has not finished yet. */
  const reopenSteam = useRef(false);
  // Dialog closed while Steam is still closed by us: start it again.
  useEffect(() => () => void (reopenSteam.current && api.startSteam().catch(() => undefined)), []);

  /** With Steam open: ask it to exit (never force-closed), import, then start it again. */
  const run = async () => {
    if (!plan) return;
    setBusy(true);
    let closedSteam = reopenSteam.current;
    try {
      let token = plan.token;
      // Ask right before closing: only a Steam that is open now is closed (and started again).
      if (!plan.simulation && (await api.appInfo()).steam_running) {
        setStage("Asking Steam to exit…");
        await api.closeSteam();
        setSteamRunning(false);
        closedSteam = true;
        // Steam saves its files on the way out, which outdates the confirmed plan. Go on only
        // if the changes are still exactly the ones shown; otherwise show the new plan.
        const fresh = await api.planImport();
        if (sameOps(fresh.ops) !== sameOps(plan.ops)) {
          setPlan(fresh);
          reopenSteam.current = true;
          closedSteam = false;
          toast("warn", "Steam changed its library while closing. Check the updated changes and press Sync again.");
          return;
        }
        token = fresh.token;
      }
      setStage("Syncing…");
      const outcome = await api.executeImport(token);
      await reload();
      if (outcome.record.simulated) toast("warn", `Simulation: ${plural(changes, "change")} would be made. Nothing was written to Steam.`);
      else toast("ok", `Steam is up to date: ${plural(changes, "change")} made.${closedSteam ? " Steam is starting again." : ""}`);
      onClose();
    } catch (e) {
      toast("bad", errorText(e));
      if (errorText(e).startsWith("Steam is running")) setSteamRunning(true);
      await load();
    } finally {
      // Steam was open before; open it again whether or not the import went through.
      if (closedSteam) {
        reopenSteam.current = false;
        await api.startSteam().catch((e) => toast("bad", errorText(e)));
      }
      setStage(null);
      setBusy(false);
    }
  };

  return (
    <Modal
      title="Sync with Steam"
      // Once started, the sync cannot be stopped halfway: closing is blocked until it ends.
      onClose={busy ? () => undefined : onClose}
      wide
      footer={
        <>
          <button type="button" className="btn-ghost" disabled={busy} onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="btn-primary" disabled={busy || !plan?.ops.length} onClick={() => void run()}>
            <Icon name="steam" /> {stage ?? `${plan?.simulation ? "Simulate sync" : "Sync now"} (${plural(changes, "change")})`}
          </button>
        </>
      }
    >
      {!plan ? (
        <p className="text-text-2">Preparing the plan…</p>
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            {[
              ["Added to Steam", s.create ?? 0, "ok"],
              ["Changed", s.update ?? 0, "info"],
              ["Removed from Steam", s.remove ?? 0, "bad"],
              ["Still to check", s.still_review ?? 0, "warn"],
            ].map(([label, value, tone]) => (
              <div key={label as string} className="rounded-lg border border-line bg-surface-2 p-3">
                <div className={`font-display text-2xl font-semibold text-${tone === "muted" ? "text" : tone}`}>{value as number}</div>
                <div className="text-xs text-text-2">{label as string}</div>
              </div>
            ))}
          </div>
          <p className="text-[13px] text-text-2">
            Steam account{plan.user_ids.length === 1 ? "" : "s"} {plan.user_ids.join(", ") || "—"}. Everything else in Steam stays exactly as it is, game files are never touched, and a verified backup is
            made first.{s.still_review ? ` ${plural(s.still_review, "game")} marked “Check this” wait until you look at them.` : ""}
          </p>
          {plan.simulation ? (
            <Callout tone="warn" icon="shield">
              Simulation mode is on: this shows exactly what would happen, but nothing will be written to Steam.
            </Callout>
          ) : null}
          {steamRunning && !plan.simulation ? (
            <Callout tone="info" icon="steam">
              Steam is open. Syncing asks Steam to exit normally (it is never force-closed), makes the changes, then starts Steam again.
            </Callout>
          ) : null}
          {plan.warnings.map((w) => (
            <Callout key={w} tone="warn">
              {w}
            </Callout>
          ))}
          {plan.blocked.length ? (
            <div>
              <h3 className="label mb-2">Left out ({plan.blocked.length})</h3>
              <ul className="space-y-1">
                {plan.blocked.map((b) => (
                  <li key={b.id + b.reason} className="flex gap-2 text-[13px]">
                    <Chip tone="warn">skipped</Chip> <b>{b.title}</b> <span className="text-text-2">{b.reason}</span>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          <div>
            <h3 className="label mb-2">Exact changes ({plan.ops.length})</h3>
            {plan.ops.length === 0 ? <p className="text-text-3">Steam is already up to date. Mark games to add or remove in My Games first.</p> : null}
            <ul className="space-y-1.5">
              {plan.ops.map((op, i) => (
                <OpRow key={i} op={op} />
              ))}
            </ul>
          </div>
        </div>
      )}
    </Modal>
  );
};
