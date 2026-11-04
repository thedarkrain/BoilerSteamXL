import clsx from "clsx";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import logo from "../assets/logo.png";
import { useApp } from "../App";
import { api, type Preferences, type SteamInfo } from "../api";
import { Callout, Chip, Field, Icon, errorText, useToast } from "../ui";
import { GameFolders } from "./Settings";

const STEPS = ["Welcome", "Steam", "Game folders", "Safety"];

/** First start: detect Steam, confirm game folders, explain the safety model. */
export const FirstRun = () => {
  const { prefs, savePrefs, refresh, reload } = useApp();
  const toast = useToast();
  const [step, setStep] = useState(0);
  const [steam, setSteam] = useState<SteamInfo | null>(null);
  const [steamPath, setSteamPath] = useState("");
  const [draft, setDraft] = useState<Preferences | null>(prefs);

  useEffect(() => {
    api.steamInfo().then((s) => {
      setSteam(s);
      setSteamPath(s.path);
    });
    api.suggestRoots().then((roots) => {
      setDraft((d) => (d && d.scan_roots.length === 0 ? { ...d, scan_roots: roots.map((path) => ({ path, enabled: true, max_depth: 6 })) } : d));
    });
  }, []);
  if (!draft) return null;

  const checkSteam = async () => {
    try {
      await api.checkSteamPath(steamPath);
      const saved = await savePrefs({ ...draft, steam_path: steamPath === steam?.path ? null : steamPath });
      if (saved) {
        setSteam(await api.steamInfo());
        setDraft({ ...draft, steam_path: steamPath === steam?.path ? null : steamPath });
        setStep(2);
      }
    } catch (e) {
      toast("bad", errorText(e));
    }
  };

  const finish = async () => {
    if (await savePrefs({ ...draft, first_run_complete: true, simulation_mode: true })) {
      await reload();
      void refresh();
    }
  };

  return (
    <div className="hero-glow flex h-full items-center justify-center p-8">
      <div className="card animate-in w-full max-w-2xl p-8">
        <div className="mb-6 flex items-center gap-3">
          <img src={logo} alt="" className="h-10 w-10 rounded-xl" />
          <div className="font-display text-xl font-bold">
            BoilerSteam<span className="gradient-text">XL</span>
          </div>
          <ol className="ml-auto flex gap-1.5">
            {STEPS.map((s, i) => (
              <li key={s} className={clsx("h-1.5 w-8 rounded-full", i <= step ? "bg-accent" : "bg-surface-3")} title={s} />
            ))}
          </ol>
        </div>

        {step === 0 ? (
          <div className="space-y-4">
            <h1 className="h1">Organize your games into a clean, duplicate-free Steam library</h1>
            <p className="text-text-2">BoilerSteamXL finds the games on your PC (repacks and loose folders included), names them from their folders, filters out installers and utilities, and adds only the games you approve to Steam.</p>
            <ul className="space-y-2 text-[13px]">
              {["Discover automatically", "Add manually", "Validate everything", "Import safely", "Refresh whenever you want"].map((t) => (
                <li key={t} className="flex items-center gap-2">
                  <Icon name="check" className="text-ok" /> {t}
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        {step === 1 ? (
          <div className="space-y-4">
            <h1 className="h1">Where is Steam installed?</h1>
            <p className="text-text-2">
              This is the <b>Steam installation folder</b> (with steam.exe and userdata), usually <span className="mono">C:\Program Files (x86)\Steam</span>. It is <b>not</b> your Steam library
              folder (like D:\SteamLibrary) and <b>not</b> your games folder (like D:\Games).
            </p>
            <Field label="Steam installation folder">
              <div className="flex gap-2">
                <input className="field mono" value={steamPath} onChange={(e) => setSteamPath(e.target.value)} />
                <button
                  type="button"
                  className="btn-ghost"
                  onClick={async () => {
                    const p = await open({ directory: true });
                    if (typeof p === "string") setSteamPath(p);
                  }}
                >
                  Browse
                </button>
              </div>
            </Field>
            {steam?.valid && steamPath === steam.path ? (
              <Callout tone="ok">
                Found Steam with {steam.users.length} account{steam.users.length === 1 ? "" : "s"}: {steam.users.map((u) => u.persona ?? u.user_id).join(", ")}. Library folders:{" "}
                {steam.library_folders.join(", ")}.
              </Callout>
            ) : steam?.error && steamPath === steam.path ? (
              <Callout tone="bad">{steam.error}</Callout>
            ) : null}
          </div>
        ) : null}

        {step === 2 ? (
          <div className="space-y-4">
            <h1 className="h1">Which folders hold your games?</h1>
            <p className="text-text-2">Folders with your non-Steam games, for example <span className="mono">D:\Games</span>. Nested folders like <span className="mono">D:\Games\PLUG&PLAY\A Way Out</span> are found automatically.</p>
            <GameFolders prefs={draft} onChange={setDraft} />
            {draft.scan_roots.length === 0 ? <Callout tone="warn">Add at least one folder.</Callout> : null}
          </div>
        ) : null}

        {step === 3 ? (
          <div className="space-y-4">
            <h1 className="h1">Your Steam library is protected</h1>
            <ul className="space-y-2.5 text-[13px]">
              <li className="flex gap-2"><Icon name="shield" className="mt-0.5 text-ok" /> Scanning and refreshing only read. They never change Steam.</li>
              <li className="flex gap-2"><Icon name="review" className="mt-0.5 text-ok" /> Every game needs your approval, then a final confirmation of the exact plan.</li>
              <li className="flex gap-2"><Icon name="backups" className="mt-0.5 text-ok" /> A verified backup is made before every change, with one-click restore.</li>
              <li className="flex gap-2"><Icon name="games" className="mt-0.5 text-ok" /> Game files and saves are never touched.</li>
            </ul>
            <Callout tone="warn" icon="shield">
              BoilerSteamXL starts in <b>Simulation mode</b>: you can try everything, and nothing is written to Steam until you switch it off in Settings → Safety.{" "}
              <Chip tone="warn">On</Chip>
            </Callout>
          </div>
        ) : null}

        <div className="mt-8 flex justify-between">
          <button type="button" className="btn-ghost" disabled={step === 0} onClick={() => setStep(step - 1)}>
            Back
          </button>
          {step === 0 ? (
            <button type="button" className="btn-primary" onClick={() => setStep(1)}>
              Get started <Icon name="arrow" />
            </button>
          ) : step === 1 ? (
            <button type="button" className="btn-primary" disabled={!steamPath.trim()} onClick={() => void checkSteam()}>
              Continue <Icon name="arrow" />
            </button>
          ) : step === 2 ? (
            <button type="button" className="btn-primary" disabled={draft.scan_roots.length === 0} onClick={() => setStep(3)}>
              Continue <Icon name="arrow" />
            </button>
          ) : (
            <button type="button" className="btn-primary" onClick={() => void finish()}>
              <Icon name="refresh" /> Finish and scan my games
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
