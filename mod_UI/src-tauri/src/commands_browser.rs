use crate::{logic, state::BrowserOpenLimiter};
use std::time::Instant;
use tauri::{AppHandle, State};
pub(crate) fn browser_search_url_for_package(
    package_name: &str,
    ecosystem: Option<&str>,
) -> Result<String, String> {
    let package_name = package_name.trim();
    if !logic::is_valid_package_name(package_name) || package_name.contains('/') {
        return Err("无效包名，无法打开浏览器搜索".to_string());
    }
    let prefix = match ecosystem {
        Some("pip") => "Python package",
        Some("conda") => "Conda package",
        _ => "R package",
    };
    let url = format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(&format!("{prefix} {package_name}"))
    );
    if !logic::is_allowed_browser_search_url(&url) {
        return Err("浏览器搜索 URL 不在允许范围内".to_string());
    }
    Ok(url)
}
/// 共用的「限速 + 调起外部浏览器」入口。两个 Tauri 命令都需要：
/// 先确保用户没有连点（`BrowserOpenLimiter` 节流），再交给
/// `tauri_plugin_opener` 调起系统默认浏览器。返回的 Err 文案与既有
/// 行为保持一致。
fn open_validated_url(
    app: &AppHandle,
    limiter: &State<'_, BrowserOpenLimiter>,
    url: &str,
) -> Result<(), String> {
    limiter.try_acquire(Instant::now())?;
    tauri_plugin_opener::OpenerExt::opener(app)
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开浏览器失败: {e}"))
}
#[tauri::command]
pub(crate) fn open_package_search(
    app: AppHandle,
    limiter: State<'_, BrowserOpenLimiter>,
    package_name: String,
    ecosystem: Option<String>,
) -> Result<(), String> {
    let url = browser_search_url_for_package(&package_name, ecosystem.as_deref())?;
    open_validated_url(&app, &limiter, &url)
}
#[tauri::command]
pub(crate) fn open_package_page(
    app: AppHandle,
    limiter: State<'_, BrowserOpenLimiter>,
    package: String,
    source: String,
    repository: String,
) -> Result<(), String> {
    let url = logic::build_package_page_url(&package, &source, &repository)?;
    if !logic::is_allowed_package_page_url(&url) {
        return Err("包页面 URL 不在允许范围内".to_string());
    }
    open_validated_url(&app, &limiter, &url)
}
