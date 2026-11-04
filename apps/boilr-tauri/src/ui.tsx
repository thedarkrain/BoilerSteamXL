// Small shared building blocks for the BoilerSteamXL interface.
import clsx from "clsx";
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { STATUS_LABEL, STATUS_TONE, type Status } from "./api";

const PATHS: Record<string, string> = {
  dashboard: "M3 13h8V3H3zm10 8h8V11h-8zM3 21h8v-6H3zm10-18v6h8V3z",
  games: "M6 11h4m-2-2v4m7-1h.01M18 10h.01M7 6h10a5 5 0 0 1 4.9 6l-.9 4.4A3 3 0 0 1 15 18l-1.5-2h-3L9 18a3 3 0 0 1-6-1.6L2.1 12A5 5 0 0 1 7 6z",
  review: "M9 11l3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11",
  library: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20M4 4.5A2.5 2.5 0 0 1 6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5z",
  rebuild: "M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5",
  rules: "M22 3H2l8 9.5V19l4 2v-8.5z",
  artwork: "M21 19V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2zM8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM21 15l-5-5L5 21",
  automation: "M12 8v4l3 3m6-3a9 9 0 1 1-18 0 9 9 0 0 1 18 0z",
  backups: "M3 3v5h5M3.05 13A9 9 0 1 0 6 5.3L3 8M12 7v5l4 2",
  settings: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm7.4-3a7.4 7.4 0 0 0-.1-1.3l2-1.6-2-3.4-2.4 1a7.6 7.6 0 0 0-2.2-1.3L14.3 3h-4l-.4 2.4a7.6 7.6 0 0 0-2.2 1.3l-2.4-1-2 3.4 2 1.6a7.4 7.4 0 0 0 0 2.6l-2 1.6 2 3.4 2.4-1a7.6 7.6 0 0 0 2.2 1.3l.4 2.4h4l.4-2.4a7.6 7.6 0 0 0 2.2-1.3l2.4 1 2-3.4-2-1.6c.1-.4.1-.9.1-1.3z",
  refresh: "M21 12a9 9 0 0 1-15.5 6.3L3 16m0 5v-5h5M3 12a9 9 0 0 1 15.5-6.3L21 8m0-5v5h-5",
  plus: "M12 5v14M5 12h14",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  check: "M20 6L9 17l-5-5",
  x: "M18 6L6 18M6 6l12 12",
  alert: "M12 9v4m0 4h.01M10.3 3.9L1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
  search: "M21 21l-4.3-4.3M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14z",
  grid: "M3 3h7v7H3zm11 0h7v7h-7zm0 11h7v7h-7zM3 14h7v7H3z",
  list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  play: "M6 4l14 8-14 8z",
  trash: "M3 6h18M8 6V4h8v2m-9 0l1 14h8l1-14",
  link: "M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7",
  info: "M12 16v-4m0-4h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0z",
  eye: "M1 12s4-8 11-8 11 8 11 8-4 8-11 8S1 12 1 12zm11 3a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  ban: "M4.9 4.9l14.2 14.2M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0z",
  shield: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
  external: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6M15 3h6v6M10 14L21 3",
  download: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3",
  upload: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12",
  edit: "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7M18.5 2.5a2.1 2.1 0 0 1 3 3L12 15l-4 1 1-4z",
  arrow: "M5 12h14m-6-6l6 6-6 6",
  back: "M19 12H5m6 6l-6-6 6-6",
  sparkle: "M12 3l1.9 5.6L19.5 10l-5.6 1.9L12 17.5l-1.9-5.6L4.5 10l5.6-1.4z",
  steam: "M12 2a10 10 0 0 0-9.9 8.6l5.3 2.2a2.8 2.8 0 0 1 1.7-.5l2.4-3.4a3.8 3.8 0 1 1 3.8 3.8l-3.4 2.4a2.8 2.8 0 0 1-5.6.3l-3.8-1.6A10 10 0 1 0 12 2z",
  terminal: "M4 17l6-6-6-6m8 14h8",
  user: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2m8-10a4 4 0 1 0 0-8 4 4 0 0 0 0 8z",
};

export const Icon = ({ name, className, size = 16 }: { name: keyof typeof PATHS | string; className?: string; size?: number }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={1.8}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={clsx("flex-none", className)}
    aria-hidden="true"
  >
    <path d={PATHS[name] ?? PATHS.info} />
  </svg>
);

const TONES = {
  accent: "border-accent/30 bg-accent/12 text-accent",
  ok: "border-ok/30 bg-ok/12 text-ok",
  warn: "border-warn/30 bg-warn/12 text-warn",
  bad: "border-bad/30 bg-bad/12 text-bad",
  info: "border-info/30 bg-info/12 text-info",
  muted: "border-line-strong bg-surface-3 text-text-2",
} as const;
export type Tone = keyof typeof TONES;

export const Chip = ({ tone = "muted", children, title }: { tone?: Tone; children: ReactNode; title?: string }) => (
  <span className={clsx("chip", TONES[tone])} title={title}>
    {children}
  </span>
);

export const StatusChip = ({ status }: { status: Status }) => <Chip tone={STATUS_TONE[status]}>{STATUS_LABEL[status]}</Chip>;

export const Toggle = ({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) => (
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    disabled={disabled}
    className="toggle disabled:opacity-40"
    onClick={() => onChange(!checked)}
  >
    <span />
  </button>
);

export const Field = ({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) => (
  <label className="block">
    <span className="mb-1 block text-xs font-semibold text-text-2">{label}</span>
    {children}
    {hint ? <span className="mt-1 block text-xs text-text-3">{hint}</span> : null}
  </label>
);

/** Open dialogs, innermost last. */
const openModals: (() => void)[] = [];

export const Modal = ({
  title,
  onClose,
  children,
  footer,
  wide,
}: {
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}) => {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  // Escape closes only the top dialog (a picker opened inside game details, not both).
  useEffect(() => {
    const close = () => onCloseRef.current();
    openModals.push(close);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && openModals.at(-1) === close && close();
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      openModals.splice(openModals.indexOf(close), 1);
    };
  }, []);
  // Drawn on <body>: inside an animated (transformed) view, "fixed" would follow the page, not the window.
  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4 backdrop-blur-sm" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        className={clsx("card animate-in flex max-h-[calc(100vh-2rem)] w-full flex-col overflow-hidden", wide ? "max-w-3xl" : "max-w-xl")}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
          <h2 className="h2">{title}</h2>
          <button type="button" className="rounded-md p-1 text-text-3 hover:bg-surface-3 hover:text-text" onClick={onClose} aria-label="Close">
            <Icon name="x" />
          </button>
        </div>
        <div className="scroll-quiet min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-5 py-4">{children}</div>
        {footer ? <div className="flex items-center justify-end gap-2 border-t border-line bg-surface-2/60 px-5 py-3">{footer}</div> : null}
      </div>
    </div>,
    document.body
  );
};

const hue = (text: string) => {
  let h = 0;
  for (const ch of text) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return h % 360;
};

/** Cover art; without one, a gradient tile with the game's own icon (or its initials). */
export const Cover = ({ title, src, icon, className }: { title: string; src?: string | null; icon?: string | null; className?: string }) => {
  const [failed, setFailed] = useState(false);
  const [iconFailed, setIconFailed] = useState(false);
  useEffect(() => {
    setFailed(false);
    setIconFailed(false);
  }, [src, icon]);
  const h = hue(title);
  const words = title.replace(/[^\p{L}\p{N} ]/gu, " ").split(/\s+/).filter(Boolean);
  const letters = (words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?").slice(0, 2)).toUpperCase();
  if (src && !failed) {
    return <img src={src} alt="" loading="lazy" decoding="async" className={clsx("h-full w-full object-cover", className)} onError={() => setFailed(true)} />;
  }
  const showIcon = icon && !iconFailed;
  return (
    <div
      className={clsx("relative flex h-full w-full flex-col items-center justify-center p-3", className)}
      style={{ background: `linear-gradient(150deg, hsl(${h} 55% 34%), hsl(${(h + 50) % 360} 60% 16%))` }}
    >
      {showIcon ? (
        <>
          <img src={icon} alt="" loading="lazy" decoding="async" className="h-[38%] max-h-24 w-auto object-contain drop-shadow-xl" onError={() => setIconFailed(true)} />
        </>
      ) : (
        <span className="absolute bottom-3 left-3 font-display text-3xl font-bold text-white/90 drop-shadow">{letters}</span>
      )}
    </div>
  );
};

export const Stat = ({ label, value, tone, icon, onClick }: { label: string; value: ReactNode; tone?: Tone; icon: string; onClick?: () => void }) => (
  <button
    type="button"
    onClick={onClick}
    disabled={!onClick}
    className="card group flex items-center gap-3 p-4 text-left transition hover:border-line-strong disabled:cursor-default"
  >
    <span className={clsx("grid h-10 w-10 place-items-center rounded-lg border", TONES[tone ?? "accent"])}>
      <Icon name={icon} size={18} />
    </span>
    <span>
      <span className="block font-display text-2xl font-semibold leading-tight">{value}</span>
      <span className="text-xs text-text-2">{label}</span>
    </span>
  </button>
);

export const Empty = ({ icon, title, children }: { icon: string; title: string; children?: ReactNode }) => (
  <div className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center">
    <span className="grid h-14 w-14 place-items-center rounded-2xl border border-line bg-surface-2 text-text-3">
      <Icon name={icon} size={26} />
    </span>
    <p className="h2">{title}</p>
    {children ? <div className="max-w-md text-text-2">{children}</div> : null}
  </div>
);

export const Callout = ({ tone = "info", icon, children }: { tone?: Tone; icon?: string; children: ReactNode }) => (
  <div className={clsx("flex gap-2.5 rounded-lg border px-3.5 py-2.5 text-[13px]", TONES[tone])}>
    <Icon name={icon ?? (tone === "bad" || tone === "warn" ? "alert" : "info")} className="mt-0.5" />
    <div className="min-w-0 flex-1 text-text">{children}</div>
  </div>
);

export const PageHeader = ({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) => (
  <div className="flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 className="h1">{title}</h1>
      {subtitle ? <p className="mt-1 text-text-2">{subtitle}</p> : null}
    </div>
    {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
  </div>
);

// ---- toasts -------------------------------------------------------------------------

type Toast = { id: number; tone: Tone; text: string };
const ToastContext = createContext<(tone: Tone, text: string) => void>(() => undefined);
export const useToast = () => useContext(ToastContext);

export const ToastProvider = ({ children }: { children: ReactNode }) => {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const push = useCallback((tone: Tone, text: string) => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, tone, text }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), tone === "bad" ? 9000 : 4500);
  }, []);
  return (
    <ToastContext.Provider value={push}>
      {children}
      <div className="pointer-events-none fixed right-5 bottom-5 z-[60] flex w-96 flex-col gap-2" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={clsx("card animate-in pointer-events-auto flex gap-2.5 px-4 py-3 text-[13px]", "border-l-4", {
            "border-l-ok": t.tone === "ok",
            "border-l-bad": t.tone === "bad",
            "border-l-warn": t.tone === "warn",
            "border-l-accent": t.tone === "accent" || t.tone === "info" || t.tone === "muted",
          })}>
            <Icon name={t.tone === "ok" ? "check" : t.tone === "bad" ? "alert" : "info"} className={clsx("mt-0.5", t.tone === "ok" ? "text-ok" : t.tone === "bad" ? "text-bad" : "text-accent")} />
            <span>{t.text}</span>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
};

export const errorText = (err: unknown): string => (err instanceof Error ? err.message : typeof err === "string" ? err : "Something went wrong");

export const fmtDate = (iso?: string | null) => {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
};

export const fmtAgo = (iso?: string | null) => {
  if (!iso) return "never";
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return `${Math.round(s / 86400)} days ago`;
};

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
export const fileName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
export const fmtSize = (bytes: number) => (bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`);
