use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use crate::models::{HistoryRecord, Settings, MAX_HISTORY_COMMAND_CHARS, MAX_HISTORY_RECORDS};

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
                && !record.command.chars().any(|character| {
                    character.is_control() && !matches!(character, '\r' | '\n' | '\t')
                })
        })
        .take(MAX_HISTORY_RECORDS)
        .collect()
}

fn atomic_write(path: &PathBuf, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|error| error.to_string())?;
    fs::rename(&tmp, path).map_err(|error| error.to_string())
}

fn backup_corrupt_file(app: &AppHandle, name: &str, content: &str) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = data_file(app, &format!("{name}.corrupt.{stamp}.bak"))?;
    fs::write(backup, content).map_err(|error| error.to_string())
}
