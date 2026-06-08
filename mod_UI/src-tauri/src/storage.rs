use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use crate::models::{
    HistoryRecord, Settings, MAX_FIELD_CHARS, MAX_HISTORY_COMMAND_CHARS, MAX_HISTORY_RECORDS,
};

fn data_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join(name))
}

pub fn load_settings(app: &AppHandle) -> Result<Settings, String> {
    let path = data_file(app, "settings.json")?;
    if !path.exists() {
        return Ok(Settings::default());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    match serde_json::from_str::<Settings>(&content).and_then(|settings| {
        settings.normalized().map_err(|error| {
            serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
    }) {
        Ok(settings) => Ok(settings),
        Err(_) => {
            backup_corrupt_file(app, "settings.json", &content)?;
            Ok(Settings::default())
        }
    }
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = data_file(app, "settings.json")?;
    let settings = settings.normalized()?;
    let content = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    atomic_write(&path, &content)
}

pub fn load_history(app: &AppHandle) -> Result<Vec<HistoryRecord>, String> {
    let path = data_file(app, "history.json")?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    match serde_json::from_str::<Vec<HistoryRecord>>(&content) {
        Ok(history) => Ok(sanitize_history(history)),
        Err(_) => {
            backup_corrupt_file(app, "history.json", &content)?;
            Ok(Vec::new())
        }
    }
}

pub fn save_history(app: &AppHandle, history: &[HistoryRecord]) -> Result<(), String> {
    let path = data_file(app, "history.json")?;
    let limited = sanitize_history(history.to_vec());
    let content = serde_json::to_string_pretty(&limited).map_err(|error| error.to_string())?;
    atomic_write(&path, &content)
}

fn sanitize_history(history: Vec<HistoryRecord>) -> Vec<HistoryRecord> {
    history
        .into_iter()
        .filter(|record| {
            !record.command.trim().is_empty()
                && record.command.len() <= MAX_HISTORY_COMMAND_CHARS
                && clean_history_field(&record.package_name, MAX_FIELD_CHARS)
                && clean_history_field(&record.version, MAX_FIELD_CHARS)
                && clean_history_field(&record.tool_name, MAX_FIELD_CHARS)
                && clean_history_field(&record.created_at, MAX_FIELD_CHARS)
                && clean_history_field(&record.id, MAX_FIELD_CHARS)
                && !record.command.chars().any(is_forbidden_control)
        })
        .take(MAX_HISTORY_RECORDS)
        .collect()
}

fn clean_history_field(value: &str, limit: usize) -> bool {
    value.len() <= limit && !value.chars().any(char::is_control)
}

fn is_forbidden_control(character: char) -> bool {
    character.is_control() && !matches!(character, '\r' | '\n' | '\t')
}

fn atomic_write(path: &PathBuf, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    fs::write(&tmp, content).map_err(|error| error.to_string())?;
    if path.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(&tmp, path) {
            let _ = fs::rename(&backup, path);
            return Err(error.to_string());
        }
        let _ = fs::remove_file(&backup);
        Ok(())
    } else {
        fs::rename(&tmp, path).map_err(|error| error.to_string())
    }
}

fn backup_corrupt_file(app: &AppHandle, name: &str, content: &str) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = data_file(app, &format!("{name}.corrupt.{stamp}.bak"))?;
    fs::write(backup, content).map_err(|error| error.to_string())
}
