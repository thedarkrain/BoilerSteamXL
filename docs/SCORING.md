# How BoilerSteamXL picks a game's executable and name

Everything here is implemented in `crates/bsxl-engine/src/scan.rs`, `title.rs` and
`rules.rs`. Every point a candidate gets is listed in the app (the ⓘ next to each
executable), so nothing below is hidden.

## 1. Walking the folders

* Each enabled game folder ("scan root") is walked up to its depth limit (default 6).
* Symbolic links and directory junctions are never followed (no loops, no escaping the root).
* Hidden folders, `steamapps`, `compatdata`, `$RECYCLE.BIN`, `System Volume Information`
  and `Windows` are skipped.
* Only `.exe` files are considered. **Nothing is ever executed**; at most the PE header and
  version resource are read (memory-mapped, read-only) and cached by path + size + mtime.

## 2. Exclusions (before scoring)

Rules from **Exclusion Rules** run first. Rules that need no context (for example `_Redist`,
`QuickSFV.exe`, `unins*.exe`, the *language selector* category) run on every executable.
Rules marked *inside games only* (`Tools`, `Support`, `Redist`) run after the game folder is
known and only match folders **strictly inside** it, so a game called "Tools" is still found.
Allowlisted paths are never excluded. Excluded executables stay visible in the review screens.

## 3. Finding the game folder

Executables that survive are arranged in a folder tree below the root:

| Folder looks like | Treated as |
|---|---|
| has executables of its own | a **game** (its whole subtree belongs to it) |
| has a generic sub-folder with executables (`bin`, `Binaries`, `Win64`, `x64`, `Engine`, `Game`, `Content`, …) | a **game** |
| a known collection name (`PLUG&PLAY`, `Games`, `Repacks`, `FitGirl`, `Installed Games`, …) | a **collection**: each sub-folder is examined separately |
| exactly one sub-folder with executables | whichever of the two names matches the executables better is the game (wrapper folders collapse) |
| two or more non-generic sub-folders with executables | a **collection** |

So `D:\Games\PLUG&PLAY\A Way Out\AWayOut.exe` is the game *A Way Out*, and
`marvel-vs-capcom\Game\Binaries\Win64\MVC-Win64-Shipping.exe` is one game *Marvel vs. Capcom*.

## 4. Title

In order of precedence:

1. **Your correction** (always wins, kept forever).
2. **GOG manifest** (`goggame-*.info` `name`).
3. **Executable metadata**: the `ProductName` is used only when it is the same title as the
   folder after normalization (it fixes spelling/casing, it never swaps in another edition).
4. **Folder name**, normalized:
   * bracketed tags are dropped: `[FitGirl Repack]`, `(v2.1 + DLC)`;
   * `_`, `.` and (in names without spaces) `-` become spaces;
   * trailing release markers are dropped: `FitGirl`, `Repack`, `DODI`, `ElAmigos`,
     `MULTi13`, `x64`, `v1.5.78`, `Build 12345`, `GOG`, `CODEX`, … (only at the end, so a
     title word in the middle is kept);
   * machine-style names (all lower or all upper case) are title-cased with small words
     lower case (`of`, `the`, `vs.` …) and Roman numerals upper case;
   * a subtitle colon is added after a known franchise (`Batman: Arkham City`,
     `Call of Duty: World at War`) or after a sequel number followed by more words
     (`The Witcher 3: Wild Hunt`);
   * names with deliberate casing (`BioShock Infinite`, `Haze1`) are left as they are.

Title confidence and executable confidence are separate: a clear folder name does not make an
ambiguous executable certain.

## 5. Executable score

Every candidate in a game folder starts at **40** and gets:

| Signal | Points |
|---|---|
| Name equals the game title (after dropping `-Win64-Shipping`, `x64`, `dx12`, …) | +35 |
| Name is the title's initials (`CoDWaW`, `DS2`, `MVC`) | +28 |
| Name contains the title or vice versa (3+ letters) | +25 |
| Name resembles the title (bigram similarity ≥ 0.5) | up to +25 |
| Generic name (`game.exe`, `start.exe`, `launcher.exe`); the title comes from the folder | +5 |
| Name unrelated to the folder | −5 |
| File description / product name matches the title | +15 |
| In the game folder itself | +10 |
| Each folder deeper: generic folder / other folder | −2 / −6 |
| Size ≥ 50 MB / ≥ 10 MB / ≥ 2 MB / ≥ 512 KB / < 100 KB | +12 / +10 / +6 / +2 / −8 |
| Unity player (`<name>_Data` next to it) | +15 |
| Next to `steam_appid.txt` | +5 |
| GOG manifest's primary play task | +40 |
| Unreal `*-Shipping.exe` when a launcher with the same name sits in the game folder | −6 |
| Name contains `launcher` (only when there are other candidates) | −8 |
| Looks like a settings tool (`config`, `settings`, `options`) | −20 |
| Looks like a server (`server`, `dedicated`) | −15 |
| Looks like a modding/editor tool (`editor`, `sdk`, `modtool`, …) | −12 |
| Looks like a benchmark | −10 |
| Console (command-line) program | −15 |

**Detection %** shown in the app = the best candidate's score, clamped to 0–99.

A detection is **Uncertain** (and is never bulk-approved) when the best score is below **65**
or the runner-up scores within **10** points of it.

## 6. Duplicate evidence

| Confidence | Evidence | Effect |
|---|---|---|
| High | a Steam shortcut with the same executable path (and compatible launch options); the executable lies inside an official Steam game's install folder; a shortcut carrying this game's BoilerSteamXL identity | *Already in Steam*; not imported |
| Medium | another BoilerSteamXL game with the same executable; a Steam shortcut starting a program inside this game's folder | *Possible duplicate*; needs an explicit "add a separate shortcut anyway" |
| Low | a Steam shortcut or game with the same name only | *Possible duplicate*; shown, never acted on |

Nothing is ever merged or deleted because of a name match.
