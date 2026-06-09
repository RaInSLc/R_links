mod logic;
mod models;
mod search;
mod secrets;
mod storage;

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

use models::{
    GenerateOptions, HistoryRecord, PublicSettings, SearchResponse, SearchResult, Settings,
};

const MAX_BROWSER_OPEN_REQUESTS: usize = 30;
const BROWSER_OPEN_WINDOW: Duration = Duration::from_secs(60);

pub struct SearchState {
    inner: Mutex<SearchStateInner>,
}

struct SearchStateInner {
    running: bool,
    run_id: u64,
    cancellation: Arc<AtomicBool>,
}

struct SearchRunGuard<'a> {
    state: &'a SearchState,
    run_id: u64,
    cancellation: Arc<AtomicBool>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(SearchStateInner {
                running: false,
                run_id: 0,
                cancellation: Arc::new(AtomicBool::new(false)),
            }),
        }
    }
}

impl SearchState {
    fn try_begin(&self, run_id: u64) -> Result<SearchRunGuard<'_>, String> {
        if run_id == 0 {
            return Err("检索任务 ID 无效".to_string());
        }
        let mut inner = self.lock_inner();
        if inner.running {
            return Err("已有检索任务正在运行".to_string());
        }

        let cancellation = Arc::new(AtomicBool::new(false));
        inner.running = true;
        inner.run_id = run_id;
        inner.cancellation = Arc::clone(&cancellation);
        Ok(SearchRunGuard {
            state: self,
            run_id,
            cancellation,
        })
    }

    fn request_stop(&self, run_id: u64) -> bool {
        let inner = self.lock_inner();
        if !inner.running || inner.run_id != run_id {
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

    #[cfg(test)]
    fn run_id_for_test(&self) -> u64 {
        self.lock_inner().run_id
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
        if inner.run_id == self.run_id && Arc::ptr_eq(&inner.cancellation, &self.cancellation) {
            inner.running = false;
            inner.run_id = 0;
            inner.cancellation = Arc::new(AtomicBool::new(false));
        }
    }
}

pub struct BrowserOpenLimiter {
    opened_at: Mutex<VecDeque<Instant>>,
}

impl Default for BrowserOpenLimiter {
    fn default() -> Self {
        Self {
            opened_at: Mutex::new(VecDeque::new()),
        }
    }
}

impl BrowserOpenLimiter {
    fn try_acquire(&self, now: Instant) -> Result<(), String> {
        let mut opened_at = self
            .opened_at
            .lock()
            .map_err(|_| "浏览器打开限流状态已损坏".to_string())?;
        while opened_at.front().is_some_and(|opened| {
            now.checked_duration_since(*opened)
                .is_some_and(|elapsed| elapsed >= BROWSER_OPEN_WINDOW)
        }) {
            opened_at.pop_front();
        }
        if opened_at.len() >= MAX_BROWSER_OPEN_REQUESTS {
            return Err(format!(
                "浏览器搜索打开过于频繁，请稍后再试；每 {} 秒最多允许 {} 次",
                BROWSER_OPEN_WINDOW.as_secs(),
                MAX_BROWSER_OPEN_REQUESTS
            ));
        }
        opened_at.push_back(now);
        Ok(())
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
    logic::clean_script(&script)
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
fn save_settings(app: AppHandle, settings: Settings) -> Result<PublicSettings, String> {
    let existing = storage::load_existing_settings(&app)?.unwrap_or_default();
    let settings = merge_runtime_settings(settings, &existing)?;
    storage::save_settings(&app, &settings)?;
    Ok(settings.public_view())
}

#[tauri::command]
fn clear_github_token(app: AppHandle) -> Result<PublicSettings, String> {
    let mut settings = storage::load_existing_settings(&app)?.unwrap_or_default();
    settings.github_token.clear();
    let settings = settings.normalized()?;
    storage::save_settings(&app, &settings)?;
    Ok(settings.public_view())
}

#[tauri::command]
fn load_history(app: AppHandle) -> Result<Vec<HistoryRecord>, String> {
    storage::load_history(&app)
}

#[tauri::command]
fn save_history(app: AppHandle, history: Vec<HistoryRecord>) -> Result<Vec<HistoryRecord>, String> {
    storage::save_history(&app, &history)
}

#[tauri::command]
fn open_package_search(
    app: AppHandle,
    limiter: State<'_, BrowserOpenLimiter>,
    package_name: String,
) -> Result<(), String> {
    let url = browser_search_url_for_package(&package_name)?;
    limiter.try_acquire(Instant::now())?;
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(url, None::<&str>)
        .map_err(|error| format!("打开浏览器失败: {error}"))
}

fn browser_search_url_for_package(package_name: &str) -> Result<String, String> {
    let package_name = package_name.trim();
    if !logic::is_valid_package_name(package_name) || package_name.contains('/') {
        return Err("无效包名，无法打开浏览器搜索".to_string());
    }
    let url = format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(&format!("R package {package_name}"))
    );
    if !logic::is_allowed_browser_search_url(&url) {
        return Err("浏览器搜索 URL 不在允许范围内".to_string());
    }
    Ok(url)
}

#[tauri::command]
fn stop_search(state: State<'_, SearchState>, run_id: u64) -> bool {
    state.request_stop(run_id)
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
    let run = state.try_begin(run_id)?;
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
        .manage(BrowserOpenLimiter::default())
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
            clear_github_token,
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
        let run = state.try_begin(10).expect("应允许启动首个检索任务");

        assert!(state.is_running_for_test());
        assert_eq!(state.run_id_for_test(), 10);
        assert!(!run.cancelled().load(Ordering::SeqCst));
        assert!(state.try_begin(11).is_err());

        drop(run);
        assert!(!state.is_running_for_test());
        assert_eq!(state.run_id_for_test(), 0);
        assert!(!state.is_cancelled_for_test());
    }

    #[test]
    fn search_state_clears_cancel_flag_after_run_drop() {
        let state = SearchState::default();
        let run = state.try_begin(20).expect("应允许启动检索任务");

        assert!(state.request_stop(20));
        assert!(run.cancelled().load(Ordering::SeqCst));

        drop(run);
        assert!(!state.is_running_for_test());
        assert!(!state.is_cancelled_for_test());
    }

    #[test]
    fn search_state_ignores_idle_stop_requests() {
        let state = SearchState::default();

        assert!(!state.request_stop(30));
        assert!(!state.is_running_for_test());
        assert!(!state.is_cancelled_for_test());
    }

    #[test]
    fn search_state_ignores_stale_stop_requests() {
        let state = SearchState::default();
        let run = state.try_begin(40).expect("应允许启动检索任务");

        assert!(!state.request_stop(39));
        assert!(!run.cancelled().load(Ordering::SeqCst));
        assert!(state.request_stop(40));
        assert!(run.cancelled().load(Ordering::SeqCst));
    }

    #[test]
    fn search_state_rejects_zero_run_id() {
        let state = SearchState::default();

        assert!(state.try_begin(0).is_err());
        assert!(!state.is_running_for_test());
    }

    #[test]
    fn browser_open_limiter_enforces_window_limit() {
        let limiter = BrowserOpenLimiter::default();
        let now = Instant::now();

        for _ in 0..MAX_BROWSER_OPEN_REQUESTS {
            assert!(limiter.try_acquire(now).is_ok());
        }
        assert!(limiter.try_acquire(now).is_err());
        assert!(limiter.try_acquire(now + BROWSER_OPEN_WINDOW).is_ok());
    }

    #[test]
    fn browser_search_url_rejects_invalid_package_without_echoing_value() {
        let package_name = format!("bad/{}", "x".repeat(4096));
        let error = browser_search_url_for_package(&package_name).expect_err("非法包名应被拒绝");

        assert_eq!(error, "无效包名，无法打开浏览器搜索");
        assert!(!error.contains(&package_name));
    }

    #[test]
    fn browser_search_url_encodes_valid_package() {
        let url = browser_search_url_for_package("GSVA").expect("合法包名应可生成搜索 URL");

        assert_eq!(url, "https://www.google.com/search?q=R%20package%20GSVA");
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

    #[test]
    fn runtime_settings_public_view_reflects_normalized_values() {
        let existing = Settings::default();
        let incoming = Settings {
            proxy: "127.0.0.1:7890".to_string(),
            github_token: "ghp_new".to_string(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: true,
        };

        let public = merge_runtime_settings(incoming, &existing)
            .expect("设置应可规范化")
            .public_view();

        assert_eq!(public.proxy, "http://127.0.0.1:7890");
        assert_eq!(public.cran_mirror, "https://cloud.r-project.org/");
        assert!(public.full_search);
        assert!(public.github_token_configured);
    }

    #[test]
    fn clearing_token_preserves_other_settings() {
        let mut settings = Settings {
            proxy: "127.0.0.1:7890".to_string(),
            github_token: "ghp_saved".to_string(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: true,
        }
        .normalized()
        .expect("设置应可规范化");

        settings.github_token.clear();
        let public = settings.public_view();

        assert_eq!(settings.proxy, "http://127.0.0.1:7890");
        assert_eq!(settings.cran_mirror, "https://cloud.r-project.org/");
        assert!(settings.full_search);
        assert!(!public.github_token_configured);
    }
}
