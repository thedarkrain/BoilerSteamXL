<p align="center"><img src="apps/boilr-tauri/icons/128x128@2x.png" width="96" alt=""></p>

# BoilerSteamXL

**Discover automatically. Add manually. Validate everything. Import safely. Refresh whenever you want.**

BoilerSteamXL turns a folder full of standalone games (loose installs, repacks, nested
collection folders) into a clean, duplicate-free Steam library. It finds the real game
executables, names games from their folders, filters out installers, redistributables and
utilities, and adds **only the games you approve** to Steam, with a verified backup before
every change.

It is a fork of [BoilR](https://github.com/PhilipK/BoilR) and keeps BoilR's Steam and
SteamGridDB code and launcher integrations. Not affiliated with Valve or the BoilR project.

**[Download the latest release](https://github.com/thedarkrain/BoilerSteamXL/releases/latest)**: installer or portable zip for Windows 10/11 x64.

![My Games: every game on this PC and in Steam, one Sync button](docs/screenshots/my-games.png)

| Merge a game that is in Steam twice | Sync with Steam: the exact changes |
| --- | --- |
| ![Merge](docs/screenshots/merge.png) | ![Sync](docs/screenshots/sync.png) |

## Highlights

* **Smart discovery**: recursive scanning with depth limits, no symlink/junction loops,
  Unicode paths, collection folders like `D:\Games\PLUG&PLAY\…`, Unreal/Unity layouts,
  cached PE metadata, relocation and missing-game detection. Nothing is ever executed.
* **Folder-based names**: `marvel-vs-capcom\game.exe` becomes *Marvel vs. Capcom*;
  `Hollow.Knight.v1.5.78-FitGirl.Repack` becomes *Hollow Knight*.
* **Explainable executable choice**: every candidate has a score and the reasons behind
  it ([docs/SCORING.md](docs/SCORING.md)); close calls go to review.
* **Exclusion rules**: folder names, paths, file names, globs, regex and categories, with
  context-aware rules, an allowlist, previews, import/export and recommended defaults
  (`_Redist`, `_CommonRedist`, `QuickSFV.exe`, `unins*.exe`, `Language Selector.exe`, …).
* **One list, one button**: My Games shows every game on this PC and every non-Steam
  shortcut already in Steam. Add, don't add, rename or remove what you like, then
  **Sync with Steam** shows the exact changes and makes them (Steam is closed normally and
  started again). Nothing reaches Steam before you confirm.
* **Renames reach Steam**: type the real name of a game and the next sync renames its Steam
  shortcut, keeping its Steam ID, artwork and play time.
* **Artwork for every game**: covers, heroes, logos and icons from SteamGridDB (free key,
  picked up automatically from BoilR if you have one), or each game's own icon.
* **+ Add Game**: browse, drag and drop, paste a path or shortcut target, or analyse a
  folder. Manual games go through the same review and survive every refresh.
* **No duplicates**: shortcuts that start the same program are the same game, whatever their
  names. A game already in Steam is never added again; one that is in Steam twice opens a
  merge window that shows each copy's path. Rescans keep your decisions and only show what
  is new.
* **Clean Rebuild**: a five-step wizard to rebuild BoilerSteamXL's shortcuts, or every
  non-Steam shortcut of an account, with side-by-side comparison, per-shortcut decisions,
  duplicate groups, artwork migration, a typed confirmation and one-click rollback.
* **Automation**: refresh on startup, daily, weekly or every N days through Windows Task
  Scheduler, headless, with deduplicated notifications. Refresh never imports.
* **Safety**: Simulation mode on by default, verified backups, atomic writes, damaged
  files never overwritten, Steam never force-closed, game files never touched.

## Download and install

* `BoilerSteamXL-Setup-Win64.exe`: per-user installer (Start menu shortcut, optional desktop
  shortcut, uninstaller that removes the scheduled task and optionally your data).
* `BoilerSteamXL-Portable-Win64.zip`: extract and run; data stays next to the exe.

Requires Windows 10/11 x64 with the WebView2 Runtime (included in Windows 11). The binaries
are **not code-signed**; SmartScreen may ask you to confirm the first start.

## First steps

1. The first-run wizard detects Steam (the *installation* folder, e.g.
   `C:\Program Files (x86)\Steam`, not `D:\SteamLibrary` and not `D:\Games`) and asks
   for your game folders (e.g. `D:\Games`).
2. **Scan for games** fills **My Games**.
3. Check the list: fix names, merge duplicates, mark games you don't want.
4. Press **Sync with Steam**, review the exact changes, and confirm.
5. BoilerSteamXL starts in **Simulation mode**: it shows exactly what would happen
   without touching Steam. Switch it off in Settings → Safety when you're ready.

## Command line

```
BoilerSteamXL.exe --scan [--scheduled]  discover games, update the review queue (never imports)
BoilerSteamXL.exe --sync                --scan, then import games already confirmed that waited for Steam to close
BoilerSteamXL.exe --dry-run             show what importing the approved games would do
BoilerSteamXL.exe --status              review queue, last scan, schedule
```

## Build from source

Requirements: Rust stable, Node.js 20.19+ or 22.12+. On Windows either the MSVC toolchain
(Visual Studio Build Tools) or the GNU toolchain with MinGW-w64 on `PATH`. With MinGW, put
the Cargo target directory on a path without spaces (`set CARGO_TARGET_DIR=D:\bsxl-target`):
`windres` cannot handle spaces.

```powershell
cargo test -p bsxl-engine            # engine tests (synthetic fixtures only)
cd apps\boilr-tauri
npm ci
npm run dev                          # browser preview with sample data (no Rust needed)
npx tauri build                      # release exe + NSIS installer
cd ..\..
powershell -File scripts\build-release.ps1   # tests + installer + portable zip + checksums
```

CI: `.github/workflows/boilersteamxl-windows.yml` builds and tests on `windows-latest` and
uploads both packages.

## Documentation

* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it is built, the write path, data files
* [docs/SCORING.md](docs/SCORING.md): executable scoring, titles, duplicate evidence
* [PROGRESS.md](PROGRESS.md): what is done, tested and outstanding
* [CHANGELOG.md](CHANGELOG.md)
* [docs/BoilR-Readme.md](docs/BoilR-Readme.md): the original BoilR README

## License

MIT OR Apache-2.0, like BoilR. See [NOTICE.md](NOTICE.md).
