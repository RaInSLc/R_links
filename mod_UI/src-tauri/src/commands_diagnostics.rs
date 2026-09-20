use crate::{
    commands_settings::check_system_toolchain,
    commands_settings_runtime::load_existing_settings_for_runtime,
    logic, models,
    models::{MirrorSpeedResult, NetworkDiagnostic},
};
use futures_util::StreamExt;
use reqwest::Client;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use url::Url;
fn client(
    proxy: Option<&str>,
    connect: Duration,
    timeout: Duration,
    redirect: bool,
) -> Result<Client, String> {
    let mut b = Client::builder()
        .user_agent("RLinkModUI/0.1")
        .connect_timeout(connect)
        .timeout(timeout)
        .redirect(if redirect {
            reqwest::redirect::Policy::limited(3)
        } else {
            reqwest::redirect::Policy::none()
        });
    if let Some(p) = proxy.filter(|p| !p.trim().is_empty()) {
        b = b.proxy(reqwest::Proxy::all(p.trim()).map_err(|_| "网络代理配置无效".to_string())?)
    }
    b.build().map_err(|e| e.to_string())
}
#[tauri::command]
pub(crate) async fn test_mirror_speed(
    app: AppHandle,
    mirror_urls: Vec<String>,
) -> Result<Vec<MirrorSpeedResult>, String> {
    let mirrors = mirror_urls
        .into_iter()
        .map(|u| {
            let n = models::normalize_cran_mirror_url(&u)?;
            let l = n
                .trim_end_matches('/')
                .rsplit_once("://")
                .map(|(_, h)| h.to_string())
                .unwrap_or(n.clone());
            Ok((n, l))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let settings = load_existing_settings_for_runtime(&app)?;
    let c = client(
        Some(&settings.proxy),
        Duration::from_secs(5),
        Duration::from_secs(8),
        true,
    )?;
    let tasks = mirrors.iter().map(|(m, l)| {
        let c = c.clone();
        let m = m.clone();
        let l = l.clone();
        async move {
            let start = Instant::now();
            let r = c
                .get(format!("{}src/contrib/PACKAGES.gz", m))
                .header("Range", "bytes=0-1023")
                .send()
                .await;
            let ms = start.elapsed().as_millis() as u64;
            match r {
                Ok(x) if x.status().is_success() => MirrorSpeedResult {
                    mirror: m,
                    label: l,
                    latency_ms: ms,
                    success: true,
                    error: None,
                },
                Ok(x) => MirrorSpeedResult {
                    mirror: m,
                    label: l,
                    latency_ms: ms,
                    success: false,
                    error: Some(format!("HTTP {}", x.status().as_u16())),
                },
                Err(e) => MirrorSpeedResult {
                    mirror: m,
                    label: l,
                    latency_ms: ms,
                    success: false,
                    error: Some(e.to_string()),
                },
            }
        }
    });
    let mut r = futures_util::future::join_all(tasks).await;
    r.sort_by_key(|x| if x.success { x.latency_ms } else { u64::MAX });
    Ok(r)
}
#[tauri::command]
pub(crate) async fn test_network_connection(
    app: AppHandle,
) -> Result<Vec<NetworkDiagnostic>, String> {
    let s = load_existing_settings_for_runtime(&app)?;
    let p = s.proxy.trim().to_string();
    let c = client(
        Some(&p),
        Duration::from_secs(5),
        Duration::from_secs(12),
        true,
    )?;
    let targets = [
        ("GitHub API", "https://api.github.com/"),
        ("GitHub 仓库", "https://github.com/davidsjoberg/ggsankey"),
        ("CRAN 主站", "https://cloud.r-project.org/"),
    ];
    Ok(
        futures_util::future::join_all(targets.into_iter().map(|(t, u)| {
            let c = c.clone();
            let p = p.clone();
            async move {
                let st = Instant::now();
                let r = c
                    .get(u)
                    .header("Accept", "application/vnd.github+json")
                    .send()
                    .await;
                let ms = st.elapsed().as_millis() as u64;
                match r {
                    Ok(x) => NetworkDiagnostic {
                        target: t.to_string(),
                        url: u.to_string(),
                        success: x.status().is_success(),
                        status_code: Some(x.status().as_u16()),
                        latency_ms: ms,
                        proxy: if p.is_empty() {
                            "未配置".to_string()
                        } else {
                            p
                        },
                        error: if x.status().is_success() {
                            None
                        } else {
                            Some(format!("HTTP {}", x.status().as_u16()))
                        },
                    },
                    Err(e) => NetworkDiagnostic {
                        target: t.to_string(),
                        url: u.to_string(),
                        success: false,
                        status_code: None,
                        latency_ms: ms,
                        proxy: if p.is_empty() {
                            "未配置".to_string()
                        } else {
                            p
                        },
                        error: Some(e.to_string()),
                    },
                }
            }
        }))
        .await,
    )
}
pub(crate) fn configured_flag(value: &str) -> bool {
    !value.trim().is_empty()
}
const MAX_REVERSE_DEPS_HTML_BYTES: u64 = 2 * 1024 * 1024;
#[tauri::command]
pub(crate) async fn fetch_reverse_dependencies(
    app: AppHandle,
    package_name: String,
    mirror: String,
) -> Result<crate::models::ReverseDependenciesInfo, String> {
    let package_name = package_name.trim().to_string();
    if !logic::is_valid_package_name(&package_name) || package_name.contains('/') {
        return Err("无效包名".to_string());
    }
    let base = if mirror.trim().is_empty() {
        "https://cloud.r-project.org".to_string()
    } else {
        models::normalize_cran_mirror_url(&mirror)?
            .trim_end_matches('/')
            .to_string()
    };
    let url = format!(
        "{}/web/packages/{}/index.html",
        base,
        urlencoding::encode(&package_name)
    );
    crate::search_urls::validate_search_request_url(&url)?;
    let settings = load_existing_settings_for_runtime(&app)?;
    let response = client(
        Some(&settings.proxy),
        Duration::from_secs(10),
        Duration::from_secs(15),
        false,
    )?
    .get(&url)
    .send()
    .await
    .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "CRAN 页面请求失败: HTTP {}",
            response.status().as_u16()
        ));
    }
    let html = read_limited_response_body(response, MAX_REVERSE_DEPS_HTML_BYTES)
        .await?
        .ok_or_else(|| "CRAN 页面响应过大，已拒绝".to_string())?;
    logic::parse_reverse_dependencies(&html, &package_name)
        .ok_or_else(|| format!("无法解析 {} 的反向依赖信息", package_name))
}

/// 流式读取响应体，遇到累计字节超过上限立即中止，避免被恶意或异常服务
/// 拖入 OOM。返回 `Ok(None)` 表示体积超限，`Ok(Some(html))` 表示读取成功。
async fn read_limited_response_body(
    response: reqwest::Response,
    max_bytes: u64,
) -> Result<Option<String>, String> {
    let max = max_bytes as usize;
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if buf.len().saturating_add(chunk.len()) > max {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf)
        .map(Some)
        .map_err(|e| format!("CRAN 页面不是有效 UTF-8: {e}"))
}

/// 诊断导出专用：剔除代理字符串中可能存在的 `user:pass@` 凭据。
fn redact_proxy_url(proxy: &str) -> String {
    let trimmed = proxy.trim();
    if trimmed.is_empty() {
        return "未配置".to_string();
    }
    match Url::parse(trimmed) {
        Ok(parsed)
            if !parsed.username().is_empty() || parsed.password().is_some() =>
        {
            "[redacted]".to_string()
        }
        Ok(_) => trimmed.to_string(),
        Err(_) => trimmed.to_string(),
    }
}
#[tauri::command]
pub(crate) fn export_diagnostics(
    app: AppHandle,
    search_summary: Option<serde_json::Value>,
    failed_categories: Option<serde_json::Value>,
    update_status: Option<String>,
) -> Result<String, String> {
    let s = load_existing_settings_for_runtime(&app)?;
    let p = s.public_view();
    let v = serde_json::json!({"schema_version":2,"app_version":env!("CARGO_PKG_VERSION"),"settings":{"full_search":p.full_search,"proxy":redact_proxy_url(&p.proxy),"cran_mirror":p.cran_mirror,"github_token_configured":p.github_token_configured,"r_lib_path_configured":configured_flag(&p.r_lib_path)},"cache_entries":crate::storage::load_cache(&app).map(|x|x.len()).unwrap_or(0),"history_entries":crate::storage::load_history(&app).map(|x|x.len()).unwrap_or(0),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"toolchain":check_system_toolchain(),"search_summary":search_summary,"failed_categories":failed_categories,"update_status":update_status});
    serde_json::to_string_pretty(&v).map_err(|e| format!("诊断信息序列化失败: {e}"))
}

#[cfg(test)]
mod proxy_redaction_tests {
    use super::redact_proxy_url;

    #[test]
    fn redacts_credentials() {
        assert_eq!(
            redact_proxy_url("http://user:pass@127.0.0.1:7890"),
            "[redacted]"
        );
        assert_eq!(redact_proxy_url("https://u:p@example.com"), "[redacted]");
        assert_eq!(redact_proxy_url("http://user@host"), "[redacted]");
    }

    #[test]
    fn keeps_no_credential_proxy() {
        assert_eq!(
            redact_proxy_url("http://127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            redact_proxy_url("https://proxy.example.com"),
            "https://proxy.example.com"
        );
    }

    #[test]
    fn empty_or_invalid_returns_unset_or_passthrough() {
        assert_eq!(redact_proxy_url(""), "未配置");
        assert_eq!(redact_proxy_url("   "), "未配置");
        assert_eq!(redact_proxy_url("not a url"), "not a url");
    }
}
