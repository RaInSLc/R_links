use tauri::AppHandle;
use crate::{models::HistoryRecord, storage};
#[tauri::command] pub(crate) fn load_history(app: AppHandle) -> Result<Vec<HistoryRecord>, String> { storage::load_history(&app) }
#[tauri::command] pub(crate) fn save_history(app: AppHandle, history: Vec<HistoryRecord>) -> Result<Vec<HistoryRecord>, String> { storage::save_history(&app, &history) }
