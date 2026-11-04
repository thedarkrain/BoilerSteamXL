# BoilerSteamXL progress

Status as of 2026-11-04. "Tested" means covered by an automated test that passes on this
machine (Windows 11, Rust 1.99 GNU toolchain) unless stated otherwise. **No test or manual
check ever wrote to a real Steam installation**: everything Steam-facing was verified
against synthetic fixtures, and the app ships with Simulation mode on.

## Phase 1: Repository audit and baseline ✅
- Cloned BoilR `main` (`1b03ebf`), read CLAUDE.md, manifests, core, Tauri app.
- Baseline: Tauri app built on Windows with the GNU toolchain after two environment fixes
  (removed the Linux-only cross-linker in `.cargo/config.toml`; target dir without spaces
  for `windres`).
- Finding: BoilR's sync removes all BoilR shortcuts and re-adds them, which is incompatible
  with per-game approval, so a new incremental write path was built.

## Phase 2: Smart game recognition ✅ (tested)
| Feature | State |
|---|---|
| Recursive scan, depth limit, multiple roots, enable/disable roots | ✅ tested |
| No symlink/junction following | ✅ tested (junction test runs when `mklink /J` is allowed) |
| Unicode and spaces in paths, case-insensitive matching | ✅ tested |
| Collection folders (`PLUG&PLAY`, `Games`), generic folders (`Binaries\Win64`, `Engine`) | ✅ tested |
| Folder-based titles (repack tags, casing, subtitles), title source shown | ✅ tested (`marvel-vs-capcom` → Marvel vs. Capcom, and the 7 examples from the brief) |
| Explainable scoring, Uncertain detection, multiple launchers → review | ✅ tested |
| PE metadata (description/product/console) read without executing, cached | ✅ implemented; fixtures use placeholder files, so PE parsing is exercised only on real executables |
| GOG manifest play task, Unity `_Data`, Unreal shipping/bootstrap | ✅ implemented (Unity tested) |
| Cancellable scans, progress events | ✅ tested (cancel) |
| Missing, relocated, offline-drive handling | ✅ tested |
| 400-game scan performance | ✅ tested (< 30 s budget; well under on this machine) |
| Exclusion engine: folder name/sequence, path, file name, glob, regex, category, allowlist, inside-game-only rules | ✅ tested |
| Rules screen: add/edit/toggle/delete, preview matches, import/export, reset | ✅ UI verified in browser preview |

## Phase 3: Import management ✅ (tested, release-blocking set)
1. Manual add works ✅ 2. Manual entries persist after restart ✅ 3. Manual executable
overrides preserved ✅ 4. Repeated scans add no queue duplicates ✅ 5. Repeated imports add
no Steam duplicates ✅ 6. Scans never import ✅ 7. Rejected stays rejected ✅ 8. Ignored
stays excluded ✅ 9. Approval invalidated on executable/argument change ✅ 10. Steam writes
need explicit approval ✅ 11. Existing Steam entries preserved ✅ 12. Final preview matches
what is written (field-by-field) ✅. Also: plan token rejects changes after preview ✅,
Steam running blocks/defers ✅, damaged `shortcuts.vdf` never overwritten ✅, account
selection respected ✅, failed import restores already-written accounts ✅, Simulation mode
writes nothing ✅, backups verified and restorable ✅, official Steam games recognised ✅.

## Phase 4: Steam integration ✅ (tested on fixtures)
Steam path detection (config → registry → Program Files) and validation that rejects a
library folder; multiple accounts with persona names; official installs from
`libraryfolders.vdf`/app manifests; verified timestamped backups; atomic writes with
read-back verification; rollback; graceful `steam -shutdown` (never kills Steam).

## Phase 5: Clean Rebuild ✅ (tested on fixtures, UI verified in browser preview)
Enumeration (managed/BoilR/other, broken, utilities, duplicate groups, artwork) ✅,
official games untouched ✅, retained shortcuts unchanged ✅, only approved replacements ✅,
approvals from before the rebuild don't count ✅, game files never deleted ✅, backups with
artwork and collections ✅, idempotent ✅, artwork migrated to new IDs with originals kept ✅,
account selection respected ✅, external change after preview stops with a valid file ✅,
rollback ✅, phrase + acknowledgement + Steam-closed required ✅, managed mode scope ✅,
simulation ✅. Five-stage wizard with saved draft decisions (resumes after navigation).

## Phase 6: UI ✅ (browser preview walkthrough automated, no console errors)
Dashboard, My Games (grid/table, search, filters, sort, details, real Steam covers for
imported games), Review & Approve (three panels, bulk actions limited to clear-cut games),
Import confirmation, + Add Game wizard, Discover Games, Steam Library, Clean Rebuild,
Artwork Manager (BoilR's, restyled), Exclusion Rules, Automation, Backups & History (with
logs and diagnostics), Settings, first-run wizard, dark/light themes. A CDP script clicked
through review → import dialog → add game → full Clean Rebuild with 9 checks passing.

## Phase 7: Automation ✅
Task Scheduler XML (daily, weekly, every N days, run missed, one instance) ✅ tested;
next-run computation ✅ tested; registration via `schtasks` (implemented, **not executed
on this machine** to avoid creating tasks during development); refresh on startup ✅;
CLI `--scan/--sync/--dry-run/--status` ✅ implemented; deduplicated notifications ✅
(dedup tested via `newly_discovered`; the toast itself was not shown during development).

## Artwork for every game ✅ (added after first user test)
- In the app: Steam's artwork for imported/linked shortcuts, else covers/heroes/logos/icons
  downloaded from SteamGridDB into `BoilerSteamXL-datartwork` (background, one game at a
  time, progress in the header), else Steam's library cover for official games, else the
  game's own .exe icon (read from the file, never run). "Change…" picks another SteamGridDB
  match per game; "Get missing artwork" fetches the rest.
- On import (and Clean Rebuild): the cached images are copied into Steam's grid folder for
  the new shortcut and its icon points at them; existing Steam artwork is never replaced.
  Same safeguards as every write (Simulation mode, backup, Steam closed).
- Tested: copy-on-import, no copy in Simulation mode, existing art kept, real .exe icon
  extraction (notepad.exe). Not tested here: live SteamGridDB downloads (needs your free
  API key) and rendering of cached files inside the installed app.

## Adversarial review (multi-agent) ✅
Two review rounds were run over the finished code: 5 independent reviewers (write path, state
machine, scanner, backend/CLI, frontend contract), each finding checked by 2 skeptics, then a
verification pass over every fix plus fresh reviewers on the changed write path and locking.
- Round 1: 40 findings raised, 38 confirmed, all fixed. Most important: artwork icon updates
  bypassed the write safeguards (now through the engine); `--sync` took the lock twice;
  scheduled scans could overwrite UI decisions (now one OS lock around every load-change-save,
  scans run lock-free); writes dropped unknown `shortcuts.vdf` keys such as `sortas` (new
  lossless `vdf.rs` layer; restores are byte-exact); tokens did not cover Simulation mode.
- Round 2: 31 follow-ups; all high/medium and most low fixed (stale Steam snapshot during a
  merge, real BoilR icon file names, restoring a damaged file, recorded identity for moved
  games and multi-account rebuilds, duplicate tie-breaks, tool sub-folders, Unreal roots,
  title edge cases). Each fix has a regression test in `tests/review_fixes.rs`.

## Phase 8: Packaging ✅
- `dist-release/BoilerSteamXL-Setup-Win64.exe` (NSIS, per-user, 5.5 MB) and
  `dist-release/BoilerSteamXL-Portable-Win64.zip` (7.2 MB) built on this machine with
  `scripts/build-release.ps1`; checksums in `SHA256SUMS.txt`.
- Portable build verified with fixtures: `--version`, `--scan` (twice, idempotent),
  `--dry-run`, `--status`, `--sync` (correctly kept a confirmed game queued because Steam
  was running), GUI window opens, Common Controls v6 active, data stays in
  `BoilerSteamXL-data`, a corrupt preferences file is set aside and defaults are used.
- The exe needs `WebView2Loader.dll` next to it on the GNU toolchain (included in both
  packages); MSVC builds link it statically.
- The installer was built and its contents listed; it was **not installed** on this machine
  (to avoid changing the system); install/uninstall and the uninstall hook are untested.

## Test totals (all passing)
bsxl-engine: 33 unit + 12 discovery + 16 import safety + 9 Clean Rebuild + 27 review
regressions = 97. App: 6 (+1 network test ignored). Upstream BoilR: boilr-core 17,
boilr library 38. Browser walkthrough: 9 flow checks, no console errors.

## Known limitations / outstanding
- Never run against your real Steam during development (by design). The first real write
  happens only when you switch off Simulation mode.
- `schtasks` registration and Windows toast notifications were implemented but not executed
  here (no tasks created on this PC); the task XML and next-run logic are unit-tested.
- Overlapping scan roots can still count the same executables twice in the scan statistics.
- A launcher game's custom launch options can't be reset to the launcher's own with "Reset".
- Binaries are **not code-signed**.
- Per-launcher detail settings (custom Epic/GOG paths) from BoilR are not in the new UI;
  only enable/disable per launcher. They still work from `config.toml`.
- SteamGridDB matching inside "+ Add Game" is not offered; choose artwork in the Artwork
  Manager after import.
- The update checker only reports a newer release from a configured feed URL; there is no
  auto-update.
- Light theme exists but was designed dark-first.
- BoilR's upstream CI jobs (Flatpak lockfile sync, Linux) were not updated for the fork;
  the BoilerSteamXL workflow is `.github/workflows/boilersteamxl-windows.yml`.
- High-DPI: the UI uses CSS pixels and WebView2 scales it; checked at 100 % only.

## One list + Sync with Steam, merge, peer review ✅ (2026-11-04)

- My Games is the single screen: games on this PC and every non-Steam shortcut in Steam, grouped by
  program path (same path = same game, whatever the name). One action per tile; "Sync with Steam (N)"
  shows the exact plan (add / change / remove) and applies it with backup, Steam closed and restarted.
- Removals: `Library.removals`, `OpKind::Remove` in `plan_import` (interactive syncs only, never `--sync`).
- Merge window for games in Steam twice or maybe already there (keep one / use mine / keep both).
- Two peer reviews (engine + UI) with adversarial verification: 44 findings, 42 confirmed. 39 handed to the fixer: 37 applied in full, 1 in part, 1 skipped.
- Tests: engine 107, app 6, upstream 17 + 38; UI walkthrough 13 checks, no console errors.
