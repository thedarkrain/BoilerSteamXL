//! Game-discovery benchmark: BoilR's "Standalone" folder scanner vs BoilerSteamXL's scanner,
//! on the same synthetic games folder, scored against built-in ground truth.
//!
//! It never touches Steam, a real games folder or a real config folder: the corpus is written
//! to a fresh temporary folder (or `--corpus <empty dir>`), BoilR's config is pointed at a
//! scratch folder through `BOILERSTEAMXL_HOME`, and the "executables" are minimal PE headers
//! with no code and no sections (Windows refuses to run them).
//!
//! Usage: compare [--runs N] [--corpus <empty dir>] [--json <file>]

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

use boilr::platforms::{get_platforms, GamesPlatform};
use bsxl_engine::{
    norm_path,
    rules::RuleSet,
    scan::{scan, PeCache, ScanRoot, DEFAULT_MAX_DEPTH},
};

#[derive(Clone, Copy, PartialEq)]
enum Label {
    /// The executable a shortcut should start.
    Main,
    /// Also starts the game correctly (Unreal shipping binary, 32-bit build, official launcher).
    Alt,
    /// Starts the game but is not the installed one (crack/NoDVD copy, spare `[old]` copy).
    Spare,
    /// Not a game: installer, redistributable, uninstaller, crash handler, tool, server...
    Junk(&'static str),
}
use Label::{Alt, Junk, Main, Spare};

/// (path inside the folder, size in KiB, label, console subsystem)
type File = (&'static str, u64, Label, bool);

struct Folder {
    dir: &'static str,
    /// Acceptable display names; empty for folders that are not a game.
    titles: &'static [&'static str],
    files: &'static [File],
    /// Extra empty folders (Unity `<game>_Data`).
    dirs: &'static [&'static str],
    traps: &'static str,
}

const REDIST: &str = "redistributable";
const INSTALLER: &str = "installer";
const UNINSTALLER: &str = "uninstaller";
const CRASH: &str = "crash handler";
const TOOL: &str = "tool";
const SERVER: &str = "server";
const VERIFY: &str = "verification";
const LANG: &str = "language selector";
const SETTINGS: &str = "settings tool";
const UPDATER: &str = "updater";

const UNITY_CRASH: File = ("UnityCrashHandler64.exe", 1450, Junk(CRASH), false);
const UNINS: File = ("unins000.exe", 1900, Junk(UNINSTALLER), false);

#[rustfmt::skip]
const CORPUS: &[Folder] = &[
    Folder { dir: "Hollow.Knight.v1.5.78-FitGirl.Repack", titles: &["Hollow Knight"], dirs: &["hollow_knight_Data"],
        files: &[("hollow_knight.exe", 650, Main, false), UNITY_CRASH, UNINS, ("MD5/QuickSFV.exe", 100, Junk(VERIFY), false)],
        traps: "FitGirl folder name, Unity, crash handler, uninstaller, MD5/QuickSFV" },
    Folder { dir: "the-witcher-3-wild-hunt", titles: &["The Witcher 3: Wild Hunt"], dirs: &[],
        files: &[("bin/x64/witcher3.exe", 60000, Main, false), ("REDprelauncher.exe", 2400, Alt, false), UNINS,
                 ("_CommonRedist/vcredist/2015/vc_redist.x64.exe", 14000, Junk(REDIST), false),
                 ("_CommonRedist/DirectX/Jun2010/DXSETUP.exe", 520, Junk(REDIST), false)],
        traps: "machine-style name, exe in bin\\x64, _CommonRedist" },
    Folder { dir: "call.of.duty.world.at.war", titles: &["Call of Duty: World at War"], dirs: &[],
        files: &[("CoDWaW.exe", 8000, Main, false), ("CoDWaWmp.exe", 8100, Alt, false),
                 ("Redist/vcredist_x86.exe", 4000, Junk(REDIST), false), ("DirectX/DXSETUP.exe", 520, Junk(REDIST), false)],
        traps: "dotted name, exe is initials, multiplayer exe, Redist + DirectX" },
    Folder { dir: "PLUG&PLAY/A Way Out", titles: &["A Way Out"], dirs: &[],
        files: &[("AWayOut.exe", 250, Main, false), ("Haven/Binaries/Win64/AWayOut-Win64-Shipping.exe", 72000, Alt, false),
                 ("Engine/Extras/Redist/en-us/UE4PrereqSetup_x64.exe", 18000, Junk(REDIST), false),
                 ("Engine/Binaries/ThirdParty/CEF3/Win64/UnrealCEFSubProcess.exe", 900, Junk(TOOL), false),
                 ("Engine/Binaries/Win64/CrashReportClient.exe", 15000, Junk(CRASH), false)],
        traps: "collection folder, Unreal layout, UE prerequisites, CEF helper, crash reporter" },
    Folder { dir: "PLUG&PLAY/Hades", titles: &["Hades"], dirs: &[],
        files: &[("x64/Hades.exe", 15000, Main, false), ("x86/Hades.exe", 14000, Alt, false)],
        traps: "same exe in x64 and x86" },
    Folder { dir: "PLUG&PLAY/Celeste", titles: &["Celeste"], dirs: &[],
        files: &[("Celeste.exe", 2100, Main, false), ("unins000.exe", 1300, Junk(UNINSTALLER), false)],
        traps: "plain (control)" },
    Folder { dir: "PLUG&PLAY/Stardew Valley", titles: &["Stardew Valley"], dirs: &[],
        files: &[("Stardew Valley.exe", 170, Main, false), ("StardewModdingAPI.exe", 160, Alt, false)],
        traps: "small exe, mod loader next to it" },
    Folder { dir: "PLUG&PLAY/Cuphead", titles: &["Cuphead"], dirs: &["Cuphead_Data"],
        files: &[("Cuphead.exe", 650, Main, false), UNITY_CRASH],
        traps: "Unity (control)" },
    Folder { dir: "PLUG&PLAY/DOOM Eternal", titles: &["DOOM Eternal"], dirs: &[],
        files: &[("DOOMEternalx64vk.exe", 60000, Main, false),
                 ("_CommonRedist/vcredist/2019/VC_redist.x64.exe", 24000, Junk(REDIST), false)],
        traps: "exe name has build suffix (x64vk)" },
    Folder { dir: "PLUG&PLAY/Grand Theft Auto V", titles: &["Grand Theft Auto V"], dirs: &[],
        files: &[("GTA5.exe", 60000, Main, false), ("PlayGTAV.exe", 1000, Alt, false), ("GTAVLauncher.exe", 20000, Alt, false),
                 ("Redistributables/vcredist_x64.exe", 7000, Junk(REDIST), false),
                 ("Redistributables/DirectXRedist/DXSETUP.exe", 520, Junk(REDIST), false),
                 ("Installers/Social-Club-Setup.exe", 90000, Junk(INSTALLER), false)],
        traps: "abbreviated exe, three launch exes, bundled installer" },
    Folder { dir: "PLUG&PLAY/LISA The Painful", titles: &["LISA The Painful", "LISA: The Painful"], dirs: &[],
        files: &[("Game.exe", 140, Main, false)],
        traps: "generic Game.exe (RPG Maker)" },
    Folder { dir: "PLUG&PLAY/Doki Doki Literature Club", titles: &["Doki Doki Literature Club"], dirs: &[],
        files: &[("DDLC.exe", 110, Main, false),
                 ("lib/windows-i686/python.exe", 30, Junk(TOOL), true), ("lib/windows-i686/pythonw.exe", 30, Junk(TOOL), false),
                 ("lib/windows-i686/zsync.exe", 90, Junk(TOOL), true), ("lib/windows-i686/zsyncmake.exe", 80, Junk(TOOL), true)],
        traps: "Ren'Py: initials exe, bundled python/zsync" },
    Folder { dir: "PLUG&PLAY/Undertale", titles: &["Undertale"], dirs: &[],
        files: &[("UNDERTALE.exe", 7000, Main, false)],
        traps: "plain (control)" },
    Folder { dir: "Terraria", titles: &["Terraria"], dirs: &[],
        files: &[("Terraria.exe", 8000, Main, false), ("TerrariaServer.exe", 8000, Junk(SERVER), true)],
        traps: "dedicated server next to the game" },
    Folder { dir: "Portal 2", titles: &["Portal 2"], dirs: &[],
        files: &[("portal2.exe", 400, Main, false), ("bin/vbsp.exe", 900, Junk(TOOL), true)],
        traps: "Source SDK map tool in bin" },
    Folder { dir: "Half-Life 2", titles: &["Half-Life 2"], dirs: &[],
        files: &[("hl2.exe", 400, Main, false)],
        traps: "exe is initials (hl2)" },
    Folder { dir: "Left 4 Dead 2", titles: &["Left 4 Dead 2"], dirs: &[],
        files: &[("left4dead2.exe", 400, Main, false), ("srcds.exe", 400, Junk(SERVER), true)],
        traps: "dedicated server next to the game" },
    Folder { dir: "Resident Evil 4", titles: &["Resident Evil 4"], dirs: &[],
        files: &[("Bin32/bio4.exe", 9000, Main, false)],
        traps: "unrelated exe name (bio4)" },
    Folder { dir: "Batman Arkham City GOTY", titles: &["Batman Arkham City", "Batman Arkham City GOTY"], dirs: &[],
        files: &[("Binaries/Win32/BatmanAC.exe", 28000, Main, false), ("Binaries/Win32/BmLauncher.exe", 1300, Alt, false),
                 ("Redist/PhysX_9.10.0513_SystemSoftware.exe", 30000, Junk(REDIST), false)],
        traps: "abbreviated exe, config launcher, PhysX installer" },
    Folder { dir: "Need for Speed Most Wanted 2005", titles: &["Need for Speed Most Wanted 2005", "Need for Speed Most Wanted"], dirs: &[],
        files: &[("speed.exe", 6000, Main, false)],
        traps: "unrelated exe name (speed)" },
    Folder { dir: "DARK SOULS III", titles: &["DARK SOULS III"], dirs: &[],
        files: &[("Game/DarkSoulsIII.exe", 90000, Main, false)],
        traps: "exe in Game sub-folder" },
    Folder { dir: "Repacks/2023/Action/Sekiro.Shadows.Die.Twice.MULTi13-ElAmigos", titles: &["Sekiro Shadows Die Twice", "Sekiro: Shadows Die Twice"], dirs: &[],
        files: &[("sekiro.exe", 65000, Main, false), ("_Redist/vc_redist.x64.exe", 14000, Junk(REDIST), false),
                 ("_Redist/dxwebsetup.exe", 290, Junk(REDIST), false)],
        traps: "scene name, 4 levels deep, _Redist" },
    Folder { dir: "Repacks/Cyberpunk 2077 (v2.1 + DLC) [DODI Repack]", titles: &["Cyberpunk 2077"], dirs: &[],
        files: &[("bin/x64/Cyberpunk2077.exe", 60000, Main, false), ("REDprelauncher.exe", 2400, Alt, false),
                 ("bin/x64/CrashReporter/CrashReporter.exe", 3000, Junk(CRASH), false), ("unins000.exe", 3000, Junk(UNINSTALLER), false)],
        traps: "DODI name with brackets, crash reporter, uninstaller" },
    Folder { dir: "Dishonored 2", titles: &["Dishonored 2"], dirs: &[],
        files: &[("Dishonored2.exe", 70000, Main, false), ("Language Selector.exe", 500, Junk(LANG), false),
                 ("_Redist/dxwebsetup.exe", 290, Junk(REDIST), false), ("_Redist/vcredist_x64.exe", 7000, Junk(REDIST), false)],
        traps: "Language Selector.exe, _Redist" },
    Folder { dir: "PLUG&PLAY/Stray", titles: &["Stray"], dirs: &[],
        files: &[("Stray.exe", 250, Main, false), ("Hk_project/Binaries/Win64/Stray-Win64-Shipping.exe", 80000, Alt, false),
                 ("Engine/Binaries/Win64/CrashReportClient.exe", 15000, Junk(CRASH), false),
                 ("Engine/Extras/Redist/en-us/UE4PrereqSetup_x64.exe", 18000, Junk(REDIST), false)],
        traps: "Unreal with unrelated project folder (Hk_project)" },
    Folder { dir: "Hellblade Senuas Sacrifice", titles: &["Hellblade Senuas Sacrifice"], dirs: &[],
        files: &[("HellbladeGame.exe", 250, Main, false),
                 ("HellbladeGame/Binaries/Win64/HellbladeGame-Win64-Shipping.exe", 70000, Alt, false),
                 ("Engine/Binaries/ThirdParty/CEF3/Win64/UnrealCEFSubProcess.exe", 900, Junk(TOOL), false)],
        traps: "Unreal, exe/project name differs from folder" },
    Folder { dir: "PLUG&PLAY/Ori and the Will of the Wisps", titles: &["Ori and the Will of the Wisps"], dirs: &["oriwotw_Data"],
        files: &[("oriwotw.exe", 650, Main, false), UNITY_CRASH],
        traps: "Unity, abbreviated exe" },
    Folder { dir: "PLUG&PLAY/INSIDE", titles: &["INSIDE"], dirs: &["INSIDE_Data"],
        files: &[("INSIDE.exe", 650, Main, false), UNITY_CRASH],
        traps: "Unity (control)" },
    Folder { dir: "Mount & Blade Warband", titles: &["Mount & Blade Warband"], dirs: &[],
        files: &[("mb_warband.exe", 5000, Main, false), ("mb_warband_dedicated.exe", 4000, Junk(SERVER), true)],
        traps: "abbreviated exe, dedicated server" },
    Folder { dir: "Euro Truck Simulator 2", titles: &["Euro Truck Simulator 2"], dirs: &[],
        files: &[("bin/win_x64/eurotrucks2.exe", 40000, Main, false), ("bin/win_x86/eurotrucks2.exe", 35000, Alt, false)],
        traps: "exe in bin\\win_x64 and bin\\win_x86, different name" },
    Folder { dir: "Grim Dawn", titles: &["Grim Dawn"], dirs: &[],
        files: &[("x64/Grim Dawn.exe", 9000, Main, false), ("Grim Dawn.exe", 8000, Alt, false), ("AssetManager.exe", 3000, Junk(TOOL), false)],
        traps: "32- and 64-bit exe, modding tool" },
    Folder { dir: "Max Payne 3", titles: &["Max Payne 3"], dirs: &[],
        files: &[("MaxPayne3.exe", 35000, Main, false), ("PlayMaxPayne3.exe", 1000, Alt, false),
                 ("Crack/MaxPayne3.exe", 35000, Spare, false), ("Crack/PlayMaxPayne3.exe", 1000, Spare, false)],
        traps: "Crack sub-folder with copies of the exes" },
    Folder { dir: "Sniper Elite V2", titles: &["Sniper Elite V2"], dirs: &[],
        files: &[("bin/SniperEliteV2.exe", 20000, Main, false), ("NoDVD/SniperEliteV2.exe", 20000, Spare, false)],
        traps: "NoDVD sub-folder with a copy" },
    Folder { dir: "Far Cry 3", titles: &["Far Cry 3"], dirs: &[],
        files: &[("bin/farcry3_d3d11.exe", 3000, Main, false), ("bin/farcry3.exe", 3000, Alt, false),
                 ("bin/FC3Updater.exe", 1500, Junk(UPDATER), false),
                 ("Fix/bin/farcry3_d3d11.exe", 3000, Spare, false), ("Fix/FC3Loader.exe", 200, Spare, false)],
        traps: "Fix sub-folder with loader, updater, DX9/DX11 exes" },
    Folder { dir: "Fallout New Vegas", titles: &["Fallout New Vegas"], dirs: &[],
        files: &[("FalloutNV.exe", 15000, Main, false), ("FalloutNVLauncher.exe", 9000, Alt, false)],
        traps: "game launcher next to the game" },
    Folder { dir: "The Elder Scrolls V Skyrim", titles: &["The Elder Scrolls V Skyrim"], dirs: &[],
        files: &[("TESV.exe", 17000, Main, false), ("SkyrimLauncher.exe", 4000, Alt, false), ("CreationKit.exe", 25000, Junk(TOOL), false)],
        traps: "initials exe, launcher, editor" },
    Folder { dir: "Dead Rising 2", titles: &["Dead Rising 2"], dirs: &[],
        files: &[("deadrising2.exe", 20000, Main, false), ("DeadRising2Config.exe", 600, Junk(SETTINGS), false)],
        traps: "settings tool named after the game" },
    Folder { dir: "Retro Rally", titles: &["Retro Rally"], dirs: &[],
        files: &[("start.exe", 3000, Main, false), ("Settings.exe", 800, Junk(SETTINGS), false)],
        traps: "generic start.exe, Settings.exe" },
    Folder { dir: "S.T.A.L.K.E.R. Anomaly", titles: &["S.T.A.L.K.E.R. Anomaly"], dirs: &[],
        files: &[("AnomalyLauncher.exe", 600, Main, false), ("bin/AnomalyDX11.exe", 9000, Alt, false),
                 ("bin/AnomalyDX9.exe", 9000, Alt, false), ("bin/AnomalyDX11AVX.exe", 9000, Alt, false)],
        traps: "the right exe is a launcher" },
    Folder { dir: "Ōkami HD", titles: &["Ōkami HD", "Okami HD"], dirs: &[],
        files: &[("okami.exe", 40000, Main, false)],
        traps: "Unicode folder name" },
    Folder { dir: "東方紅魔郷", titles: &["東方紅魔郷"], dirs: &[],
        files: &[("東方紅魔郷.exe", 800, Main, false), ("custom.exe", 200, Junk(SETTINGS), false)],
        traps: "Japanese folder and exe name, config tool" },
    Folder { dir: "Hotline Miami", titles: &["Hotline Miami"], dirs: &[],
        files: &[("HotlineMiami.exe", 9000, Main, false), ("HotlineMiami[old].exe", 9000, Spare, false)],
        traps: "spare [old] copy" },
    Folder { dir: "Repacks/2019/Archive/Old/Backup/Metro 2033 Redux", titles: &["Metro 2033 Redux"], dirs: &[],
        files: &[("metro.exe", 30000, Main, false)],
        traps: "6 folders deep, short exe name" },
    Folder { dir: "Repacks/2019/Archive/Old/Backup/Shooters/Bulletstorm", titles: &["Bulletstorm"], dirs: &[],
        files: &[("Bulletstorm.exe", 40000, Main, false)],
        traps: "7 folders deep" },
    // Not games.
    Folder { dir: "New folder", titles: &[], dirs: &[], files: &[], traps: "empty folder" },
    Folder { dir: "Elden.Ring.v1.10-FitGirl.Repack", titles: &[], dirs: &[],
        files: &[("setup.exe", 5000, Junk(INSTALLER), false), ("MD5/QuickSFV.exe", 100, Junk(VERIFY), false)],
        traps: "repack not installed yet: only setup.exe" },
    Folder { dir: "_CommonRedist", titles: &[], dirs: &[],
        files: &[("DirectX/DXSETUP.exe", 520, Junk(REDIST), false), ("vcredist/2022/VC_redist.x64.exe", 24000, Junk(REDIST), false)],
        traps: "stray redistributables at the top" },
    Folder { dir: "Tools/DS4Windows", titles: &[], dirs: &[],
        files: &[("DS4Windows.exe", 4000, Junk(TOOL), false)],
        traps: "controller utility kept with the games" },
];

/// A 64-bit PE header with no sections and no code: enough for a PE parser to read the
/// subsystem, nothing that can run. Padded with zeros to `size` bytes.
fn write_pe(path: &Path, size: u64, console: bool) -> std::io::Result<()> {
    let mut h = vec![0u8; 0x200];
    let put16 = |h: &mut [u8], at: usize, v: u16| h[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let put32 = |h: &mut [u8], at: usize, v: u32| h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    h[0..2].copy_from_slice(b"MZ");
    put32(&mut h, 0x3c, 0x40);
    h[0x40..0x44].copy_from_slice(b"PE\0\0");
    put16(&mut h, 0x44, 0x8664); // AMD64
    put16(&mut h, 0x54, 0xf0); // SizeOfOptionalHeader
    put16(&mut h, 0x56, 0x22); // executable, large address aware
    let o = 0x58;
    put16(&mut h, o, 0x20b); // PE32+
    h[o + 24..o + 32].copy_from_slice(&0x1_4000_0000u64.to_le_bytes()); // ImageBase
    put32(&mut h, o + 32, 0x1000); // SectionAlignment
    put32(&mut h, o + 36, 0x200); // FileAlignment
    put16(&mut h, o + 40, 6);
    put16(&mut h, o + 48, 6);
    put32(&mut h, o + 56, 0x1000); // SizeOfImage
    put32(&mut h, o + 60, 0x200); // SizeOfHeaders
    put16(&mut h, o + 68, if console { 3 } else { 2 });
    put16(&mut h, o + 70, 0x8160);
    for (i, v) in [0x10_0000u64, 0x1000, 0x10_0000, 0x1000].iter().enumerate() {
        h[o + 72 + i * 8..o + 80 + i * 8].copy_from_slice(&v.to_le_bytes());
    }
    put32(&mut h, o + 108, 16); // NumberOfRvaAndSizes
    let mut file = fs::File::create(path)?;
    file.write_all(&h)?;
    file.set_len(size.max(h.len() as u64))
}

fn build_corpus(root: &Path) -> std::io::Result<()> {
    for folder in CORPUS {
        let dir = root.join(folder.dir);
        fs::create_dir_all(&dir)?;
        for extra in folder.dirs {
            fs::create_dir_all(dir.join(extra))?;
        }
        for (rel, kib, _, console) in folder.files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            write_pe(&path, kib * 1024, *console)?;
        }
    }
    Ok(())
}

/// One entry a tool would add to Steam: (display name, executable relative to the root).
type Entry = (String, String);

fn relative(root: &str, exe: &str) -> String {
    let exe = norm_path(exe);
    exe.strip_prefix(&format!("{}/", norm_path(root))).unwrap_or(&exe).to_string()
}

fn run_boilr(platform: &dyn GamesPlatform, root: &str) -> Result<Vec<Entry>, String> {
    let shortcuts = platform.get_shortcut_info().map_err(|e| e.to_string())?;
    let mut out: Vec<Entry> = shortcuts
        .into_iter()
        .map(|s| (s.shortcut.app_name, relative(root, &s.shortcut.exe)))
        .collect();
    out.sort();
    Ok(out)
}

struct BsxlRun {
    entries: Vec<Entry>,
    /// Executables of the games it flags as uncertain (shown first in its review queue).
    uncertain: Vec<String>,
    /// The full scan result: every candidate with its score and reasons. Serialized outside
    /// the timed region, so the JSON report does not count against BoilerSteamXL's time.
    games: Vec<bsxl_engine::scan::DiscoveredGame>,
}

/// `cache` is the PE-metadata cache the app keeps between scans; pass an empty one for a
/// cold scan (BoilR has no such cache).
fn run_bsxl(root: &str, cache: &mut PeCache) -> BsxlRun {
    let roots = [ScanRoot { path: root.to_string(), enabled: true, max_depth: DEFAULT_MAX_DEPTH }];
    let result = scan(&roots, &RuleSet::recommended(), cache, &|_| {}, &AtomicBool::new(false));
    let mut entries = Vec::new();
    let mut uncertain = Vec::new();
    for g in &result.games {
        let Some(best) = g.candidates.first() else { continue };
        let exe = relative(root, &best.path);
        if g.uncertain {
            uncertain.push(exe.clone());
        }
        entries.push((g.title.clone(), exe));
    }
    entries.sort();
    BsxlRun { entries, uncertain, games: result.games }
}

#[derive(Default)]
struct Score {
    correct: usize,
    missed: usize,
    wrong: usize,
    split: usize,
    junk: usize,
    names: usize,
    per_game: Vec<String>,
    junk_list: Vec<String>,
}

fn label_of(exe: &str) -> Option<(usize, Label)> {
    CORPUS.iter().enumerate().find_map(|(i, f)| {
        f.files
            .iter()
            .find(|(rel, ..)| norm_path(&format!("{}/{rel}", f.dir)) == exe)
            .map(|(_, _, label, _)| (i, *label))
    })
}

fn score(entries: &[Entry], uncertain: &[String]) -> Score {
    let mut s = Score::default();
    let mut by_game: HashMap<usize, Vec<(&Entry, Label)>> = HashMap::new();
    let junk = |s: &mut Score, entry: &Entry, kind: &str| {
        s.junk += 1;
        s.junk_list.push(format!("{} → `{}` ({kind})", entry.0, entry.1));
    };
    for entry in entries {
        match label_of(&entry.1) {
            Some((i, label)) if !CORPUS[i].titles.is_empty() => by_game.entry(i).or_default().push((entry, label)),
            Some((_, Junk(kind))) => junk(&mut s, entry, kind),
            _ => junk(&mut s, entry, "not a game"),
        }
    }
    // Titles must match an accepted name up to case and spacing; punctuation counts, so
    // "S T A L K E R Anomaly" is not "S.T.A.L.K.E.R. Anomaly".
    let same = |a: &str, b: &str| {
        let squash = |x: &str| x.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        squash(a) == squash(b)
    };
    for (i, folder) in CORPUS.iter().enumerate().filter(|(_, f)| !f.titles.is_empty()) {
        let all = by_game.remove(&i).unwrap_or_default();
        // A junk exe in a game's folder is an extra junk entry, unless it is the game's only
        // entry: then the tool picked the wrong exe for that game.
        let (mut found, extra): (Vec<_>, Vec<_>) = all.into_iter().partition(|(_, l)| !matches!(l, Junk(_)));
        let mut extra = extra.into_iter();
        if found.is_empty() {
            found.extend(extra.next());
        }
        for (e, l) in extra {
            junk(&mut s, e, if let Junk(kind) = l { kind } else { "" });
        }
        let found = found.as_slice();
        let good: Vec<&Entry> = found.iter().filter(|(_, l)| matches!(l, Main | Alt)).map(|(e, _)| *e).collect();
        let name_ok = good.iter().any(|e| folder.titles.iter().any(|t| same(t, &e.0)));
        if name_ok {
            s.names += 1;
        }
        let file = |e: &Entry| e.1.rsplit('/').next().unwrap_or_default().to_string();
        let name_note = match good.first() {
            Some(e) if !name_ok => format!(", name \"{}\"", e.0),
            _ => String::new(),
        };
        let verdict = match (found.len(), good.len()) {
            (0, _) => {
                s.missed += 1;
                "missed".to_string()
            }
            (1, 1) => {
                s.correct += 1;
                format!("found ({}){name_note}", file(good[0]))
            }
            (n, _) if n > 1 => {
                s.split += 1;
                let files: Vec<String> = found.iter().map(|(e, _)| file(e)).collect();
                format!("split ×{n} ({}){name_note}", files.join(", "))
            }
            _ => {
                s.wrong += 1;
                let why = match found[0].1 {
                    Junk(kind) => kind,
                    _ => "spare copy",
                };
                format!("wrong exe ({}, {why})", file(found[0].0))
            }
        };
        let flagged = found.iter().any(|(e, _)| uncertain.contains(&e.1));
        s.per_game.push(if flagged { format!("{verdict} · flagged uncertain") } else { verdict });
    }
    s
}

fn median(times: &mut [f64]) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let runs: usize = arg("--runs").map_or(Ok(5), |n| n.parse().map_err(|_| "--runs needs a number"))?.max(1);
    let scratch = std::env::temp_dir().join(format!("bsxl-compare-{}", std::process::id()));
    let (base, keep) = match arg("--corpus") {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_some()) {
                return Err(format!("{} is not empty; give an empty or new folder", dir.display()));
            }
            (dir, true)
        }
        None => (scratch.clone(), false),
    };
    let root = base.join("Games");
    build_corpus(&root).map_err(|e| format!("could not write the corpus: {e}"))?;
    let root_str = root.to_string_lossy().to_string();

    // BoilR reads its settings from <config folder>/config.toml; point it at a scratch folder
    // holding only the Standalone settings (default options, one scan folder).
    let config_dir = scratch.join("boilr-config");
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    fs::write(
        config_dir.join("config.toml"),
        format!("[standalone]\nenabled = true\nscan_directories = ['{root_str}']\nselected_executables = []\n"),
    )
    .map_err(|e| e.to_string())?;
    std::env::set_var("BOILERSTEAMXL_HOME", &config_dir);
    let platforms = get_platforms();
    let standalone = platforms
        .iter()
        .find(|p| p.code_name() == "standalone")
        .ok_or("BoilR has no standalone platform")?;

    // One unmeasured run each warms the file system cache for both.
    run_boilr(standalone.as_ref(), &root_str)?;
    let mut warm_cache = PeCache::new();
    let first = run_bsxl(&root_str, &mut warm_cache);
    let (mut boilr_times, mut bsxl_times, mut warm_times) = (Vec::new(), Vec::new(), Vec::new());
    let (mut boilr_runs, mut bsxl_runs) = (Vec::new(), Vec::new());
    for _ in 0..runs {
        let t = Instant::now();
        boilr_runs.push(run_boilr(standalone.as_ref(), &root_str)?);
        boilr_times.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        bsxl_runs.push(run_bsxl(&root_str, &mut PeCache::new()).entries);
        bsxl_times.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        bsxl_runs.push(run_bsxl(&root_str, &mut warm_cache).entries);
        warm_times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    // Cleaned up before printing, so a closed pipe cannot leave 1.6 GB behind.
    let _ = fs::remove_dir_all(&scratch);
    let stable = |r: &[Vec<Entry>]| r.iter().all(|x| *x == r[0]);
    let uncertain = first.uncertain.len();
    let (b, x) = (score(&boilr_runs[0], &[]), score(&bsxl_runs[0], &first.uncertain));
    let games = CORPUS.iter().filter(|f| !f.titles.is_empty()).count();
    let exes: usize = CORPUS.iter().map(|f| f.files.len()).sum();
    let (bt, xt, wt) = (median(&mut boilr_times), median(&mut bsxl_times), median(&mut warm_times));
    let yes = |v: bool| if v { "yes" } else { "no" };

    println!("Corpus: {games} games, {} folders that are not games, {exes} .exe files, at {root_str}\n", CORPUS.len() - games);
    println!("| Metric | BoilR (Standalone scanner) | BoilerSteamXL |");
    println!("|---|---|---|");
    println!("| Games found correctly (one entry, right exe) | {} / {games} | {} / {games} |", b.correct, x.correct);
    println!("| Missed games | {} | {} |", b.missed, x.missed);
    println!("| Wrong exe chosen | {} | {} |", b.wrong, x.wrong);
    println!("| Split games (listed more than once) | {} | {} |", b.split, x.split);
    println!("| Junk entries (installers, redists, tools, ...) | {} | {} |", b.junk, x.junk);
    println!("| Correct display names | {} / {games} | {} / {games} |", b.names, x.names);
    println!("| Entries it would add to Steam | {} | {} |", boilr_runs[0].len(), bsxl_runs[0].len());
    println!("| Flagged for review as uncertain | n/a | {uncertain} |");
    println!("| Same result on every scan ({runs} runs) | {} | {} |", yes(stable(&boilr_runs)), yes(stable(&bsxl_runs)));
    println!("| Median scan time ({runs} runs) | {bt:.1} ms | {xt:.1} ms |");
    println!("| Median scan time, PE cache warm (as on the app's later scans) | n/a (no cache) | {wt:.1} ms |");

    println!("\n| Game folder | Traps | BoilR | BoilerSteamXL |");
    println!("|---|---|---|---|");
    for (i, folder) in CORPUS.iter().filter(|f| !f.titles.is_empty()).enumerate() {
        println!("| `{}` | {} | {} | {} |", folder.dir, folder.traps, b.per_game[i], x.per_game[i]);
    }
    let listed = |entries: &[Entry], dir: &str| {
        let prefix = format!("{}/", norm_path(dir));
        if entries.iter().any(|e| e.1.starts_with(&prefix)) { "listed (junk)" } else { "not listed" }
    };
    for folder in CORPUS.iter().filter(|f| f.titles.is_empty()) {
        let (bl, xl) = (listed(&boilr_runs[0], folder.dir), listed(&bsxl_runs[0], folder.dir));
        println!("| `{}` (not a game) | {} | {bl} | {xl} |", folder.dir, folder.traps);
    }
    for (name, s) in [("BoilR", &b), ("BoilerSteamXL", &x)] {
        println!("\nJunk entries, {name}: {}", if s.junk_list.is_empty() { "none".to_string() } else { String::new() });
        for j in &s.junk_list {
            println!("- {j}");
        }
    }

    let json_path = arg("--json").map_or_else(|| std::env::temp_dir().join("bsxl-compare-results.json"), PathBuf::from);
    let side = |s: &Score, entries: &[Entry], t: f64, times: &[f64], stable: bool| {
        serde_json::json!({
            "games_found_correctly": s.correct, "missed": s.missed, "wrong_exe": s.wrong, "split": s.split,
            "junk": s.junk, "correct_names": s.names, "entries": entries.len(), "stable": stable,
            "median_ms": t, "times_ms": times, "per_game": s.per_game, "junk_entries": s.junk_list,
            "raw": entries.iter().map(|(t, e)| serde_json::json!({"title": t, "exe": e})).collect::<Vec<_>>(),
        })
    };
    let json = serde_json::json!({
        "corpus": { "games": games, "non_game_folders": CORPUS.len() - games, "exe_files": exes,
                    "folders": CORPUS.iter().map(|f| f.dir).collect::<Vec<_>>() },
        "runs": runs,
        "boilr": side(&b, &boilr_runs[0], bt, &boilr_times, stable(&boilr_runs)),
        "boilersteamxl": side(&x, &bsxl_runs[0], xt, &bsxl_times, stable(&bsxl_runs)),
        "boilersteamxl_uncertain": first.uncertain,
        "boilersteamxl_warm_cache_median_ms": wt,
        "boilersteamxl_warm_cache_times_ms": warm_times,
        "boilersteamxl_scan": first.games,
    });
    fs::write(&json_path, serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    println!("\nJSON: {}", json_path.display());

    if keep {
        println!("Corpus kept at {}", root.display());
    }
    Ok(())
}
