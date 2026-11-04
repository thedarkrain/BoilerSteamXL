//! Where BoilerSteamXL keeps its data.
//!
//! Installed: `%APPDATA%\BoilerSteamXL`. Portable: a `BoilerSteamXL.portable` file next to
//! the executable switches to `BoilerSteamXL-data` beside it; if that folder is not
//! writable (a read-only USB stick, Program Files), `%LOCALAPPDATA%\BoilerSteamXL-portable`
//! is used and the UI says so.

use std::path::{Path, PathBuf};

use serde::Serialize;

pub const PORTABLE_MARKER: &str = "BoilerSteamXL.portable";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Installed,
    Portable,
    /// Portable, but the folder next to the executable could not be written.
    PortableFallback,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataLocation {
    pub mode: Mode,
    /// `None` for the installed edition, which uses the default folder.
    pub dir: Option<PathBuf>,
}

fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-test");
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn resolve(exe_dir: &Path, fallback_base: &Path) -> DataLocation {
    if !exe_dir.join(PORTABLE_MARKER).is_file() {
        return DataLocation {
            mode: Mode::Installed,
            dir: None,
        };
    }
    let beside = exe_dir.join("BoilerSteamXL-data");
    if writable(&beside) {
        return DataLocation {
            mode: Mode::Portable,
            dir: Some(beside),
        };
    }
    DataLocation {
        mode: Mode::PortableFallback,
        dir: Some(fallback_base.join("BoilerSteamXL-portable")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_switches_to_folder_beside_exe() {
        let base = std::env::temp_dir().join(format!("bsxl-portable-{}", std::process::id()));
        let exe_dir = base.join("app");
        std::fs::create_dir_all(&exe_dir).ok();
        assert_eq!(resolve(&exe_dir, &base).mode, Mode::Installed);
        std::fs::write(exe_dir.join(PORTABLE_MARKER), b"").ok();
        let location = resolve(&exe_dir, &base);
        assert_eq!(location.mode, Mode::Portable);
        assert_eq!(location.dir, Some(exe_dir.join("BoilerSteamXL-data")));
        let _ = std::fs::remove_dir_all(&base);
    }
}
