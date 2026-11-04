//! Structured application log: one JSON object per line in `logs/boilersteamxl.log`,
//! rotated at 2 MB. Never log settings or API keys.

use std::io::Write;

const MAX_BYTES: u64 = 2 * 1024 * 1024;

pub fn log_file() -> std::path::PathBuf {
    crate::data_dir().join("logs").join("boilersteamxl.log")
}

fn write(level: &str, event: &str, message: &str) {
    let path = log_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    let line = serde_json::json!({
        "ts": crate::now_rfc3339(),
        "level": level,
        "event": event,
        "message": message,
    });
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
    if level != "info" {
        eprintln!("[{level}] {event}: {message}");
    }
}

pub fn info(event: &str, message: &str) {
    write("info", event, message);
}

pub fn warn(event: &str, message: &str) {
    write("warn", event, message);
}

pub fn error(event: &str, message: &str) {
    write("error", event, message);
}

/// The last `limit` log records, newest first.
pub fn tail(limit: usize) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(log_file()).unwrap_or_default();
    text.lines()
        .rev()
        .filter_map(|l| serde_json::from_str(l).ok())
        .take(limit)
        .collect()
}
