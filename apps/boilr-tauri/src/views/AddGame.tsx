import clsx from "clsx";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../App";
import { api, type DiscoveredGame, type ManualCheck } from "../api";
import { Callout, Chip, Field, Icon, Modal, errorText, fileName, fmtSize, useToast } from "../ui";
import { browseExe } from "./EntryEditor";

/** `"D:\Game\game.exe" -windowed` → exe and arguments. */
export const splitTarget = (text: string): { exe: string; args: string } => {
  const t = text.trim();
  if (t.startsWith('"')) {
    const end = t.indexOf('"', 1);
    if (end > 0) return { exe: t.slice(1, end), args: t.slice(end + 1).trim() };
  }
  const m = t.match(/^(.*?\.(?:exe|bat|cmd|lnk))(\s+.*)?$/i);
  return m ? { exe: m[1], args: (m[2] ?? "").trim() } : { exe: t, args: "" };
};

const STEPS = ["Choose executable", "Configure", "Check duplicates", "Add to review"];
const parentOf = (path: string) => path.replace(/[\\/][^\\/]+$/, "");

export const AddGame = ({ onClose }: { onClose: () => void }) => {
  const { reload, go } = useApp();
  const toast = useToast();
  const [step, setStep] = useState(0);
  const [target, setTarget] = useState("");
  const [folder, setFolder] = useState<{ path: string; games: DiscoveredGame[] } | null>(null);
  const [title, setTitle] = useState("");
  const [exe, setExe] = useState("");
  const [args, setArgs] = useState("");
  const [dir, setDir] = useState("");
  const [collection, setCollection] = useState("");
  const [tags, setTags] = useState("");
  const [notes, setNotes] = useState("");
  const [check, setCheck] = useState<ManualCheck | null>(null);
  const [confirmSeparate, setConfirmSeparate] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);

  const useExe = async (path: string, launchArgs = "") => {
    const c = await api.checkManual(path, undefined, launchArgs);
    if (!c.exists) {
      toast("bad", `${path} does not exist.`);
      return;
    }
    setExe(path);
    setArgs(launchArgs);
    setTitle(c.suggested_title);
    setDir(parentOf(path));
    setCheck(c);
    setStep(1);
  };

  useEffect(() => {
    let stop: (() => void) | undefined;
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          const p = event.payload;
          if (p.type === "over" || p.type === "enter") setDragging(true);
          else if (p.type === "leave") setDragging(false);
          else if (p.type === "drop") {
            setDragging(false);
            const file = p.paths[0];
            if (file) void useExe(file);
          }
        })
      )
      .then((unlisten) => (stop = unlisten))
      .catch(() => undefined);
    return () => stop?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const analyze = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      const result = await api.analyzeFolder(picked);
      setFolder({ path: picked, games: result.games });
    } catch (e) {
      toast("bad", errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const goCheck = async () => {
    setConfirmSeparate(false);
    setBusy(true);
    try {
      setCheck(await api.checkManual(exe, title, args));
      setStep(2);
    } catch (e) {
      toast("bad", errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const add = async (allowDuplicate: boolean) => {
    setBusy(true);
    try {
      await api.addManual(
        {
          exe,
          title,
          launch_options: args,
          ...(dir !== parentOf(exe) ? { start_dir: dir } : {}),
          collection,
          notes,
          tags: tags.split(",").map((t) => t.trim()).filter(Boolean),
        },
        allowDuplicate
      );
      await reload();
      setStep(3);
    } catch (e) {
      toast("bad", errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const updateExisting = async (entryId: string) => {
    setBusy(true);
    try {
      await api.editEntry(entryId, { exe, title, launch_options: args, ...(dir !== parentOf(exe) ? { start_dir: dir } : {}) });
      await reload();
      toast("ok", "The existing game was updated; it goes back to review.");
      onClose();
      go("review");
    } catch (e) {
      toast("bad", errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const dups = check?.duplicates ?? [];

  return (
    <Modal title="Add a game" onClose={onClose} wide>
      <ol className="mb-5 flex items-center gap-2">
        {STEPS.map((s, i) => (
          <li key={s} className="flex flex-1 items-center gap-2">
            <span className={clsx("grid h-6 w-6 flex-none place-items-center rounded-full text-xs font-bold", i < step ? "bg-ok text-white" : i === step ? "bg-accent text-white" : "bg-surface-3 text-text-3")}>
              {i < step ? <Icon name="check" size={12} /> : i + 1}
            </span>
            <span className={clsx("text-xs font-semibold", i === step ? "text-text" : "text-text-3")}>{s}</span>
            {i < STEPS.length - 1 ? <span className="h-px flex-1 bg-line" /> : null}
          </li>
        ))}
      </ol>

      {step === 0 ? (
        <div className="space-y-4">
          <div className={clsx("grid place-items-center rounded-xl border-2 border-dashed px-6 py-10 text-center transition", dragging ? "border-accent bg-accent/8" : "border-line-strong")}>
            <Icon name="upload" size={30} className="text-text-3" />
            <p className="mt-3 font-semibold">Drop the game's .exe here</p>
            <p className="text-xs text-text-3">It is checked, never run.</p>
            <div className="mt-4 flex gap-2">
              <button type="button" className="btn-primary" onClick={async () => { const p = await browseExe(); if (p) await useExe(p); }}>
                <Icon name="folder" /> Browse for .exe
              </button>
              <button type="button" className="btn-ghost" onClick={() => void analyze()} disabled={busy}>
                <Icon name="search" /> {busy ? "Analysing…" : "Analyse a folder"}
              </button>
            </div>
          </div>
          <Field label="…or paste a path or shortcut target" hint={<>For example <span className="mono">"D:\Games\MyGame\GameLauncher.exe" -windowed</span></>}>
            <div className="flex gap-2">
              <input className="field mono" value={target} onChange={(e) => setTarget(e.target.value)} placeholder="D:\Games\MyGame\Game.exe" />
              <button type="button" className="btn-ghost" disabled={!target.trim()} onClick={() => { const t = splitTarget(target); void useExe(t.exe, t.args); }}>
                Continue
              </button>
            </div>
          </Field>
          {folder ? (
            <div>
              <h3 className="label mb-2">Executables in {folder.path}</h3>
              {folder.games.length === 0 ? <p className="text-text-3">No usable executables (everything matched an exclusion rule).</p> : null}
              {folder.games.map((g) => (
                <div key={g.game_dir} className="mb-3">
                  <p className="mb-1 text-[13px] font-semibold">
                    {g.title} <span className="text-xs font-normal text-text-3">(name from {g.title_source.toLowerCase()})</span>
                  </p>
                  <ul className="space-y-1">
                    {g.candidates.map((c, i) => (
                      <li key={c.path}>
                        <button type="button" className="flex w-full items-center gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2 text-left hover:border-accent/60" onClick={() => void useExe(c.path, c.arguments ?? "")}>
                          <span className="text-[13px] font-semibold">{c.file_name}</span>
                          {i === 0 ? <Chip tone="ok">Recommended</Chip> : null}
                          <span className="mono ml-2 min-w-0 flex-1 truncate text-text-3">{c.path}</span>
                          <span className="text-xs text-text-3">{fmtSize(c.size)}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}

      {step === 1 ? (
        <div className="grid gap-5 lg:grid-cols-[1.4fr_1fr]">
          <div className="space-y-3">
            <Field label="Game name">
              <input className="field" value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
            </Field>
            <Field label="Executable">
              <div className="flex gap-2">
                <input
                  className="field mono"
                  value={exe}
                  onChange={(e) => {
                    if (dir === parentOf(exe)) setDir(parentOf(e.target.value));
                    setExe(e.target.value);
                  }}
                />
                <button
                  type="button"
                  className="btn-ghost"
                  onClick={async () => {
                    const p = await browseExe(dir);
                    if (!p) return;
                    if (dir === parentOf(exe)) setDir(parentOf(p));
                    setExe(p);
                  }}
                >
                  Browse
                </button>
              </div>
            </Field>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Start in (working directory)">
                <input className="field mono" value={dir} onChange={(e) => setDir(e.target.value)} />
              </Field>
              <Field label="Launch options">
                <input className="field mono" value={args} onChange={(e) => setArgs(e.target.value)} placeholder="-windowed" />
              </Field>
              <Field label="Platform / collection" hint="Shown in My Games; e.g. Repacks, Emulators">
                <input className="field" value={collection} onChange={(e) => setCollection(e.target.value)} />
              </Field>
              <Field label="Tags" hint="Comma separated">
                <input className="field" value={tags} onChange={(e) => setTags(e.target.value)} />
              </Field>
            </div>
            <Field label="Notes">
              <textarea className="field min-h-14" value={notes} onChange={(e) => setNotes(e.target.value)} />
            </Field>
            {check && !check.is_exe ? <Callout tone="warn">This is not an .exe file. Steam can still start it, but check it is the right file.</Callout> : null}
          </div>
          <div>
            <h3 className="label mb-2">Steam shortcut preview</h3>
            <div className="rounded-xl border border-line bg-bg-2 p-4 text-xs">
              <div className="mb-3 text-base font-semibold">{title || "Untitled"}</div>
              <dl className="space-y-1.5">
                {[
                  ["Target", `"${exe}"`],
                  ["Start in", `"${dir}"`],
                  ["Launch options", args || "—"],
                ].map(([k, v]) => (
                  <div key={k}>
                    <dt className="text-text-3">{k}</dt>
                    <dd className="mono break-all">{v}</dd>
                  </div>
                ))}
              </dl>
            </div>
            <p className="mt-3 text-xs text-text-3">Artwork is downloaded automatically; you can change it later in the game's details.</p>
          </div>
        </div>
      ) : null}

      {step === 2 ? (
        <div className="space-y-4">
          {dups.length === 0 ? (
            <Callout tone="ok" icon="check">
              No existing Steam shortcut or BoilerSteamXL game uses <b>{fileName(exe)}</b>.
            </Callout>
          ) : (
            <>
              <Callout tone="warn">This executable is already known. Choose what to do. Nothing is duplicated without your confirmation.</Callout>
              <ul className="space-y-2">
                {dups.map((d, i) => (
                  <li key={i} className="flex items-center gap-3 rounded-lg border border-line bg-surface-2 px-3 py-2.5">
                    <Chip tone={d.kind === "entry" ? "accent" : "info"}>{d.kind === "entry" ? "BoilerSteamXL" : d.kind === "official" ? "Steam game" : "Steam shortcut"}</Chip>
                    <div className="min-w-0 flex-1">
                      <div className="text-[13px] font-semibold">{d.name}</div>
                      <div className="mono truncate text-text-3">{d.path}</div>
                    </div>
                    {d.entry_id ? (
                      <button type="button" className="btn-ghost btn-sm" disabled={busy} onClick={() => void updateExisting(d.entry_id!)}>
                        Update existing entry
                      </button>
                    ) : null}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      ) : null}

      {step === 3 ? (
        <div className="py-6 text-center">
          <span className="mx-auto grid h-14 w-14 place-items-center rounded-full bg-ok/15 text-ok">
            <Icon name="check" size={28} />
          </span>
          <p className="h2 mt-4">“{title}” is in the review queue</p>
          <p className="mx-auto mt-2 max-w-md text-text-2">It has not been written to Steam. Approve it in New Games, then confirm the import. It is saved permanently and survives every refresh.</p>
        </div>
      ) : null}

      <div className="mt-6 flex items-center justify-between border-t border-line pt-4">
        <button type="button" className="btn-ghost" onClick={step === 0 || step === 3 ? onClose : () => setStep(step - 1)}>
          {step === 0 ? "Cancel" : step === 3 ? "Close" : "Back"}
        </button>
        {step === 1 ? (
          <button type="button" className="btn-primary" disabled={!exe.trim() || !title.trim() || busy} onClick={() => void goCheck()}>
            Check for duplicates <Icon name="arrow" />
          </button>
        ) : null}
        {step === 2 ? (
          dups.length === 0 ? (
            <button type="button" className="btn-primary" disabled={busy} onClick={() => void add(false)}>
              Add to review queue
            </button>
          ) : (
            <div className="flex gap-2">
              <button type="button" className="btn-ghost" onClick={onClose}>
                Keep existing
              </button>
              {confirmSeparate ? (
                <button type="button" className="btn-danger" disabled={busy} onClick={() => void add(true)}>
                  Yes, add a separate shortcut
                </button>
              ) : (
                <button type="button" className="btn-ghost" onClick={() => setConfirmSeparate(true)}>
                  Add separately anyway…
                </button>
              )}
            </div>
          )
        ) : null}
        {step === 3 ? (
          <button type="button" className="btn-primary" onClick={() => { onClose(); go("review"); }}>
            Go to New Games
          </button>
        ) : null}
      </div>
    </Modal>
  );
};
