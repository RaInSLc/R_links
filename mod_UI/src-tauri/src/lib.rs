mod logic;
mod models;
mod search;
mod search_sanitize;
mod search_urls;
mod secrets;
mod storage;

use regex::Regex;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard, OnceLock,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};

use models::{
    GenerateOptions, HistoryRecord, InputRules, PublicSettings, SearchResponse, SearchResult,
    Settings,
};

const MAX_BROWSER_OPEN_REQUESTS: usize = 30;
const BROWSER_OPEN_WINDOW: Duration = Duration::from_secs(60);
const MAX_JS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
static SETTINGS_UPDATE_LOCK: Mutex<()> = Mutex::new(());
static HISTORY_EXTRACT_RE: OnceLock<Regex> = OnceLock::new();

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
        if run_id == 0 || run_id > MAX_JS_SAFE_INTEGER {
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
fn load_input_rules(app: AppHandle) -> Result<InputRules, String> {
    let path = storage::data_file(&app, "input_rules.json")
        .map_err(|e| format!("获取输入规则文件路径失败: {e}"))?;
    if !path.exists() {
        return Ok(InputRules::default());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("读取输入规则文件失败: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析输入规则失败: {e}"))
}

#[tauri::command]
fn save_input_rules(app: AppHandle, rules: InputRules) -> Result<(), String> {
    let path = storage::data_file(&app, "input_rules.json")
        .map_err(|e| format!("获取输入规则文件路径失败: {e}"))?;
    let content = serde_json::to_string_pretty(&rules)
        .map_err(|e| format!("序列化输入规则失败: {e}"))?;
    std::fs::write(&path, &content).map_err(|e| format!("写入输入规则文件失败: {e}"))
}

#[tauri::command]
fn generate_script(
    app: tauri::AppHandle,
    input: String,
    options: GenerateOptions,
    mut results: Vec<SearchResult>,
    show_remote_version: Option<bool>,
) -> Result<String, String> {
    let rules = storage::load_input_rules(&app);
    if results.is_empty() {
        let mut offline_results = Vec::new();
        if let Ok(packages) = logic::parse_inputs_filtered(&input, &rules) {
            let cache = storage::load_cache(&app).unwrap_or_default();
            let history = storage::load_history(&app).unwrap_or_default();

            for pkg in packages {
                let pkg_lower = pkg.name.to_ascii_lowercase();

                if let Some(entry) = cache.get(&pkg_lower) {
                    offline_results.push(SearchResult {
                        package: pkg.name.clone(),
                        requested_version: pkg.version.clone(),
                        latest_version: entry.version.clone(),
                        repository: entry.repository.clone(),
                        real_name: entry.real_name.clone(),
                        source: entry.source.clone(),
                        found: true,
                        message: "离线缓存命中".to_string(),
                        status: "found".to_string(),
                    });
                    continue;
                }

                if let Some(record) = history
                    .iter()
                    .find(|r| r.package_name.eq_ignore_ascii_case(&pkg.name))
                {
                    let source = match record.tool_name.as_str() {
                        "Bioconductor" => "bioc",
                        "GitHub" => "github",
                        "CRAN" | "base R" => "cran",
                        _ => "cran",
                    };

                    let mut repository = String::new();
                    if source == "github" {
                        #[allow(clippy::regex_creation_in_loops)]
                        let re = HISTORY_EXTRACT_RE.get_or_init(|| {
                            Regex::new(r#"(?:install_github|install_url)\("([^"]+)""#)
                                .expect("固定历史提取正则必须有效")
                        });
                        if let Some(caps) = re.captures(&record.command) {
                            if let Some(m) = caps.get(1) {
                                let val = m.as_str();
                                if val.contains('/') && !val.starts_with("http") {
                                    repository = val.to_string();
                                }
                            }
                        }
                    }

                    offline_results.push(SearchResult {
                        package: pkg.name.clone(),
                        requested_version: pkg.version.clone(),
                        latest_version: record.version.clone(),
                        repository,
                        real_name: record.package_name.clone(),
                        source: source.to_string(),
                        found: true,
                        message: "历史记录命中".to_string(),
                        status: "found".to_string(),
                    });
                }
            }
        }
        results = offline_results;
    }

    if show_remote_version == Some(false) {
        logic::generate_script_with_rules(&input, &options, &results, false, &rules)
    } else {
        logic::generate_script_with_rules(&input, &options, &results, true, &rules)
    }
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
    let _guard = lock_settings_update()?;
    let existing = load_existing_settings_for_runtime(&app)?;
    let settings = merge_runtime_settings(settings, &existing)?;
    storage::save_settings(&app, &settings)?;
    Ok(settings.public_view())
}

#[tauri::command]
fn clear_github_token(app: AppHandle) -> Result<PublicSettings, String> {
    let _guard = lock_settings_update()?;
    let settings = clear_github_token_settings(load_existing_settings_for_runtime(&app)?)?;
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
fn clear_package_cache(app: AppHandle) -> Result<(), String> {
    storage::clear_cache(&app)
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
fn export_diagnostics(app: AppHandle) -> Result<String, String> {
    let settings = load_existing_settings_for_runtime(&app)?;
    let public_settings = settings.public_view();

    let cache_count = storage::load_cache(&app)
        .map(|cache| cache.len())
        .unwrap_or(0);

    let history_count = storage::load_history(&app)
        .map(|history| history.len())
        .unwrap_or(0);

    let diagnostics = serde_json::json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "settings": {
            "full_search": public_settings.full_search,
            "proxy": public_settings.proxy,
            "cran_mirror": public_settings.cran_mirror,
            "github_token_configured": public_settings.github_token_configured,
        },
        "cache_entries": cache_count,
        "history_entries": history_count,
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });

    serde_json::to_string_pretty(&diagnostics).map_err(|e| format!("诊断信息序列化失败: {e}"))
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
    let run = state.try_begin(run_id)?;
    let existing = load_existing_settings_for_runtime(&app)?;
    let settings = merge_runtime_settings(settings, &existing)?;
    let result = search::search_packages(&app, run_id, run.cancelled(), &input, &settings).await;
    drop(run);
    result
}

fn merge_runtime_settings(incoming: Settings, existing: &Settings) -> Result<Settings, String> {
    incoming.merged_with_existing_token(existing)
}

fn lock_settings_update() -> Result<MutexGuard<'static, ()>, String> {
    SETTINGS_UPDATE_LOCK
        .lock()
        .map_err(|_| "设置更新锁已损坏".to_string())
}

fn load_existing_settings_for_runtime(app: &AppHandle) -> Result<Settings, String> {
    recover_existing_settings_for_runtime(storage::load_existing_settings(app))
}

fn recover_existing_settings_for_runtime(
    load_result: Result<Option<Settings>, String>,
) -> Result<Settings, String> {
    match load_result {
        Ok(Some(settings)) => Ok(settings),
        Ok(None) => Ok(Settings::default()),
        Err(error) if is_recoverable_settings_read_error(&error) => Ok(Settings::default()),
        Err(error) => Err(error),
    }
}

fn is_recoverable_settings_read_error(error: &str) -> bool {
    error.starts_with("设置文件超过安全读取上限，已备份")
        || error.starts_with("设置文件损坏，已备份")
}

fn clear_github_token_settings(mut settings: Settings) -> Result<Settings, String> {
    settings.github_token.clear();
    settings.normalized()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(SearchState::default())
        .manage(BrowserOpenLimiter::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            storage::ensure_data_directory(app.handle()).map_err(std::io::Error::other)?;
            storage::save_default_input_rules(app.handle());
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
            clear_package_cache,
            open_package_search,
            start_search,
            stop_search,
            export_diagnostics,
            load_input_rules,
            save_input_rules
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
    fn search_state_rejects_non_js_safe_run_ids() {
        let state = SearchState::default();

        assert!(state.try_begin(0).is_err());
        assert!(state.try_begin(MAX_JS_SAFE_INTEGER + 1).is_err());
        assert!(!state.is_running_for_test());

        let run = state
            .try_begin(MAX_JS_SAFE_INTEGER)
            .expect("JavaScript 最大安全整数应可用作任务 ID");
        assert_eq!(state.run_id_for_test(), MAX_JS_SAFE_INTEGER);
        drop(run);
    }

    #[test]
    fn search_state_releases_slot_when_setup_returns_error() {
        let state = SearchState::default();
        let setup_result = (|| -> Result<(), String> {
            let _run = state.try_begin(50)?;
            Err("模拟初始化失败".to_string())
        })();

        assert!(setup_result.is_err());
        assert!(!state.is_running_for_test());
        let retry = state.try_begin(51).expect("初始化失败后应允许重试");
        assert_eq!(state.run_id_for_test(), 51);
        drop(retry);
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
            ..Settings::default()
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
    fn recoverable_settings_read_errors_do_not_preserve_token() {
        assert!(is_recoverable_settings_read_error(
            "设置文件损坏，已备份；请重新确认设置后再保存"
        ));
        assert!(is_recoverable_settings_read_error(
            "设置文件超过安全读取上限，已备份；请重新确认设置后再保存"
        ));
        assert!(!is_recoverable_settings_read_error("存储目录无效"));

        let incoming = Settings {
            proxy: "127.0.0.1:7890".to_string(),
            github_token: String::new(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: false,
            ..Settings::default()
        };
        let merged = merge_runtime_settings(incoming, &Settings::default())
            .expect("损坏旧设置恢复保存时不应要求旧 Token");

        assert!(merged.github_token.is_empty());
        assert_eq!(merged.proxy, "http://127.0.0.1:7890");
    }

    #[test]
    fn runtime_settings_recover_from_corrupt_saved_settings() {
        let recovered = recover_existing_settings_for_runtime(Err(
            "设置文件损坏，已备份；请重新确认设置后再保存".to_string(),
        ))
        .expect("可恢复的设置读取错误应回退默认设置");

        assert!(recovered.proxy.is_empty());
        assert!(recovered.github_token.is_empty());
        assert_eq!(recovered.cran_mirror, "https://cloud.r-project.org");
        assert!(!recovered.full_search);

        let incoming = Settings {
            proxy: "127.0.0.1:7890".to_string(),
            github_token: String::new(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: true,
            ..Settings::default()
        };
        let merged =
            merge_runtime_settings(incoming, &recovered).expect("默认恢复设置应可参与运行时合并");

        assert!(merged.github_token.is_empty());
        assert_eq!(merged.proxy, "http://127.0.0.1:7890");
        assert!(merged.full_search);
    }

    #[test]
    fn runtime_settings_keep_unrecoverable_saved_settings_error() {
        let error = recover_existing_settings_for_runtime(Err("存储目录无效".to_string()))
            .expect_err("不可恢复的设置读取错误不应被吞掉");

        assert_eq!(error, "存储目录无效");
    }

    #[test]
    fn clearing_token_from_recovered_settings_uses_default_public_state() {
        let public = clear_github_token_settings(Settings::default())
            .expect("默认设置应可清除 Token")
            .public_view();

        assert!(!public.github_token_configured);
        assert!(public.proxy.is_empty());
        assert_eq!(public.cran_mirror, "https://cloud.r-project.org/");
    }

    #[test]
    fn clearing_token_preserves_other_settings() {
        let mut settings = Settings {
            proxy: "127.0.0.1:7890".to_string(),
            github_token: "ghp_saved".to_string(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: true,
            ..Settings::default()
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
