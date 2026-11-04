//! User preferences, stored in `preferences.json` with a schema version for migrations.

use serde::{Deserialize, Serialize};

use crate::{atomic_write, data_dir, read_json, rules::RuleSet, scan::ScanRoot, schedule::Schedule};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub schema_version: u32,
    pub first_run_complete: bool,
    /// The Steam *installation* folder; `None` detects it.
    pub steam_path: Option<String>,
    pub scan_roots: Vec<ScanRoot>,
    pub rules: RuleSet,
    pub schedule: Schedule,
    /// While on (the default), imports and rebuilds are computed and reported in full but
    /// nothing is written to Steam. Writing must be switched on deliberately in Settings.
    pub simulation_mode: bool,
    /// Steam account ids to write to; empty means every account found.
    pub target_users: Vec<String>,
    /// Also queue games from BoilR's launcher integrations (Epic, GOG Galaxy, ...).
    pub include_launchers: bool,
    pub notifications: bool,
    pub theme: String,
    pub download_artwork: bool,
    pub check_updates: bool,
    pub update_url: String,
    /// The user removed their SteamGridDB key: never adopt one from another config again.
    pub steamgriddb_key_removed: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            schema_version: SCHEMA_VERSION,
            first_run_complete: false,
            steam_path: None,
            scan_roots: Vec::new(),
            rules: RuleSet::recommended(),
            schedule: Schedule::default(),
            simulation_mode: true,
            target_users: Vec::new(),
            include_launchers: false,
            notifications: true,
            theme: "dark".into(),
            download_artwork: true,
            check_updates: false,
            update_url: String::new(),
            steamgriddb_key_removed: false,
        }
    }
}

pub fn prefs_file() -> std::path::PathBuf {
    data_dir().join("preferences.json")
}

impl Preferences {
    pub fn load() -> Preferences {
        let mut prefs: Preferences = read_json(&prefs_file()).unwrap_or_default();
        prefs.migrate();
        prefs
    }

    pub fn save(&self) -> eyre::Result<()> {
        atomic_write(&prefs_file(), serde_json::to_string_pretty(self)?.as_bytes())?;
        Ok(())
    }

    /// Brings older files up to date. Rules added in a newer version are appended; rules
    /// the user changed or disabled are left alone.
    pub fn migrate(&mut self) {
        if self.schema_version < SCHEMA_VERSION {
            self.schema_version = SCHEMA_VERSION;
        }
        self.rules.merge_new_builtins();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_mode_is_on_by_default() {
        assert!(Preferences::default().simulation_mode);
        let parsed: Preferences = serde_json::from_str("{}").unwrap_or_default();
        assert!(parsed.simulation_mode);
    }

    #[test]
    fn migration_restores_missing_builtin_rules_but_keeps_user_edits() {
        let mut prefs = Preferences::default();
        prefs.rules.rules.retain(|r| r.id != "f-quicksfv");
        if let Some(rule) = prefs.rules.rules.iter_mut().find(|r| r.id == "f-setup") {
            rule.enabled = false;
        }
        prefs.migrate();
        assert!(prefs.rules.rules.iter().any(|r| r.id == "f-quicksfv"));
        assert!(prefs.rules.rules.iter().any(|r| r.id == "f-setup" && !r.enabled));
    }
}
