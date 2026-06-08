use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::models::{HistoryRecord, Settings};

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
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = data_file(app, "settings.json")?;
    let content = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

pub fn load_history(app: &AppHandle) -> Result<Vec<HistoryRecord>, String> {
    let path = data_file(app, "history.json")?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

pub fn save_history(app: &AppHandle, history: &[HistoryRecord]) -> Result<(), String> {
    let path = data_file(app, "history.json")?;
    let limited = history.iter().take(100).collect::<Vec<_>>();
    let content = serde_json::to_string_pretty(&limited).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}
