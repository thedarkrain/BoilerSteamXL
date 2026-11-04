# BoilerSteamXL architecture

BoilerSteamXL is a fork of [BoilR](https://github.com/PhilipK/BoilR). It keeps BoilR's Rust
core (Steam file formats, SteamGridDB, launcher integrations) and its Tauri 2 + React app,
and replaces BoilR's "sync everything" model with **discover → review → approve → confirm →
import**.

```
crates/boilr-core      BoilR: settings, Steam file formats, SteamGridDB, (BoilR's sync)
src/                   BoilR: launcher integrations (Epic, GOG, itch, Xbox, …), egui app
crates/bsxl-engine     NEW: everything BoilerSteamXL adds (no UI, fully unit-tested)
apps/boilr-tauri       The BoilerSteamXL desktop app (Tauri 2 + React + Tailwind 4)
```

## bsxl-engine

| Module | Responsibility |
|---|---|
| `rules.rs` | Exclusion rules (folder name/sequence, full path, file name with wildcards, glob, regex, category), allowlist, recommended defaults, context-aware *inside games only* rules |
| `title.rs` | Folder name → game title (repack tags, casing, subtitles), container and generic names |
| `scan.rs` | Recursive walk (no symlinks/junctions, depth limit, cancellable, progress), game-folder grouping, executable scoring with reasons, PE metadata cache (see `docs/SCORING.md`) |
| `library.rs` | The review queue (`library.json`): entries, the status state machine, merging scans without undoing user decisions, relocation, missing games, manual games, duplicate detection |
| `steam.rs` | Read-only Steam snapshot: installation detection/validation, accounts (with persona names), shortcuts (managed/BoilR/other, broken), official games and their install folders, Steam-running check, graceful `steam -shutdown` |
| `import.rs` | The single write path: operation plans, verified backups, approval-gated import, Clean Rebuild, restore, artwork migration |
| `schedule.rs` | Task Scheduler XML, next-run computation, `schtasks` registration |
| `prefs.rs` | `preferences.json` with schema version and migrations (Simulation mode defaults to on) |
| `portable.rs` | Portable-edition data folder resolution with read-only fallback |
| `diagnostics.rs`, `log.rs` | Masked diagnostic report, JSON-lines log with rotation |

### Statuses

```
NeedsReview ──approve──▶ Approved ──final confirm──▶ Imported
    │  ▲                    │   │                      │
    │  └──exe/args change───┘   └─Steam running────▶ ReadyToImport ──(--sync, Steam closed)──▶ Imported
    ├─▶ Rejected / Ignored (kept on every scan)
    ├─▶ PossibleDuplicate ──"add separately anyway"──▶ Approved
    ├─▶ AlreadyInSteam (detected or linked by the user)
    └─▶ Missing (folder gone; previous status restored when it returns)
Imported ──exe changed──▶ UpdateAvailable ──approve──▶ Approved ──confirm──▶ Imported (updated in place)
```

### The write path (`import.rs`)

Every write to Steam (import, Clean Rebuild, restore) does, in this order:

1. Rebuild the plan from current state and compare its **token** with the one the user
   confirmed. The token covers every operation and the exact bytes of each affected
   `shortcuts.vdf`, so any change by Steam (or anything else) since the preview stops it.
2. Stop if **Simulation mode** is on (the run is recorded as simulated).
3. Stop if Steam is running (or defer, for imports). Steam is never killed.
4. The caller holds the OS file lock (`operation.lock`); the write functions take the
   `OpLock` as a parameter, so they cannot be called without it (see *Concurrency*).
5. Create a timestamped backup (`Backups/<date>_<time>-<kind>/`): `shortcuts.vdf` per account,
   for rebuilds also the account's non-Steam artwork and collections file, plus
   `library.json`, the plan and a manifest. Every copy is read back and compared; the backup
   `shortcuts.vdf` must parse. Any failure aborts before anything is written.
6. Per account: re-check the file fingerprint, read it with two parsers (the lossless
   `vdf.rs` layer and `steam_shortcuts_util`; a file either rejects is damaged and never
   written), build the new record list (kept records byte-for-byte including keys
   BoilerSteamXL does not know, such as `sortas`; updates replace only name, target, start
   folder and launch options; replacements take the old record's place carrying Steam's
   per-game state; new games appended), validate the bytes with both parsers, write to a
   temporary file, atomically rename it over `shortcuts.vdf`, read it back byte-for-byte.
7. On any failure, every account already written (including one that failed after its
   rename) is restored from the backup. Restores copy backup bytes verbatim and are
   all-or-nothing across accounts.

Shortcut icon updates for the Artwork Manager (`point_icons_at_grid`) use the same path.

### Concurrency

The app and the scheduled `--scan` can run at the same time. Scanning (`discover`) needs
no lock; merging the result into the library (`merge`), every UI change, and every write
loads `library.json`, changes it and saves it while holding `operation.lock`, so neither
process overwrites decisions the other just saved. UI commands wait up to 15 s for the
lock and run off the UI thread.

### Data files

`%APPDATA%\BoilerSteamXL\` (portable: `BoilerSteamXL-data\` beside the exe):
`preferences.json`, `library.json` (+ `.bak`), `scan-cache.json`, `Backups\`, `logs\`,
`config.toml` (BoilR's SteamGridDB/launcher settings). JSON files are written atomically
(temporary file + rename, previous version kept as `.bak`; unreadable files are set aside,
never deleted). SQLite was not needed: the queue is small, and JSON avoids a native
dependency and stays human-readable.

## Desktop app (`apps/boilr-tauri`)

* `src/main.rs`: the Tauri builder, plus BoilR's still-safe commands (SteamGridDB settings,
  launcher settings, artwork). BoilR's `run_full_sync`/`plan_sync` (bulk import without
  approval) are **not exposed**; a test checks this.
* `src/commands.rs`: BoilerSteamXL commands, thin wrappers over `bsxl-engine`.
* `src/cli.rs`: `--scan`, `--sync`, `--dry-run`, `--status` for Task Scheduler and terminals.
* `src/artwork.rs`: BoilR's artwork commands, now respecting Simulation mode.
* Frontend: `src/App.tsx` (shell, navigation, refresh), `src/views/*` (one file per
  screen), `src/ui.tsx` (design-system components), `src/api.ts` (typed command wrappers),
  `src/devMock.ts` (sample-data backend for browser development).

## Decisions and deviations

* **Windows toolchain**: builds with the `x86_64-pc-windows-gnu` Rust target and MinGW-w64,
  which need no administrator rights; MSVC works too. CI builds on `windows-latest`.
* **Approval-gated incremental writes** instead of BoilR's remove-all-and-re-add sync; BoilR
  shortcuts are detected and left alone unless a Clean Rebuild says otherwise.
* **One binary** for UI and CLI, so the scheduled task and the app share the same engine.
* **Launcher integrations** are optional sources for the same review queue.
