//! Synthetic fixtures: a games folder shaped like a real repack collection and a fake
//! Steam installation. No real game or Steam file is ever touched.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
};

use bsxl_engine::{
    library::Library,
    prefs::Preferences,
    scan::{ScanRoot, DEFAULT_MAX_DEPTH},
    steam::SteamSnapshot,
};
use steam_shortcuts_util::{shortcut::ShortcutOwned, shortcuts_to_bytes, Shortcut};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Tests share one process-wide data folder and lock; run them one at a time.
pub fn serial() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|p| p.into_inner());
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    let home = HOME.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("bsxl-test-home-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    });
    std::env::set_var("BOILERSTEAMXL_HOME", home);
    guard
}

pub struct Temp(pub PathBuf);

impl Temp {
    pub fn new(label: &str) -> Temp {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bsxl-{label}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    pub fn s(&self, rel: &str) -> String {
        self.path(rel).to_string_lossy().to_string()
    }

    /// A placeholder "executable" of `kb` kilobytes (not a real program).
    pub fn exe(&self, rel: &str, kb: usize) -> PathBuf {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![0u8; kb * 1024]).unwrap();
        path
    }

    pub fn dir(&self, rel: &str) -> PathBuf {
        let path = self.path(rel);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Emulates `D:\Games` with collection folders, repack names, redistributables,
/// verification tools, language selectors and Unicode names.
pub fn games_fixture(t: &Temp) {
    let mb = 1024;
    // PLUG&PLAY collection folder.
    t.exe("Games/PLUG&PLAY/marvel-vs-capcom/game.exe", 8 * mb);
    t.exe("Games/PLUG&PLAY/marvel-vs-capcom/Engine/Binaries/Win64/CrashReportClient.exe", 2 * mb);
    t.exe("Games/PLUG&PLAY/marvel-vs-capcom/_Redist/vcredist_x64.exe", 6 * mb);
    t.exe("Games/PLUG&PLAY/A Way Out/AWayOut.exe", 20 * mb);
    t.exe("Games/PLUG&PLAY/A Way Out/QuickSFV.EXE", 200);
    t.exe("Games/PLUG&PLAY/A Way Out/unins000.exe", 1 * mb);
    t.exe("Games/PLUG&PLAY/A Way Out/_CommonRedist/DirectX/Jun2010/DXSETUP.exe", 1 * mb);
    t.exe("Games/PLUG&PLAY/Haze1/Language Selector.exe", 1 * mb);
    t.exe("Games/PLUG&PLAY/Haze1/Binaries/Win64/Game-Win64-Shipping.exe", 30 * mb);
    t.exe("Games/PLUG&PLAY/Other Game/start.exe", 3 * mb);
    // Plain game folders.
    t.exe("Games/Halo/haloce.exe", 5 * mb);
    t.exe("Games/Call of Duty WaW/CoDWaW.exe", 10 * mb);
    t.exe("Games/Call of Duty WaW/Redist/vcredist_x86.exe", 4 * mb);
    // FitGirl-style folder name.
    t.exe("Games/Hollow.Knight.v1.5.78-FitGirl.Repack/hollow_knight.exe", 15 * mb);
    t.dir("Games/Hollow.Knight.v1.5.78-FitGirl.Repack/hollow_knight_Data");
    t.exe("Games/Hollow.Knight.v1.5.78-FitGirl.Repack/MD5/QuickSFV.exe", 200);
    // Unicode and spaces.
    t.exe("Games/Ōkami HD/okami.exe", 12 * mb);
    // A stray redistributable folder at the top level.
    t.exe("Games/_CommonRedist/vcredist/2019/VC_redist.x64.exe", 14 * mb);
}

pub fn prefs_for(t: &Temp, steam: &Path) -> Preferences {
    Preferences {
        steam_path: Some(steam.to_string_lossy().to_string()),
        scan_roots: vec![ScanRoot {
            path: t.s("Games"),
            enabled: true,
            max_depth: DEFAULT_MAX_DEPTH,
        }],
        ..Preferences::default()
    }
}

pub fn shortcut(name: &str, exe: &str, args: &str) -> ShortcutOwned {
    let quoted = format!("\"{exe}\"");
    let dir = format!("\"{}\"", Path::new(exe).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
    Shortcut::new("0", name, &quoted, &dir, "", "", args).to_owned()
}

pub fn write_vdf(path: &Path, shortcuts: &[ShortcutOwned]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let refs: Vec<Shortcut> = shortcuts.iter().map(|s| s.borrow()).collect();
    std::fs::write(path, shortcuts_to_bytes(&refs)).unwrap();
}

/// A fake Steam folder with accounts, shortcuts and one official game.
pub fn steam_fixture(t: &Temp, users: &[(&str, Vec<ShortcutOwned>)]) -> PathBuf {
    let steam = t.dir("Steam");
    std::fs::write(steam.join("steam.exe"), b"").unwrap();
    for (id, shortcuts) in users {
        write_vdf(&steam.join("userdata").join(id).join("config").join("shortcuts.vdf"), shortcuts);
    }
    // An official Steam game in a second library.
    let library = t.dir("SteamLibrary");
    std::fs::create_dir_all(steam.join("steamapps")).unwrap();
    std::fs::write(
        steam.join("steamapps").join("libraryfolders.vdf"),
        format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
            library.to_string_lossy().replace('\\', "\\\\")
        ),
    )
    .unwrap();
    std::fs::create_dir_all(library.join("steamapps").join("common").join("Portal 2")).unwrap();
    std::fs::write(
        library.join("steamapps").join("appmanifest_620.acf"),
        "\"AppState\"\n{\n\t\"appid\"\t\t\"620\"\n\t\"name\"\t\t\"Portal 2\"\n\t\"installdir\"\t\t\"Portal 2\"\n}\n",
    )
    .unwrap();
    steam
}

pub fn vdf_path(steam: &Path, user: &str) -> PathBuf {
    steam.join("userdata").join(user).join("config").join("shortcuts.vdf")
}

pub fn read_vdf(steam: &Path, user: &str) -> Vec<ShortcutOwned> {
    let bytes = std::fs::read(vdf_path(steam, user)).unwrap_or_default();
    steam_shortcuts_util::parse_shortcuts(&bytes).unwrap().iter().map(|s| s.to_owned()).collect()
}

pub fn snap(steam: &Path) -> SteamSnapshot {
    bsxl_engine::steam::snapshot(&steam.to_string_lossy())
}

pub fn refresh(prefs: &Preferences, lib: &mut Library) -> bsxl_engine::library::ScanSummary {
    bsxl_engine::refresh(prefs, lib, None, &|_| {}, &AtomicBool::new(false))
}

pub fn entry_by_title<'a>(lib: &'a Library, title: &str) -> &'a bsxl_engine::library::Entry {
    lib.entries
        .values()
        .find(|e| e.display_title() == title)
        .unwrap_or_else(|| panic!("no entry titled {title}; have {:?}", lib.entries.values().map(|e| e.display_title()).collect::<Vec<_>>()))
}

pub fn id_of(lib: &Library, title: &str) -> String {
    entry_by_title(lib, title).id.clone()
}

pub const NOT_RUNNING: fn() -> bool = || false;
pub const RUNNING: fn() -> bool = || true;

/// The operation lock, as every real caller holds it around a write.
pub fn lock() -> bsxl_engine::import::OpLock {
    bsxl_engine::import::wait_for_lock(std::time::Duration::from_secs(10)).unwrap()
}
