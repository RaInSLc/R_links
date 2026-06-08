use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use url::Url;

use crate::logic::{infer_bioc_version, normalize_github_repository, parse_inputs};
use crate::models::{PackageInput, SearchResponse, SearchResult, Settings};

const BIOC_VERSIONS: &[&str] = &[
    "3.23", "3.22", "3.21", "3.20", "3.19", "3.18", "3.17", "3.16", "3.15", "3.14", "3.13", "3.12",
    "3.11", "3.10", "3.9", "3.8", "3.7", "3.6", "3.5", "3.4", "3.3", "3.2", "3.1", "3.0",
];
const BIOC_CATEGORIES: &[&str] = &["bioc", "data/annotation", "data/experiment", "workflows"];
const MAX_TEXT_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_DESCRIPTION_BYTES: usize = 64 * 1024;
const MAX_JSON_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_SEARCH_HTTP_REQUESTS: usize = 200;

#[derive(Debug, Deserialize)]
struct GithubSearchResponse {
    items: Vec<GithubRepository>,
}

#[derive(Debug, Deserialize)]
struct GithubRepository {
    full_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchLogEvent {
    pub run_id: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgressEvent {
    pub run_id: u64,
    pub result: SearchResult,
}

struct RequestBudget {
    remaining: AtomicUsize,
}

impl RequestBudget {
    fn new(limit: usize) -> Self {
        Self {
            remaining: AtomicUsize::new(limit),
        }
    }

    fn try_acquire(&self) -> Result<(), String> {
        self.remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                if remaining > 0 {
                    Some(remaining - 1)
                } else {
                    None
                }
            })
            .map(|_| ())
            .map_err(|_| {
                format!("单次检索 HTTP 请求数超过上限 {MAX_SEARCH_HTTP_REQUESTS}，任务已停止")
            })
    }

    #[cfg(test)]
    fn remaining_for_test(&self) -> usize {
        self.remaining.load(Ordering::SeqCst)
    }
}

struct SearchContext<'a> {
    client: &'a Client,
    settings: &'a Settings,
    cancelled: &'a AtomicBool,
    budget: &'a RequestBudget,
    app: &'a AppHandle,
    run_id: u64,
    logs: &'a mut Vec<String>,
}

impl SearchContext<'_> {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn log(&mut self, message: &str) {
        log(self.app, self.run_id, self.logs, message);
    }

    fn acquire_request_budget(&mut self) -> bool {
        match self.budget.try_acquire() {
            Ok(()) => true,
            Err(message) => {
                self.log(&message);
                false
            }
        }
    }
}

pub async fn search_packages(
    app: &AppHandle,
    run_id: u64,
    cancelled: &AtomicBool,
    input: &str,
    settings: &Settings,
) -> Result<SearchResponse, String> {
    if run_id == 0 {
        return Err("检索任务 ID 无效".to_string());
    }
    let packages = parse_inputs(input)?;
    if packages.is_empty() {
        return Err("请输入至少一个有效的 R 包".to_string());
    }

    let client = build_client(settings)?;
    let budget = RequestBudget::new(MAX_SEARCH_HTTP_REQUESTS);
    let mut results = Vec::new();
    let mut logs = Vec::new();

    let stopped = {
        let mut context = SearchContext {
            client: &client,
            settings,
            cancelled,
            budget: &budget,
            app,
            run_id,
            logs: &mut logs,
        };

        context.log("开始多源检索");
        for (index, package) in packages.iter().enumerate() {
            if context.is_cancelled() {
                break;
            }
            context.log(&format!(
                "[{}/{}] 检索 {}{}",
                index + 1,
                packages.len(),
                package.name,
                if package.version.is_empty() {
                    String::new()
                } else {
                    format!(" {}", package.version)
                }
            ));

            let before = results.len();
            if package.name.contains('/') {
                if let Some(result) = search_explicit_github(&mut context, package).await {
                    push_result(app, run_id, &mut results, result);
                }
            } else {
                if let Some(result) = search_cran(&mut context, package).await {
                    push_result(app, run_id, &mut results, result);
                }

                if (settings.full_search || results.len() == before) && !context.is_cancelled() {
                    let bioc_results = search_bioconductor(&mut context, package).await;
                    for result in bioc_results {
                        push_result(app, run_id, &mut results, result);
                    }
                }

                if (settings.full_search || results.len() == before) && !context.is_cancelled() {
                    let github_results = search_github(&mut context, package).await;
                    for result in github_results {
                        push_result(app, run_id, &mut results, result);
                    }
                }
            }

            if results.len() == before && !context.is_cancelled() {
                let result = SearchResult {
                    package: package.name.clone(),
                    requested_version: package.version.clone(),
                    latest_version: String::new(),
                    repository: String::new(),
                    real_name: package.name.clone(),
                    source: "none".to_string(),
                    found: false,
                    message: "所有来源均未找到".to_string(),
                };
                push_result(app, run_id, &mut results, result);
            }
        }

        let stopped = context.is_cancelled();
        context.log(if stopped {
            "检索任务已停止"
        } else {
            "检索任务已完成"
        });
        stopped
    };
    Ok(SearchResponse {
        run_id,
        results,
        logs,
        stopped,
    })
}

fn build_client(settings: &Settings) -> Result<Client, String> {
    let mut builder = Client::builder()
        .user_agent("RLinkModUI/0.1")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30));
    if !settings.proxy.trim().is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(settings.proxy.trim())
                .map_err(|_| "网络代理配置无效".to_string())?,
        );
    }
    builder.build().map_err(|error| error.to_string())
}

async fn search_cran(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Option<SearchResult> {
    context.log("查询 CRAN");
    let url = format!(
        "https://cloud.r-project.org/web/packages/{}/index.html",
        urlencoding::encode(&package.name)
    );
    let html = get_text(context, &url).await?;
    let version = extract_html_version(&html)?;
    context.log(&format!("CRAN 命中版本 {version}"));
    Some(found_result(package, &version, "", &package.name, "cran"))
}

async fn search_bioconductor(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Vec<SearchResult> {
    context.log("查询 Bioconductor");
    for category in BIOC_CATEGORIES {
        if context.is_cancelled() {
            return Vec::new();
        }
        let release_url = format!(
            "https://bioconductor.org/packages/release/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        if let Some(html) = get_text(context, &release_url).await {
            if let Some(release_version) = extract_html_version(&html) {
                if !package.version.is_empty()
                    && !version_compatible(&release_version, &package.version)
                {
                    if let Some(history) = find_bioc_history(context, package, category).await {
                        return vec![history];
                    }
                }
                context.log(&format!("Bioconductor Release 命中版本 {release_version}"));
                return vec![found_result(
                    package,
                    &release_version,
                    "",
                    &package.name,
                    "bioc",
                )];
            }
        }
    }
    Vec::new()
}

async fn find_bioc_history(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
    category: &str,
) -> Option<SearchResult> {
    let mut versions = BIOC_VERSIONS.to_vec();
    let parts = package
        .version
        .split('.')
        .filter_map(|value| value.parse::<i32>().ok())
        .collect::<Vec<_>>();
    if parts.len() >= 2 {
        if let Some(inferred) = infer_bioc_version(parts[0], parts[1]) {
            let inferred = format!("3.{inferred}");
            if let Some(position) = versions.iter().position(|value| *value == inferred) {
                versions.remove(position);
                versions.insert(0, BIOC_VERSIONS[position]);
            }
        }
    }

    for bioc_version in versions {
        if context.is_cancelled() {
            return None;
        }
        let url = format!(
            "https://bioconductor.org/packages/{bioc_version}/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        if let Some(html) = get_text(context, &url).await {
            if let Some(version) = extract_html_version(&html) {
                if version_compatible(&version, &package.version) {
                    context.log(&format!("Bioconductor {bioc_version} 匹配版本 {version}"));
                    return Some(found_result(
                        package,
                        &version,
                        bioc_version,
                        &package.name,
                        "biocGit",
                    ));
                }
            }
        }
    }
    None
}

async fn search_explicit_github(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Option<SearchResult> {
    context.log("验证指定 GitHub 仓库");
    let Some(repository) = normalize_github_repository(&package.name) else {
        context.log("GitHub 仓库格式无效，已跳过");
        return None;
    };
    let version = github_description_version(context, &repository).await?;
    let real_name = repository.rsplit('/').next().unwrap_or(&repository);
    Some(found_result(
        package,
        &version,
        &repository,
        real_name,
        "github",
    ))
}

async fn search_github(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Vec<SearchResult> {
    context.log("查询 r-universe 与 GitHub");
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let universe_url = format!(
        "https://r-universe.dev/api/search?q=package:{}&limit=1",
        urlencoding::encode(&package.name)
    );
    if let Some(value) = get_json(context, &universe_url).await {
        if let Some(object) = find_package_object(&value) {
            let real_name = object
                .get("Package")
                .and_then(Value::as_str)
                .unwrap_or(&package.name);
            let version = object
                .get("Version")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let repository = object
                .get("RemoteUrl")
                .and_then(Value::as_str)
                .and_then(normalize_github_repository);
            if let Some(repository) = repository {
                seen.insert(repository.to_ascii_lowercase());
                if let Some(version) = clean_version(version) {
                    results.push(found_result(
                        package,
                        &version,
                        repository.as_str(),
                        real_name,
                        "github",
                    ));
                }
            }
        }
    }

    if !results.is_empty() && !context.settings.full_search {
        return results;
    }

    let url = format!(
        "https://api.github.com/search/repositories?q={}+language:R&sort=stars&per_page=10",
        urlencoding::encode(&package.name)
    );
    if !context.acquire_request_budget() {
        return results;
    }
    let request = authorized_get(context.client, &url, context.settings);
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            context.log(&format!("GitHub 请求失败: {error}"));
            return results;
        }
    };
    if response.status() == StatusCode::FORBIDDEN {
        context.log("GitHub API 已触发频率限制");
        return results;
    }
    let text = match read_limited_text(response, MAX_JSON_RESPONSE_BYTES).await {
        Ok(value) => value,
        Err(error) => {
            context.log(&format!("GitHub 响应读取失败: {error}"));
            return results;
        }
    };
    let body = match serde_json::from_str::<GithubSearchResponse>(&text) {
        Ok(value) => value,
        Err(error) => {
            context.log(&format!("GitHub 响应解析失败: {error}"));
            return results;
        }
    };

    for repository in body.items {
        if context.is_cancelled() {
            break;
        }
        let repo_name = repository.full_name.rsplit('/').next().unwrap_or_default();
        let lower_repo = repo_name.to_ascii_lowercase();
        let lower_package = package.name.to_ascii_lowercase();
        if !lower_repo.contains(&lower_package)
            || seen.contains(&repository.full_name.to_ascii_lowercase())
        {
            continue;
        }
        if let Some(repository_name) = normalize_github_repository(&repository.full_name) {
            if let Some(version) = github_description_version(context, &repository_name).await {
                seen.insert(repository_name.to_ascii_lowercase());
                results.push(found_result(
                    package,
                    &version,
                    &repository_name,
                    repo_name,
                    "github",
                ));
            }
        }
    }
    results
}

async fn github_description_version(
    context: &mut SearchContext<'_>,
    repository: &str,
) -> Option<String> {
    for branch in ["HEAD", "master", "main", "devel"] {
        if context.is_cancelled() {
            return None;
        }
        let url = format!("https://raw.githubusercontent.com/{repository}/{branch}/DESCRIPTION");
        if !context.acquire_request_budget() {
            return None;
        }
        let response = authorized_get(context.client, &url, context.settings)
            .send()
            .await
            .ok()?;
        if response.status().is_success() {
            let description = read_limited_text(response, MAX_DESCRIPTION_BYTES)
                .await
                .ok()?;
            if let Some(version) = extract_description_version(&description) {
                return Some(version);
            }
        }
    }
    None
}

fn authorized_get(client: &Client, url: &str, settings: &Settings) -> reqwest::RequestBuilder {
    let request = client
        .get(url)
        .header("Accept", "application/vnd.github+json");
    if should_attach_github_token(url, settings) {
        request.bearer_auth(settings.github_token.trim())
    } else {
        request
    }
}

async fn get_text(context: &mut SearchContext<'_>, url: &str) -> Option<String> {
    if context.is_cancelled() {
        return None;
    }
    if !context.acquire_request_budget() {
        return None;
    }
    let response = context.client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    read_limited_text(response, MAX_TEXT_RESPONSE_BYTES)
        .await
        .ok()
}

async fn get_json(context: &mut SearchContext<'_>, url: &str) -> Option<Value> {
    if context.is_cancelled() {
        return None;
    }
    if !context.acquire_request_budget() {
        return None;
    }
    let response = authorized_get(context.client, url, context.settings)
        .send()
        .await
        .ok()?;
    let text = read_limited_text(response, MAX_JSON_RESPONSE_BYTES)
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}

async fn read_limited_text(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<String, String> {
    if let Some(length) = response.content_length() {
        if length > limit as u64 {
            return Err("响应内容超过大小限制".to_string());
        }
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "读取响应失败".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("响应内容超过大小限制".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "响应不是有效 UTF-8".to_string())
}

fn should_attach_github_token(url: &str, settings: &Settings) -> bool {
    !settings.github_token.trim().is_empty()
        && Url::parse(url)
            .ok()
            .and_then(|parsed| {
                parsed
                    .host_str()
                    .map(|host| host.eq_ignore_ascii_case("api.github.com"))
            })
            .unwrap_or(false)
}

fn extract_html_version(html: &str) -> Option<String> {
    let regex = Regex::new(r"(?is)<td[^>]*>\s*Version[^<]*</td>\s*<td[^>]*>\s*([^<\s][^<]*)</td>")
        .expect("固定 HTML 版本正则必须有效");
    regex
        .captures(html)
        .and_then(|capture| capture.get(1))
        .and_then(|value| clean_version(value.as_str()))
}

fn extract_description_version(description: &str) -> Option<String> {
    description
        .lines()
        .find_map(|line| line.strip_prefix("Version:").and_then(clean_version))
}

fn clean_version(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        .then(|| trimmed.to_string())
}

fn find_package_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    match value {
        Value::Object(object) => {
            if object.contains_key("Package") {
                return Some(object);
            }
            object.values().find_map(find_package_object)
        }
        Value::Array(values) => values.iter().find_map(find_package_object),
        _ => None,
    }
}

fn version_compatible(found: &str, requested: &str) -> bool {
    found == requested
        || (requested.matches('.').count() == 1
            && found
                .strip_prefix(requested)
                .is_some_and(|suffix| suffix.starts_with('.')))
}

fn found_result(
    package: &PackageInput,
    version: &str,
    repository: &str,
    real_name: &str,
    source: &str,
) -> SearchResult {
    SearchResult {
        package: package.name.clone(),
        requested_version: package.version.clone(),
        latest_version: version.to_string(),
        repository: repository.to_string(),
        real_name: real_name.to_string(),
        source: source.to_string(),
        found: true,
        message: "验证成功".to_string(),
    }
}

fn log(app: &AppHandle, run_id: u64, logs: &mut Vec<String>, message: &str) {
    logs.push(message.to_string());
    let _ = app.emit(
        "search-log",
        SearchLogEvent {
            run_id,
            message: message.to_string(),
        },
    );
}

fn push_result(
    app: &AppHandle,
    run_id: u64,
    results: &mut Vec<SearchResult>,
    result: SearchResult,
) {
    let _ = app.emit(
        "search-progress",
        SearchProgressEvent {
            run_id,
            result: result.clone(),
        },
    );
    results.push(result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_versions_from_sources() {
        assert_eq!(
            extract_html_version("<td>Version:</td><td>1.2.3</td>"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            extract_description_version("Package: demo\nVersion: 0.4.1\n"),
            Some("0.4.1".to_string())
        );
    }

    #[test]
    fn accepts_major_minor_request() {
        assert!(version_compatible("1.50.2", "1.50"));
        assert!(!version_compatible("1.52.0", "1.50"));
    }

    #[test]
    fn sends_token_only_to_github_api() {
        let settings = Settings {
            github_token: "ghp_demo".to_string(),
            ..Settings::default()
        };
        assert!(should_attach_github_token(
            "https://api.github.com/search/repositories?q=demo",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://r-universe.dev/api/search?q=package:demo",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://raw.githubusercontent.com/owner/repo/HEAD/DESCRIPTION",
            &settings
        ));
    }

    #[test]
    fn rejects_untrusted_github_repository_hosts() {
        assert_eq!(
            normalize_github_repository("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert!(normalize_github_repository("https://example.com/github.com/owner/repo").is_none());
        assert!(normalize_github_repository("https://github.com/owner/repo/issues").is_none());
        assert!(normalize_github_repository("https://github.com/owner/repo?tab=readme").is_none());
    }

    #[test]
    fn rejects_unbounded_or_controlled_versions() {
        assert_eq!(clean_version(" 1.2.3-rc1 "), Some("1.2.3-rc1".to_string()));
        assert!(clean_version("1.2.3\nInjected: yes").is_none());
        assert!(clean_version(&"1".repeat(65)).is_none());
        assert!(extract_description_version("Version: 1.0.0\n").is_some());
        assert!(extract_description_version("Version: 1.0.0<script>\n").is_none());
    }

    #[test]
    fn serializes_search_events_with_run_id() {
        let event = SearchLogEvent {
            run_id: 42,
            message: "开始".to_string(),
        };
        let encoded = serde_json::to_string(&event).expect("事件应可序列化");

        assert!(encoded.contains("\"runId\":42"));
        assert!(encoded.contains("\"message\""));
    }

    #[test]
    fn request_budget_rejects_after_limit() {
        let budget = RequestBudget::new(2);

        assert!(budget.try_acquire().is_ok());
        assert!(budget.try_acquire().is_ok());
        assert!(budget.try_acquire().is_err());
        assert_eq!(budget.remaining_for_test(), 0);
    }
}
