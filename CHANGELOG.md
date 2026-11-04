# Changelog

## BoilerSteamXL 1.0.1 (2026-11-04)

First release of the fork, based on BoilR `main` at `1b03ebf` (2.0 beta Tauri app).

### Added
* `crates/bsxl-engine`: discovery, exclusion rules, title resolver, review queue, duplicate
  detection, approval-gated import, Clean Rebuild, backups/restore, scheduling, portable
  mode, diagnostics. 64 tests on synthetic games folders and Steam fixtures.
* SteamGridDB key: reused automatically from an existing BoilR/BoilerSteamXL config,
  "Get my free key" opens the API page, and a copied or pasted key is checked and saved.
* Renaming a game searches its artwork again under the new name.
* Import with Steam open asks Steam to exit (never force-closed), imports, then starts it again.
* Clean Rebuild duplicate groups show each shortcut's target, start folder, options and app ID.
* One list and one button: My Games shows every game on this PC and every non-Steam shortcut
  in Steam; mark games to add, not add or remove, then "Sync with Steam" shows the exact changes
  and makes them (backup first, Steam closed and reopened). Removals are never done by the
  unattended scheduled sync.
* No duplicates: shortcuts that start the same program are one game whatever their names; a game
  in Steam twice, or maybe already there, opens a merge window (keep one, use mine, keep both).
* Navigation cut to My Games and Settings; the rest sits under "More tools".
* Artwork match picker shows cover thumbnails; the game window is smaller and folds rare options.
* Benchmark against BoilR's own folder scanner (`tools/compare`, [docs/BENCHMARK.md](docs/BENCHMARK.md)).
  Fixed what it found: mod tools such as Creation Kit are no longer picked as the game, the
  default scan depth is 8 folders (like BoilR), and dotted initials stay one word
  ("S.T.A.L.K.E.R.").
* Artwork matching prefers the game with the same name, Roman numerals included
  ("Baldur's Gate III" for "Baldur's Gate 3"), over SteamGridDB's most popular result.
* Folder-based title resolution for repack and machine-style folder names.
* Explainable executable scoring with "Uncertain" detection ([docs/SCORING.md](docs/SCORING.md)).
* Exclusion Rules screen: rule types, context-aware rules, allowlist, preview, import/export,
  reset to recommended.
* Review & Approve (three-panel), final import confirmation with before/after diffs.
* + Add Game wizard (browse, drag and drop, paste a target with arguments, analyse a folder,
  duplicate check). Manual games persist and go through review.
* Clean Rebuild wizard (managed or all non-Steam shortcuts) with typed confirmation,
  verified backups including artwork and collections, artwork migration and rollback.
* Steam Library, Discover Games, Automation, Backups & History (backups, run history,
  logs, diagnostics) and Settings screens; first-run wizard; dark and light themes.
* Simulation mode (on by default): every Steam write is planned and reported, not applied.
* Headless CLI `--scan`, `--sync`, `--dry-run`, `--status`; Windows Task Scheduler
  integration (daily, weekly, every N days, run missed); deduplicated notifications.
* Portable edition (`BoilerSteamXL.portable` marker) with read-only fallback.
* NSIS installer (per-user) that removes the scheduled task on uninstall.
* Windows CI workflow and `scripts/build-release.ps1`.

### Changed
* Renamed the app to BoilerSteamXL (`BoilerSteamXL.exe`, config folder `BoilerSteamXL`).
* BoilR's replace-all sync is no longer reachable from the app; writes are incremental and
  only for approved, confirmed games. BoilR-made shortcuts are recognised and left alone.
* BoilR's artwork tools respect Simulation mode.
* The config folder honours `BOILERSTEAMXL_HOME` (and `BOILR_CONFIG_HOME`), which BoilR's
  tests already assumed.
* `.cargo/config.toml`: removed the Linux-only MinGW cross-linker override that broke native
  Windows GNU builds.

### Fixed (pre-release adversarial review)
* Artwork icon updates now use the guarded write path (Simulation mode, Steam closed,
  backup, lock, atomic write) and BoilR's real grid file names.
* One OS-level operation lock covers every library change and Steam write; scans run
  without it and read Steam under it, so the app and the scheduled scan never lose each
  other's changes; `--sync` no longer locks itself out.
* Lossless `shortcuts.vdf` handling: unknown keys are preserved, restores are byte-exact,
  damaged files can be restored, failed read-backs roll back.
* Plan and restore tokens include Simulation mode; updates can't create duplicates; moved
  games keep their shortcut identity; user launch options survive rescans.
* Scanner: tool/mod sub-folders, publisher folders with launchers, roots that are games,
  nested roots, `Windows` build folders, GOG DLC manifests, title numbers ("Tron 2.0").

### Removed
* BoilR's Tauri screens Games/Shortcuts/Settings and the pipe bar (replaced), and BoilR's
  standalone scanner from the app (replaced by the engine's scanner).
