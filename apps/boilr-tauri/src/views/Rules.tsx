import clsx from "clsx";
import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useApp } from "../App";
import { api, type Rule, type RuleKind } from "../api";
import { Callout, Chip, Field, Icon, Modal, PageHeader, Toggle, errorText, plural, useToast } from "../ui";

const KINDS: { kind: RuleKind; label: string; placeholder: string; help: string }[] = [
  { kind: "dir_name", label: "Folder name", placeholder: "_Redist  or  Engine/Extras/Redist", help: "Any folder with this name (or sequence of names)." },
  { kind: "dir_path", label: "Full folder path", placeholder: "D:\\Games\\Tools", help: "Everything under this folder." },
  { kind: "file_name", label: "Executable name", placeholder: "unins*.exe", help: "File name; * and ? wildcards allowed." },
  { kind: "glob", label: "Glob pattern", placeholder: "D:/Games/**/Benchmark/*.exe", help: "Full path; ** spans folders." },
  { kind: "regex", label: "Regular expression", placeholder: "server\\.exe$", help: "Advanced: matched against the full path, case-insensitive." },
  { kind: "category", label: "Executable category", placeholder: "language_selector", help: "uninstaller, installer, redistributable, verification, language_selector, crash_reporter, updater, anti_cheat, helper" },
];
const kindLabel = (k: RuleKind) => KINDS.find((x) => x.kind === k)?.label ?? k;

export const Rules = () => {
  const { prefs, savePrefs, reload } = useApp();
  const toast = useToast();
  const [editing, setEditing] = useState<Rule | null>(null);
  const [preview, setPreview] = useState<{ rule: Rule; matches: { path: string; game: string }[] } | null>(null);
  const [allow, setAllow] = useState("");
  const [confirmReset, setConfirmReset] = useState(false);
  if (!prefs) return null;
  const rules = prefs.rules.rules;
  const custom = rules.filter((r) => !r.builtin);
  const builtin = rules.filter((r) => r.builtin);

  const saveRules = (next: Rule[], allowlist = prefs.rules.allowlist) => savePrefs({ ...prefs, rules: { rules: next, allowlist } });
  const showPreview = async (rule: Rule) => {
    try {
      setPreview({ rule, matches: await api.previewRule(rule) });
    } catch (e) {
      toast("bad", errorText(e));
    }
  };

  const row = (r: Rule) => (
    <li key={r.id} className={clsx("flex items-center gap-3 rounded-lg border border-line bg-surface-2 px-3 py-2", !r.enabled && "opacity-55")}>
      <Toggle checked={r.enabled} label={`Enable ${r.pattern}`} onChange={(v) => void saveRules(rules.map((x) => (x.id === r.id ? { ...x, enabled: v } : x)))} />
      <Chip>{kindLabel(r.kind)}</Chip>
      <span className="mono font-semibold">{r.pattern}</span>
      {r.inside_game_only ? <Chip tone="info" title="Only matches folders inside a game folder">inside games only</Chip> : null}
      <span className="min-w-0 flex-1 truncate text-xs text-text-3">{r.note}</span>
      <button type="button" className="btn-ghost btn-sm" onClick={() => void showPreview(r)}>
        <Icon name="eye" size={13} /> Preview
      </button>
      <button type="button" className="btn-ghost btn-sm" onClick={() => setEditing(r)}>
        <Icon name="edit" size={13} />
      </button>
      {!r.builtin ? (
        <button type="button" className="btn-ghost btn-sm !text-bad" onClick={() => void saveRules(rules.filter((x) => x.id !== r.id))} aria-label="Delete rule">
          <Icon name="trash" size={13} />
        </button>
      ) : null}
    </li>
  );

  return (
    <div className="space-y-5">
      <PageHeader
        title="Exclusion Rules"
        subtitle="Permanent rules applied before executables are scored. They survive restarts and updates."
        actions={
          <>
            <button
              type="button"
              className="btn-ghost"
              onClick={async () => {
                const p = await open({ filters: [{ name: "Rules", extensions: ["json"] }] });
                if (typeof p === "string") {
                  try {
                    await api.importJson(p, "rules");
                    await reload();
                    toast("ok", "Rules imported.");
                  } catch (e) {
                    toast("bad", errorText(e));
                  }
                }
              }}
            >
              <Icon name="upload" /> Import
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={async () => {
                const p = await save({ defaultPath: "BoilerSteamXL-rules.json", filters: [{ name: "Rules", extensions: ["json"] }] });
                if (p) await api.exportJson(p, "rules").then(() => toast("ok", "Rules exported.")).catch((e) => toast("bad", errorText(e)));
              }}
            >
              <Icon name="download" /> Export
            </button>
            <button type="button" className="btn-ghost" onClick={() => setConfirmReset(true)}>
              Reset to recommended
            </button>
            <button
              type="button"
              className="btn-primary"
              onClick={() => setEditing({ id: `u-${Date.now().toString(36)}`, kind: "file_name", pattern: "", enabled: true, builtin: false, inside_game_only: false, note: "" })}
            >
              <Icon name="plus" /> Add rule
            </button>
          </>
        }
      />
      <Callout>
        Rules are case-insensitive and treat <span className="mono">\</span> and <span className="mono">/</span> alike. Rules marked <i>inside games only</i> never hide a game
        folder itself, so a game called “Tools” or “Support” is still found. The search box elsewhere only filters what you see; these rules decide what is scanned.
      </Callout>

      <section>
        <h2 className="label mb-2">Your rules ({custom.length})</h2>
        {custom.length ? <ul className="space-y-1.5">{custom.map(row)}</ul> : <p className="text-[13px] text-text-3">None yet. “Ignore forever” in the review queue adds one here.</p>}
      </section>

      <section>
        <h2 className="label mb-2">Allowlist ({prefs.rules.allowlist.length})</h2>
        <p className="mb-2 text-xs text-text-3">Executables here are never excluded, whatever the rules say.</p>
        <ul className="space-y-1.5">
          {prefs.rules.allowlist.map((a) => (
            <li key={a} className="flex items-center gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2">
              <Icon name="check" className="text-ok" />
              <span className="mono flex-1 truncate">{a}</span>
              <button type="button" className="btn-ghost btn-sm" onClick={() => void saveRules(rules, prefs.rules.allowlist.filter((x) => x !== a))}>
                Remove
              </button>
            </li>
          ))}
        </ul>
        <div className="mt-2 flex gap-2">
          <input className="field mono" placeholder="D:\Games\Haze1\Language Selector.exe" value={allow} onChange={(e) => setAllow(e.target.value)} />
          <button
            type="button"
            className="btn-ghost"
            disabled={!allow.trim()}
            onClick={() => {
              void saveRules(rules, [...prefs.rules.allowlist, allow.trim()]);
              setAllow("");
            }}
          >
            Allow
          </button>
        </div>
      </section>

      <section>
        <h2 className="label mb-2">Recommended rules ({builtin.length})</h2>
        <ul className="space-y-1.5">{builtin.map(row)}</ul>
      </section>

      {editing ? <RuleEditor rule={editing} onClose={() => setEditing(null)} onSave={async (r) => {
        const exists = rules.some((x) => x.id === r.id);
        if (await saveRules(exists ? rules.map((x) => (x.id === r.id ? r : x)) : [...rules, r])) setEditing(null);
      }} onPreview={showPreview} /> : null}

      {preview ? (
        <Modal title={`Matches for “${preview.rule.pattern}”`} onClose={() => setPreview(null)}>
          <p className="mb-3 text-[13px] text-text-2">
            {plural(preview.matches.length, "known executable")} match{preview.matches.length === 1 ? "es" : ""} (from your last scan).
          </p>
          <ul className="space-y-1">
            {preview.matches.map((m) => (
              <li key={m.path} className="mono truncate text-text-2" title={m.path}>
                {m.path} {m.game ? <span className="text-text-3">({m.game})</span> : null}
              </li>
            ))}
          </ul>
        </Modal>
      ) : null}

      {confirmReset ? (
        <Modal
          title="Reset recommended rules?"
          onClose={() => setConfirmReset(false)}
          footer={
            <>
              <button type="button" className="btn-ghost" onClick={() => setConfirmReset(false)}>
                Cancel
              </button>
              <button
                type="button"
                className="btn-primary"
                onClick={async () => {
                  const rec = await api.recommendedRules();
                  if (await saveRules([...custom, ...rec.rules])) {
                    setConfirmReset(false);
                    toast("ok", "Recommended rules restored. Your own rules and allowlist were kept.");
                  }
                }}
              >
                Reset
              </button>
            </>
          }
        >
          <p className="text-text-2">The recommended rules go back to their original state. Your own rules and allowlist are kept.</p>
        </Modal>
      ) : null}
    </div>
  );
};

const RuleEditor = ({ rule, onClose, onSave, onPreview }: { rule: Rule; onClose: () => void; onSave: (r: Rule) => Promise<void>; onPreview: (r: Rule) => Promise<void> }) => {
  const [r, setR] = useState(rule);
  const [error, setError] = useState<string | null>(null);
  const meta = KINDS.find((k) => k.kind === r.kind) ?? KINDS[0];
  const validate = async () => {
    try {
      await api.validateRule(r.kind, r.pattern);
      setError(null);
      return true;
    } catch (e) {
      setError(errorText(e));
      return false;
    }
  };
  return (
    <Modal
      title={rule.pattern ? "Edit rule" : "New rule"}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn-ghost" onClick={async () => (await validate()) && (await onPreview(r))}>
            <Icon name="eye" /> Preview matches
          </button>
          <button type="button" className="btn-primary" onClick={async () => (await validate()) && (await onSave(r))}>
            Save rule
          </button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label="Type" hint={meta.help}>
          <select className="field" value={r.kind} onChange={(e) => setR({ ...r, kind: e.target.value as RuleKind })} disabled={r.builtin}>
            {KINDS.map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Pattern">
          <input className="field mono" value={r.pattern} placeholder={meta.placeholder} onChange={(e) => setR({ ...r, pattern: e.target.value })} autoFocus />
        </Field>
        {r.kind === "dir_name" ? (
          <label className="flex items-center gap-2 text-[13px]">
            <input type="checkbox" className="check" checked={r.inside_game_only} onChange={(e) => setR({ ...r, inside_game_only: e.target.checked })} />
            Only inside game folders (never hide a game with this name)
          </label>
        ) : null}
        <Field label="Note">
          <input className="field" value={r.note} onChange={(e) => setR({ ...r, note: e.target.value })} placeholder="Why this rule exists" />
        </Field>
        {error ? <Callout tone="bad">{error}</Callout> : null}
      </div>
    </Modal>
  );
};
