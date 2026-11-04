import clsx from "clsx";
import { useState } from "react";
import type { Op, ShortcutView } from "../api";
import { Chip, Icon, type Tone } from "../ui";

const KIND: Record<Op["kind"], { label: string; tone: Tone; icon: string }> = {
  create: { label: "Add", tone: "ok", icon: "plus" },
  replace: { label: "Replace", tone: "accent", icon: "rebuild" },
  update: { label: "Update", tone: "info", icon: "edit" },
  remove: { label: "Remove", tone: "bad", icon: "trash" },
  keep: { label: "Keep", tone: "muted", icon: "check" },
  unresolved: { label: "Leave as is", tone: "warn", icon: "info" },
};

const FIELDS: [keyof ShortcutView, string][] = [
  ["name", "Name"],
  ["exe", "Executable"],
  ["start_dir", "Start in"],
  ["launch_options", "Launch options"],
  ["app_id", "Steam ID"],
];

/** One planned shortcut operation, expandable to the before/after of every field. */
export const OpRow = ({ op }: { op: Op }) => {
  const [open, setOpen] = useState(op.kind === "update");
  const k = KIND[op.kind];
  const title = op.new?.name ?? op.old?.name ?? "";
  return (
    <li className="rounded-lg border border-line bg-surface-2">
      <button type="button" onClick={() => setOpen(!open)} className="flex w-full items-center gap-2.5 px-3 py-2 text-left">
        <Chip tone={k.tone}>
          <Icon name={k.icon} size={11} /> {k.label}
        </Chip>
        <span className="text-[13px] font-semibold">{title}</span>
        {op.old && op.new && op.old.name !== op.new.name ? <span className="text-xs text-text-3">was “{op.old.name}”</span> : null}
        <span className="mono ml-auto max-w-[45%] truncate text-text-3">{(op.new ?? op.old)?.exe}</span>
        <Icon name="arrow" size={13} className={clsx("text-text-3 transition", open && "rotate-90")} />
      </button>
      {open ? (
        <div className="border-t border-line px-3 py-2.5">
          <table className="w-full text-xs">
            <tbody>
              {FIELDS.map(([key, label]) => {
                const before = op.old ? String(op.old[key] ?? "") : "";
                const after = op.new ? String(op.new[key] ?? "") : "";
                const changed = op.old && op.new && before !== after;
                if (!before && !after) return null;
                return (
                  <tr key={key} className="align-top">
                    <td className="w-28 py-0.5 text-text-3">{label}</td>
                    {op.old ? <td className={clsx("mono py-0.5 pr-3 break-all", changed && "text-bad line-through decoration-bad/50")}>{before || "—"}</td> : null}
                    {op.new ? <td className={clsx("mono py-0.5 break-all", changed && "text-ok")}>{after || "—"}</td> : null}
                  </tr>
                );
              })}
            </tbody>
          </table>
          {op.notes.length ? (
            <ul className="mt-2 space-y-0.5 text-xs text-warn">
              {op.notes.map((n) => (
                <li key={n}>• {n}</li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </li>
  );
};
