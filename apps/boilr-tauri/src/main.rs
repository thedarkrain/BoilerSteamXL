#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod artwork;
mod cli;
mod commands;
mod covers;

use std::collections::HashMap;

use boilr::platforms::{get_platforms, GamesPlatform};
use boilr_core::settings::{load_setting_sections, save_settings, Settings};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use toml::Value;

/// SteamGridDB settings live in BoilR's config file and are shared with the artwork tools.
#[tauri::command]
fn load_settings() -> Result<Settings, String> {
    Settings::new().map_err(|err| err.to_string())
}

#[tauri::command]
fn update_settings(update: SettingsUpdate) -> Result<Settings, String> {
    let mut settings = Settings::new().map_err(|err| err.to_string())?;
    if let Some(grid) = update.steamgrid_db {
        if let Some(value) = grid.enabled {
            settings.steamgrid_db.enabled = value;
        }
        if let Some(value) = grid.prefer_animated {
            settings.steamgrid_db.prefer_animated = value;
        }
        if let Some(value) = grid.allow_nsfw {
            settings.steamgrid_db.allow_nsfw = value;
        }
        if let Some(value) = grid.only_download_boilr_images {
            settings.steamgrid_db.only_download_boilr_images = value;
        }
        // An empty key clears it (JSON null would be indistinguishable from "not sent").
        if let Some(value) = grid.auth_key {
            settings.steamgrid_db.auth_key = value.filter(|k| !k.trim().is_empty());
        }
    }
    let platforms = get_platforms();
    save_settings(&settings, &collect_platform_sections(&platforms)).map_err(|err| err.to_string())?;
    Ok(settings)
}

/// The page where a signed-in SteamGridDB user finds their API key.
const STEAMGRIDDB_KEY_PAGE: &str = "https://www.steamgriddb.com/profile/preferences/api";

/// Opens the SteamGridDB API key page in the default browser (a fixed URL, no shell).
#[tauri::command]
fn open_steamgriddb_key_page() -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg(STEAMGRIDDB_KEY_PAGE).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// A SteamGridDB key already saved by BoilR or an installed BoilerSteamXL on this PC.
fn existing_steamgriddb_key() -> Option<String> {
    let appdata = std::path::PathBuf::from(std::env::var_os("APPDATA")?);
    let ours = boilr_core::config::get_config_file();
    ["BoilerSteamXL", "boilr"]
        .iter()
        .map(|dir| appdata.join(dir).join("config.toml"))
        .filter(|path| *path != ours)
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|text| text.parse::<Value>().ok())
        .filter_map(|toml| toml.get("steamgrid_db")?.get("auth_key")?.as_str().map(|k| k.trim().to_string()))
        .find(|key| !key.is_empty())
}

/// Without a key of its own (e.g. a fresh portable copy), reuse one found on this PC.
fn adopt_existing_steamgriddb_key() {
    if bsxl_engine::prefs::Preferences::load().steamgriddb_key_removed {
        return;
    }
    let Ok(settings) = Settings::new() else { return };
    if settings.steamgrid_db.auth_key.as_deref().is_some_and(|k| !k.trim().is_empty()) {
        return;
    }
    if let Some(key) = existing_steamgriddb_key() {
        let update = SettingsUpdate { steamgrid_db: Some(SteamGridDbUpdate { enabled: Some(true), prefer_animated: None, allow_nsfw: None, only_download_boilr_images: None, auth_key: Some(Some(key)) }) };
        if update_settings(update).is_ok() {
            bsxl_engine::log::info("artwork", "reused the SteamGridDB key found in another BoilR/BoilerSteamXL config");
        }
    }
}

#[tauri::command]
fn list_platform_settings() -> Result<Vec<PlatformSettingsPayload>, String> {
    get_platforms()
        .into_iter()
        .filter(|p| p.code_name() != "standalone")
        .map(|platform| platform_settings_payload(platform.as_ref()))
        .collect()
}

#[tauri::command]
fn update_platform_settings(code_name: String, settings: JsonValue) -> Result<PlatformSettingsPayload, String> {
    let table = json_to_toml_table(settings)?;
    persist_platform_table(&code_name, table)?;
    let platform = get_platforms()
        .into_iter()
        .find(|platform| platform.code_name() == code_name)
        .ok_or_else(|| format!("Platform {code_name} not found after updating settings"))?;
    platform_settings_payload(platform.as_ref())
}

#[tauri::command]
fn update_platform_enabled(code_name: String, enabled: bool) -> Result<(), String> {
    let mut table = load_platform_table(&code_name)?;
    table.insert("enabled".to_string(), Value::Boolean(enabled));
    persist_platform_table(&code_name, table)
}

macro_rules! handlers {
    () => {
        tauri::generate_handler![
            load_settings,
            update_settings,
            open_steamgriddb_key_page,
            covers::check_steamgriddb_key,
            covers::artwork_thumb,
            list_platform_settings,
            update_platform_settings,
            update_platform_enabled,
            artwork::list_steam_accounts,
            artwork::list_artwork,
            artwork::artwork_options,
            artwork::set_artwork,
            artwork::clear_artwork,
            artwork::artwork_game_match,
            artwork::set_artwork_game,
            artwork::find_missing_artwork,
            commands::app_info,
            commands::get_prefs,
            commands::save_prefs,
            commands::steam_info,
            commands::check_steam_path,
            commands::suggest_scan_roots,
            commands::get_library,
            commands::refresh_library,
            commands::cancel_refresh,
            commands::approve,
            commands::approve_eligible,
            commands::reject,
            commands::ignore,
            commands::restore_entries,
            commands::edit_entry,
            commands::reset_override,
            commands::link_entry,
            commands::list_shortcuts,
            commands::steam_games,
            commands::set_removal,
            commands::check_manual,
            commands::add_manual,
            commands::remove_manual,
            commands::analyze_folder,
            commands::plan_import,
            commands::execute_import,
            commands::close_steam,
            commands::start_steam,
            commands::preview_rule,
            commands::validate_rule,
            commands::recommended_rules,
            commands::export_json,
            commands::import_json,
            commands::rebuild_start,
            commands::rebuild_draft,
            commands::rebuild_inspect,
            commands::rebuild_set_decision,
            commands::rebuild_approve,
            commands::rebuild_plan,
            commands::rebuild_execute,
            commands::rebuild_cancel,
            commands::list_backups,
            commands::restore_preview,
            commands::restore_backup,
            commands::export_backup,
            commands::get_logs,
            commands::diagnostics_report,
            commands::schedule_preview,
            commands::reveal,
            commands::open_data_folder,
            commands::launch_in_steam,
            commands::check_update,
            commands::inspect_account,
            covers::game_art,
            covers::fetch_artwork,
            covers::search_artwork,
            covers::set_artwork_match,
        ]
    };
}

fn main() {
    // Portable edition: data next to the executable (or a fallback when read-only).
    let mode = cli::setup_data_dir();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if cli::is_cli(&args) {
        std::process::exit(cli::run(&args, &mode));
    }

    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DMABUF_RENDERER_FORCE_SHM").is_none() {
        std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
    }
    bsxl_engine::log::info("app", &format!("BoilerSteamXL {} started ({mode})", bsxl_engine::VERSION));
    adopt_existing_steamgriddb_key();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState { mode, ..Default::default() })
        .invoke_handler(handlers!())
        .run(tauri::generate_context!());
    if let Err(e) = result {
        bsxl_engine::log::error("app", &format!("could not start the window: {e}"));
    }
}

fn collect_platform_sections(platforms: &[Box<dyn GamesPlatform>]) -> Vec<(String, String)> {
    platforms
        .iter()
        .map(|platform| (platform.code_name().to_string(), platform.get_settings_serializable()))
        .collect()
}

fn load_platform_table(code_name: &str) -> Result<toml::value::Table, String> {
    let sections = load_setting_sections().unwrap_or_default();
    let default_table = default_settings_table(code_name);
    let mut table: toml::value::Table = match sections.get(code_name) {
        Some(serialized) if !serialized.trim().is_empty() => {
            toml::from_str(serialized).unwrap_or_else(|_| default_table.clone().unwrap_or_default())
        }
        _ => default_table.clone().unwrap_or_default(),
    };
    if table.is_empty() {
        if let Some(defaults) = default_table {
            table = defaults;
        }
    }
    Ok(table)
}

fn persist_platform_table(code_name: &str, table: toml::value::Table) -> Result<(), String> {
    let settings = Settings::new().map_err(|err| err.to_string())?;
    let mut sections: HashMap<String, String> = load_setting_sections().unwrap_or_default();
    let serialized = toml::to_string(&table).map_err(|err| format!("Could not serialise settings for {code_name}: {err}"))?;
    sections.insert(code_name.to_string(), serialized);
    let mut platform_sections: Vec<(String, String)> = sections.into_iter().collect();
    platform_sections.sort_by(|a, b| a.0.cmp(&b.0));
    save_settings(&settings, &platform_sections).map_err(|err| err.to_string())
}

fn json_to_toml_table(settings: JsonValue) -> Result<toml::value::Table, String> {
    match toml::Value::try_from(settings).map_err(|err| format!("Could not convert settings payload to TOML: {err}"))? {
        toml::Value::Table(table) => Ok(table),
        other => Err(format!("Platform settings payload must be a table, found {other:?}")),
    }
}

fn platform_settings_payload(platform: &dyn GamesPlatform) -> Result<PlatformSettingsPayload, String> {
    let table = match toml::from_str::<toml::Value>(&platform.get_settings_serializable()) {
        Ok(toml::Value::Table(table)) => table,
        _ => toml::value::Table::new(),
    };
    let json = serde_json::to_value(&table).map_err(|err| format!("Could not serialise settings for {}: {err}", platform.code_name()))?;
    Ok(PlatformSettingsPayload {
        code_name: platform.code_name().to_string(),
        name: platform.name().to_string(),
        settings: json,
    })
}

fn default_settings_table(code_name: &str) -> Option<toml::value::Table> {
    let defaults = get_platforms()
        .into_iter()
        .find(|platform| platform.code_name() == code_name)?
        .get_settings_serializable();
    toml::from_str(&defaults).ok()
}

#[derive(Debug, Deserialize)]
struct SettingsUpdate {
    #[serde(rename = "steamgrid_db")]
    steamgrid_db: Option<SteamGridDbUpdate>,
}

#[derive(Debug, Deserialize)]
struct SteamGridDbUpdate {
    enabled: Option<bool>,
    prefer_animated: Option<bool>,
    allow_nsfw: Option<bool>,
    only_download_boilr_images: Option<bool>,
    auth_key: Option<Option<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlatformSettingsPayload {
    code_name: String,
    name: String,
    settings: JsonValue,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::{
        ipc::CallbackFn,
        test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY},
        webview::InvokeRequest,
        WebviewWindowBuilder,
    };

    fn test_home() {
        let dir = std::env::temp_dir().join(format!("bsxl-tauri-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp home");
        std::env::set_var("BOILERSTEAMXL_HOME", &dir);
    }

    fn invoke(cmd: &str) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
        test_home();
        let app = mock_builder()
            .manage(commands::AppState::default())
            .invoke_handler(handlers!())
            .build(mock_context(noop_assets()))
            .expect("failed to build tauri app for tests");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().expect("webview");
        get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "http://tauri.localhost".parse().expect("url"),
                body: Default::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
    }

    #[test]
    fn the_bulk_sync_commands_of_boilr_are_not_exposed() {
        // These imported everything without approval; the interface must not reach them.
        assert!(invoke("run_full_sync").is_err());
        assert!(invoke("plan_sync").is_err());
    }

    #[test]
    fn library_and_prefs_commands_answer() {
        assert!(invoke("get_library").is_ok());
        let prefs = invoke("get_prefs").expect("prefs");
        let prefs: bsxl_engine::prefs::Preferences = prefs.deserialize().expect("prefs payload");
        assert!(prefs.simulation_mode, "Simulation mode is on by default");
    }
}
