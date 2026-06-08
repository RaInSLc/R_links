mod logic;
mod models;
mod search;
mod storage;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, State};

use models::{GenerateOptions, HistoryRecord, SearchResponse, SearchResult, Settings};

pub struct SearchState {
    running: AtomicBool,
    cancelled: AtomicBool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        }
    }
}

#[tauri::command]
fn generate_script(
    input: String,
    options: GenerateOptions,
    results: Vec<SearchResult>,
) -> Result<String, String> {
    logic::generate_script(&input, &options, &results)
}

#[tauri::command]
fn clean_script(script: String) -> String {
    script
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[tauri::command]
fn build_history_records(script: String) -> Vec<HistoryRecord> {
    logic::build_history_records(&script)
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<Settings, String> {
    storage::load_settings(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    storage::save_settings(&app, &settings)
}

#[tauri::command]
fn load_history(app: AppHandle) -> Result<Vec<HistoryRecord>, String> {
    storage::load_history(&app)
}

#[tauri::command]
fn save_history(app: AppHandle, history: Vec<HistoryRecord>) -> Result<(), String> {
    storage::save_history(&app, &history)
}

#[tauri::command]
fn stop_search(state: State<'_, SearchState>) -> bool {
    state.cancelled.store(true, Ordering::SeqCst);
    state.running.load(Ordering::SeqCst)
}

#[tauri::command]
async fn start_search(
    app: AppHandle,
    state: State<'_, SearchState>,
    input: String,
    settings: Settings,
) -> Result<SearchResponse, String> {
    if state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有检索任务正在运行".to_string());
    }

    state.cancelled.store(false, Ordering::SeqCst);
    let result = search::search_packages(&app, &state.cancelled, &input, &settings).await;
    state.running.store(false, Ordering::SeqCst);
    result
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(SearchState::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(data_dir)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            generate_script,
            clean_script,
            build_history_records,
            load_settings,
            save_settings,
            load_history,
            save_history,
            start_search,
            stop_search
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
