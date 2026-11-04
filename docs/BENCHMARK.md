# Benchmark: finding games in a folder, BoilR vs BoilerSteamXL

BoilerSteamXL is a fork of [BoilR](https://github.com/PhilipK/BoilR). This page compares one
thing: **how well each one finds games in a folder of loose installs** (the kind of folder
FitGirl, DODI and other repacks leave behind). Both tools ran on the same synthetic folder,
and each result was checked against the right answer for every game.

It is not a comparison of the two apps overall. BoilR's main job is importing games from
launchers (Epic, GOG, itch, Xbox, Heroic, Lutris and more). Its folder scanner ("Standalone")
is one feature among many, and launchers are not compared here.

## Results

44 games, 4 folders that are not games, 119 `.exe` files. Run on 2026-11-04, Windows 11,
release build, 5 rounds.

| Metric | BoilR (Standalone scanner) | BoilerSteamXL |
|---|---|---|
| Games found correctly (one entry, right exe) | 21 / 44 | **44 / 44** |
| Missed games | 23 | **0** |
| Wrong exe chosen | 0 | 0 |
| Split games (listed more than once) | 0 | 0 |
| Junk entries (installers, redists, tools, ...) | 1 | **0** |
| Correct display names | 21 / 44 | **44 / 44** |
| Entries it would add to Steam | 22 | 44 |
| Flagged for review as uncertain | n/a | 12 |
| Same result on every scan | yes | yes |
| Median scan time (5 runs) | **13.0 ms** | 34.9 ms |
| Median scan time, PE cache warm (like the app's later scans) | n/a (no cache) | 23.9 ms |

**Read this with the first run in mind.** The first run of this benchmark found three mistakes
in BoilerSteamXL, and they were fixed before the run above:

| First run | Fix |
|---|---|
| 42 / 44 found. **Skyrim**: picked `CreationKit.exe` (the mod editor) over `TESV.exe` on a tie | Mod tools (`CreationKit`, `*toolkit*`, mod managers) now get the editor penalty |
| **Bulletstorm** missed: 7 folders deep, BoilerSteamXL stopped at 6 | Default depth is now 8, like BoilR |
| `S.T.A.L.K.E.R. Anomaly` named `S T A L K E R Anomaly` | Dotted initials stay one word (`S.T.A.L.K.E.R.`, `F.E.A.R.`) |

Each fix is covered by a test, but they were made after seeing these cases, and the test folder
was written by the BoilerSteamXL side. Expect a real collection to show a few misses the corpus
does not have. The first-run numbers were 42 / 44 found, 1 wrong exe, 41 / 44 names.

What the numbers mean:

- **BoilR misses about half the games, but whatever it does list is right** (except one
  utility). Its rule is strict: an `.exe` counts only if its name matches a folder above it,
  letters and digits only (`Hadesd\Hades.exe`, `DARK SOULS III\Game\DarkSoulsIII.exe`).
  That rule never picked a wrong exe or split a game in this test. It misses every game whose
  exe is named differently (`hl2.exe`, `bio4.exe`, `GTA5.exe`, `TESV.exe`, `Game.exe`) and every
  folder that carries a repack name (`Hollow.Knight.v1.5.78-FitGirl.Repack`).
- **BoilerSteamXL scores every exe in a game folder and keeps the best one**, so it does not
  depend on exe names, and repack and scene tags come off the names (`Hollow Knight`,
  `The Witcher 3: Wild Hunt`, `Call of Duty: World at War`). It never adds anything to Steam on
  its own: every game waits for your OK, and the 12 it is unsure about are marked for a look.
- **One result is decided by an alphabetical tie-break.** BoilerSteamXL scored
  `Sniper Elite V2in\SniperEliteV2.exe` and the copy in `NoDVD\` the same (83) and took `bin`
  because it sorts first. It is flagged uncertain. BoilR also chose the `bin` copy alphabetically.
- **BoilR is faster, by about 2.5 to 3 times, on every run.** BoilerSteamXL does more work: it
  reads PE headers and checks for Unity `_Data` folders, GOG manifests and `steam_appid.txt` next
  to each candidate. Both scan 44 games in well under a tenth of a second. Other work was running
  on the machine, so the timings vary between runs; the ranking never changed.

### Where each one was wrong

| | BoilR | BoilerSteamXL |
|---|---|---|
| Missed | 23 games, all for the same reason: the exe or the folder name doesn't match (list below) | none |
| Wrong exe | none | none (Skyrim in the first run, see above) |
| Junk | `DS4Windows` (a controller utility in `Tools\DS4Windows\DS4Windows.exe`) is listed as a game because the exe matches its folder name | none |
| Wrong name | none | none (`S T A L K E R` in the first run) |

## The corpus

The tool builds the folder from scratch on each run. Each "executable" is a valid 64-bit PE
header with no sections and no code, padded with zeros to a realistic size. Windows will not
run them. BoilerSteamXL reads the PE header (for the subsystem and the version resource), so
the files had to be real PE files. BoilR looks only at file names. The files have **no version
resource**, so BoilerSteamXL's "title from the exe's product name" path is never used here.
That keeps both tools working from folder and file names only.

Layout: some games sit directly in `Games\`, some in a `PLUG&PLAY` collection folder, and some
under `Repacks\...` up to 7 levels deep. Every game, its traps and the result:

| Game folder | Traps | BoilR | BoilerSteamXL |
|---|---|---|---|
| `Hollow.Knight.v1.5.78-FitGirl.Repack` | FitGirl folder name, Unity, crash handler, uninstaller, MD5/QuickSFV | missed | found (hollow_knight.exe) |
| `the-witcher-3-wild-hunt` | machine-style name, exe in bin\x64, _CommonRedist | missed | found (witcher3.exe) |
| `call.of.duty.world.at.war` | dotted name, exe is initials, multiplayer exe, Redist + DirectX | missed | found (codwaw.exe) |
| `PLUG&PLAY/A Way Out` | collection folder, Unreal layout, UE prerequisites, CEF helper, crash reporter | found (awayout.exe) | found (awayout.exe) |
| `PLUG&PLAY/Hades` | same exe in x64 and x86 | found (hades.exe) | found (hades.exe) · flagged uncertain |
| `PLUG&PLAY/Celeste` | plain (control) | found (celeste.exe) | found (celeste.exe) |
| `PLUG&PLAY/Stardew Valley` | small exe, mod loader next to it | found (stardew valley.exe) | found (stardew valley.exe) |
| `PLUG&PLAY/Cuphead` | Unity (control) | found (cuphead.exe) | found (cuphead.exe) |
| `PLUG&PLAY/DOOM Eternal` | exe name has build suffix (x64vk) | missed | found (doometernalx64vk.exe) |
| `PLUG&PLAY/Grand Theft Auto V` | abbreviated exe, three launch exes, bundled installer | missed | found (gta5.exe) · flagged uncertain |
| `PLUG&PLAY/LISA The Painful` | generic Game.exe (RPG Maker) | missed | found (game.exe) · flagged uncertain |
| `PLUG&PLAY/Doki Doki Literature Club` | Ren'Py: initials exe, bundled python/zsync | missed | found (ddlc.exe) |
| `PLUG&PLAY/Undertale` | plain (control) | found (undertale.exe) | found (undertale.exe) |
| `Terraria` | dedicated server next to the game | found (terraria.exe) | found (terraria.exe) |
| `Portal 2` | Source SDK map tool in bin | found (portal2.exe) | found (portal2.exe) |
| `Half-Life 2` | exe is initials (hl2) | missed | found (hl2.exe) |
| `Left 4 Dead 2` | dedicated server next to the game | found (left4dead2.exe) | found (left4dead2.exe) |
| `Resident Evil 4` | unrelated exe name (bio4) | missed | found (bio4.exe) · flagged uncertain |
| `Batman Arkham City GOTY` | abbreviated exe, config launcher, PhysX installer | missed | found (batmanac.exe) · flagged uncertain |
| `Need for Speed Most Wanted 2005` | unrelated exe name (speed) | missed | found (speed.exe) |
| `DARK SOULS III` | exe in Game sub-folder | found (darksoulsiii.exe) | found (darksoulsiii.exe) |
| `Repacks/2023/Action/Sekiro.Shadows.Die.Twice.MULTi13-ElAmigos` | scene name, 4 levels deep, _Redist | missed | found (sekiro.exe) |
| `Repacks/Cyberpunk 2077 (v2.1 + DLC) [DODI Repack]` | DODI name with brackets, crash reporter, uninstaller | missed | found (cyberpunk2077.exe) |
| `Dishonored 2` | Language Selector.exe, _Redist | found (dishonored2.exe) | found (dishonored2.exe) |
| `PLUG&PLAY/Stray` | Unreal with unrelated project folder (Hk_project) | found (stray.exe) | found (stray.exe) |
| `Hellblade Senuas Sacrifice` | Unreal, exe/project name differs from folder | missed | found (hellbladegame.exe) · flagged uncertain |
| `PLUG&PLAY/Ori and the Will of the Wisps` | Unity, abbreviated exe | missed | found (oriwotw.exe) · flagged uncertain |
| `PLUG&PLAY/INSIDE` | Unity (control) | found (inside.exe) | found (inside.exe) |
| `Mount & Blade Warband` | abbreviated exe, dedicated server | missed | found (mb_warband.exe) |
| `Euro Truck Simulator 2` | exe in bin\win_x64 and bin\win_x86, different name | missed | found (eurotrucks2.exe) · flagged uncertain |
| `Grim Dawn` | 32- and 64-bit exe, modding tool | found (grim dawn.exe) | found (grim dawn.exe) |
| `Max Payne 3` | Crack sub-folder with copies of the exes | found (maxpayne3.exe) | found (maxpayne3.exe) |
| `Sniper Elite V2` | NoDVD sub-folder with a copy | found (sniperelitev2.exe) | found (sniperelitev2.exe) · flagged uncertain |
| `Far Cry 3` | Fix sub-folder with loader, updater, DX9/DX11 exes | found (farcry3.exe) | found (farcry3.exe) · flagged uncertain |
| `Fallout New Vegas` | game launcher next to the game | missed | found (falloutnv.exe) |
| `The Elder Scrolls V Skyrim` | initials exe, launcher, editor | missed | found (tesv.exe) · flagged uncertain |
| `Dead Rising 2` | settings tool named after the game | found (deadrising2.exe) | found (deadrising2.exe) |
| `Retro Rally` | generic start.exe, Settings.exe | missed | found (start.exe) · flagged uncertain |
| `S.T.A.L.K.E.R. Anomaly` | the right exe is a launcher | missed | found (anomalydx11.exe) |
| `Ōkami HD` | Unicode folder name | missed | found (okami.exe) |
| `東方紅魔郷` | Japanese folder and exe name, config tool | found (東方紅魔郷.exe) | found (東方紅魔郷.exe) |
| `Hotline Miami` | spare [old] copy | found (hotlinemiami.exe) | found (hotlinemiami.exe) |
| `Repacks/2019/Archive/Old/Backup/Metro 2033 Redux` | 6 folders deep, short exe name | missed | found (metro.exe) |
| `Repacks/2019/Archive/Old/Backup/Shooters/Bulletstorm` | 7 folders deep | found (bulletstorm.exe) | found (bulletstorm.exe) |
| `New folder` (not a game) | empty folder | not listed | not listed |
| `Elden.Ring.v1.10-FitGirl.Repack` (not a game) | repack not installed yet: only setup.exe | not listed | not listed |
| `_CommonRedist` (not a game) | stray redistributables at the top | not listed | not listed |
| `Tools/DS4Windows` (not a game) | controller utility kept with the games | listed (junk) | not listed |

This table is the tool's own output. The full file list for every folder, with sizes and
labels, is the `CORPUS` table in [`tools/compare/src/main.rs`](../tools/compare/src/main.rs).

## Method

- **BoilR**: its own code, unchanged (`src/platforms/standalone` is identical to upstream
  commit `1b03ebf`). The tool loads it the way BoilR does: a `config.toml` with only a
  `[standalone]` section (`enabled = true`, one scan folder, no manual picks), in a scratch
  config folder set through `BOILERSTEAMXL_HOME`. It then calls
  `get_platforms()` → `standalone.get_shortcut_info()`. The result is the list of shortcuts
  BoilR would write.
- **BoilerSteamXL**: `bsxl_engine::scan::scan` with the recommended exclusion rules
  (`RuleSet::recommended()`) and the default depth (8), as on a first run. The entry for each
  game is its first (best) candidate, which is the executable the library would import once
  you approve the game. Titles come from `title::title_from_folder`, as in the library.
- **Ground truth**: every `.exe` in the corpus has a label. *Main* is the exe a shortcut should
  start. *Alt* also starts the game properly (Unreal shipping binary, x86 build, official
  launcher). *Spare* is a crack/NoDVD copy or an `[old]` copy. *Junk* is an installer,
  redistributable, uninstaller, crash handler, tool, server, verifier or settings tool.
  - *Found correctly*: exactly one non-junk entry for the game, and it is Main or Alt.
  - *Wrong exe*: the game's only entry is a Spare or a junk file from its folder.
  - *Split*: two or more non-junk entries for one game.
  - *Missed*: no entry.
  - *Junk entries*: entries pointing at a folder that is not a game, or extra junk entries
    for a game that also has a non-junk entry.
  - *Correct names*: counted for games found correctly. The title must equal an expected name
    ignoring case and extra spaces. Punctuation counts, so `S T A L K E R Anomaly` is not
    `S.T.A.L.K.E.R. Anomaly`. Some games accept more than one name (`Okami HD`, `LISA: The Painful`).
- **Runs**: the corpus is written once. Each tool runs once unmeasured to warm the file system
  cache, then N times in turn (default 5), and the median is reported. BoilerSteamXL runs
  twice per round: with an empty PE cache (BoilR has no cache) and with a warm one, as on the
  app's later scans. *Same result on every scan* compares all runs, cold and warm. The timed
  part is the scan and the conversion to (name, exe) pairs; writing the JSON report is outside it.

## Caveats

- **The corpus is synthetic and was written by the BoilerSteamXL side.** It is modelled on
  real repack and store layouts, but it was not drawn from a sample of real libraries. Many
  traps (repack names, exes named differently from their folder) are exactly the cases BoilR's
  name-matching rule does not try to handle. On a folder of cleanly named games whose exes match
  their folders, BoilR would score much closer.
- **BoilR has manual fixes that this test leaves out.** Its egui settings have "Find unmatched
  executables" to pick the exe for a game it missed, and renames to fix titles. BoilerSteamXL's
  review list (change the exe, rename, approve) is also manual and also left out. Only what
  each tool finds by itself is measured.
- The files carry no version resource, so BoilerSteamXL's product-name title and description
  checks did nothing here. On real exes they can help with or change a pick.
- Timings come from one busy Windows 11 machine, in a corpus of zero-filled files. Use them to
  rank the two tools, not as absolute numbers.
- Launchers (Epic, GOG, Xbox, ...) are not compared. BoilerSteamXL uses BoilR's own launcher code
  for those.

## Writing to Steam (from the code, nothing was run against Steam)

| Question | BoilR (upstream `1b03ebf`) | BoilerSteamXL |
|---|---|---|
| Steam already has a shortcut to the same exe under another name: is it added again? | **Yes.** A shortcut's app id is a CRC of exe + name (`steam_shortcuts_util` `app_id_generator.rs:17-22`, called from `Shortcut::new` in `src/platforms/standalone/game.rs:25-26`). Before writing, BoilR removes only its own tagged shortcuts and shortcuts with the same app id (`crates/boilr-core/src/sync/synchronization.rs:149-150`, `236-250`). A shortcut you made by hand under another name has a different app id and stays, so the game ends up in Steam twice. | **No.** `detect_duplicate` matches existing shortcuts by executable path, whatever their name (`crates/bsxl-engine/src/library.rs:1078-1087`). The game becomes *Already in Steam*, linked to that shortcut (`library.rs:745-750`), and cannot be approved (`library.rs:784-785`). A shortcut to another exe in the same game folder makes it a *possible duplicate*, which needs an explicit "add anyway" (`library.rs:1135-1140`, `786-787`). |
| Does it ask before writing to Steam? | **One click.** The games list has a checkbox for each game, ticked by default (`src/ui/ui_import_games.rs:78`, `111-122`). The Import button saves settings and starts the sync straight away, with no confirmation step (`src/ui/uiapp.rs:123-133` → `ui_import_games.rs:149-171`). `--no-ui` syncs with no UI at all (`src/main.rs:33-34`). | **Yes, twice.** Each game must be approved (`library.rs:772-803`). The import shows a plan and runs only with that plan's token (`crates/bsxl-engine/src/import.rs:509-518`). Simulation mode is on by default, and while it is on nothing is written (`prefs.rs:44`, `import.rs:543`). While Steam is running nothing is written; the import stops, or waits until Steam is closed (`import.rs:548-560`). |
| Backup before writing? | **Yes, best effort.** `backup_shortcuts` copies each `shortcuts.vdf` right before the sync (`ui_import_games.rs:170`; `src/backups.rs:60-88`). A failed copy is only logged (`backups.rs:81`) and the sync goes ahead. The file is then rewritten in place (`synchronization.rs:330-337`). | **Yes, verified.** `create_backup` reads the copy back and checks its manifest (`import.rs:1030`, `1075-1077`, `1104`). If the backup fails, nothing is written (`import.rs:564`). Writes go through a temporary file, and a failed write restores the backup (`import.rs:720-771`). |

## Reproduce

Windows, from the repository root. With the GNU toolchain, `CARGO_TARGET_DIR` must not contain
spaces.

```
cargo run --release -p compare -- --runs 5
```

- `--corpus <empty folder>` writes the corpus there and keeps it, so you can browse it. By
  default it goes in a temporary folder that is deleted afterwards (about 1.6 GB while it
  exists).
- `--json <file>` sets where the full results go: every entry, per-run times, and BoilerSteamXL's
  candidates with their scores and reasons. The default is `%TEMP%\bsxl-compare-results.json`.

The tool reads no Steam folder, no real games folder and no real config folder. It writes only
the corpus and a scratch BoilR config, both under the temporary folder.
