mod logic;
mod models;
mod search;
mod secrets;
mod storage;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard,
};
use tauri::{AppHandle, Manager, State};

use models::{
    GenerateOptions, HistoryRecord, PublicSettings, SearchResponse, SearchResult, Settings,
};

pub struct SearchState {
    inner: Mutex<SearchStateInner>,
}

struct SearchStateInner {
    running: bool,
    cancellation: Arc<AtomicBool>,
}

struct SearchRunGuard<'a> {
    state: &'a SearchState,
    cancellation: Arc<AtomicBool>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(SearchStateInner {
                running: false,
                cancellation: Arc::new(AtomicBool::new(false)),
            }),
        }
    }
}

impl SearchState {
    fn try_begin(&self) -> Result<SearchRunGuard<'_>, String> {
        let mut inner = self.lock_inner();
        if inner.running {
            return Err("已有检索任务正在运行".to_string());
        }

        let cancellation = Arc::new(AtomicBool::new(false));
        inner.running = true;
        inner.cancellation = Arc::clone(&cancellation);
        Ok(SearchRunGuard {
            state: self,
            cancellation,
        })
    }

    fn request_stop(&self) -> bool {
        let inner = self.lock_inner();
        if !inner.running {
            return false;
        }

        inner.cancellation.store(true, Ordering::SeqCst);
        true
    }

    fn lock_inner(&self) -> MutexGuard<'_, SearchStateInner> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    #[cfg(test)]
    fn is_running_for_test(&self) -> bool {
        self.lock_inner().running
    }

    #[cfg(test)]
    fn is_cancelled_for_test(&self) -> bool {
        self.lock_inner().cancellation.load(Ordering::SeqCst)
    }
}

impl SearchRunGuard<'_> {
    fn cancelled(&self) -> &AtomicBool {
        self.cancellation.as_ref()
    }
}

impl Drop for SearchRunGuard<'_> {
    fn drop(&mut self) {
        self.cancellation.store(false, Ordering::SeqCst);
        let mut inner = self.state.lock_inner();
        if Arc::ptr_eq(&inner.cancellation, &self.cancellation) {
            inner.running = false;
            inner.cancellation = Arc::new(AtomicBool::new(false));
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
fn clean_script(script: String) -> Result<String, String> {
    logic::validate_script_size(&script)?;
    Ok(script
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\r\n"))
}

#[tauri::command]
fn build_history_records(script: String) -> Result<Vec<HistoryRecord>, String> {
    logic::validate_script_size(&script)?;
    Ok(logic::build_history_records(&script))
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<PublicSettings, String> {
    storage::load_settings(&app).map(|settings| settings.public_view())
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let existing = storage::load_existing_settings(&app)?.unwrap_or_default();
    let settings = merge_runtime_settings(settings, &existing)?;
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
fn open_package_search(app: AppHandle, package_name: String) -> Result<(), String> {
    let package_name = package_name.trim();
    if !logic::is_valid_package_name(package_name) || package_name.contains('/') {
        return Err(format!("无效包名，无法打开浏览器搜索: {package_name}"));
    }
    let url = format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(&format!("R package {package_name}"))
    );
    if !logic::is_allowed_browser_search_url(&url) {
        return Err("浏览器搜索 URL 不在允许范围内".to_string());
    }
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(url, None::<&str>)
        .map_err(|error| format!("打开浏览器失败: {error}"))
}

#[tauri::command]
fn stop_search(state: State<'_, SearchState>) -> bool {
    state.request_stop()
}

#[tauri::command]
async fn start_search(
    app: AppHandle,
    state: State<'_, SearchState>,
    run_id: u64,
    input: String,
    settings: Settings,
) -> Result<SearchResponse, String> {
    logic::validate_input_size(&input)?;
    let existing = storage::load_existing_settings(&app)?.unwrap_or_default();
    let settings = merge_runtime_settings(settings, &existing)?;
    let run = state.try_begin()?;
    let result = search::search_packages(&app, run_id, run.cancelled(), &input, &settings).await;
    drop(run);
    result
}

fn merge_runtime_settings(incoming: Settings, existing: &Settings) -> Result<Settings, String> {
    incoming.merged_with_existing_token(existing)
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
            open_package_search,
            start_search,
            stop_search
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_state_rejects_overlapping_runs() {
        let state = SearchState::default();
        let run = state.try_begin().expect("应允许启动首个检索任务");

        assert!(state.is_running_for_test());
        assert!(!run.cancelled().load(Ordering::SeqCst));
        assert!(state.try_begin().is_err());

        drop(run);
        assert!(!state.is_running_for_test());
        assert!(!state.is_cancelled_for_test());
    }

    #[test]
    fn search_state_clears_cancel_flag_after_run_drop() {
        let state = SearchState::default();
        let run = state.try_begin().expect("应允许启动检索任务");

        assert!(state.request_stop());
        assert!(run.cancelled().load(Ordering::SeqCst));

        drop(run);
        assert!(!state.is_running_for_test());
        assert!(!state.is_cancelled_for_test());
    }

    #[test]
    fn search_state_ignores_idle_stop_requests() {
        let state = SearchState::default();

        assert!(!state.request_stop());
        assert!(!state.is_running_for_test());
        assert!(!state.is_cancelled_for_test());
    }

    #[test]
    fn runtime_settings_preserve_saved_token_when_incoming_token_empty() {
        let existing = Settings {
            github_token: "ghp_saved".to_string(),
            ..Settings::default()
        };
        let incoming = Settings {
            github_token: String::new(),
            ..Settings::default()
        };

        let merged = merge_runtime_settings(incoming, &existing).expect("设置应可合并");

        assert_eq!(merged.github_token, "ghp_saved");
    }

    #[test]
    fn runtime_settings_allow_explicit_token_replacement() {
        let existing = Settings {
            github_token: "ghp_saved".to_string(),
            ..Settings::default()
        };
        let incoming = Settings {
            github_token: "ghp_new".to_string(),
            ..Settings::default()
        };

        let merged = merge_runtime_settings(incoming, &existing).expect("设置应可合并");

        assert_eq!(merged.github_token, "ghp_new");
    }
}
