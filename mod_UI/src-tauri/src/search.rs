use regex::Regex;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use url::Url;

use crate::logic::{
    infer_bioc_version, is_valid_package_name, normalize_github_repository, parse_inputs,
};
use crate::models::{url_has_explicit_port, PackageInput, SearchResponse, SearchResult, Settings};

const BIOC_VERSIONS: &[&str] = &[
    "3.23", "3.22", "3.21", "3.20", "3.19", "3.18", "3.17", "3.16", "3.15", "3.14", "3.13", "3.12",
    "3.11", "3.10", "3.9", "3.8", "3.7", "3.6", "3.5", "3.4", "3.3", "3.2", "3.1", "3.0",
];
const BIOC_CATEGORIES: &[&str] = &["bioc", "data/annotation", "data/experiment", "workflows"];
const MAX_TEXT_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_DESCRIPTION_BYTES: usize = 64 * 1024;
const MAX_JSON_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_SEARCH_HTTP_REQUESTS: usize = 200;
const MAX_RESULT_MESSAGE_CHARS: usize = 256;
const MAX_SEARCH_LOG_CHARS: usize = 512;
const MAX_SEARCH_LOGS: usize = 1_000;
const SEARCH_LOG_EMPTY_MESSAGE: &str = "日志内容为空或已被清理";
const SEARCH_LOGS_TRUNCATED_MESSAGE: &str = "检索日志达到上限，后续日志已停止记录";
static HTML_VERSION_RE: OnceLock<Regex> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct GithubSearchResponse {
    items: Vec<GithubRepository>,
}

#[derive(Debug, Deserialize)]
struct GithubRepository {
    full_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubDescription {
    package_name: String,
    version: String,
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
    exhausted: AtomicBool,
}

impl RequestBudget {
    fn new(limit: usize) -> Self {
        Self {
            remaining: AtomicUsize::new(limit),
            exhausted: AtomicBool::new(false),
        }
    }

    fn try_acquire(&self) -> Result<(), String> {
        let result = self
            .remaining
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
            });
        if result.is_err() {
            self.exhausted.store(true, Ordering::SeqCst);
        }
        result
    }

    fn is_exhausted(&self) -> bool {
        self.exhausted.load(Ordering::SeqCst)
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
    fn is_stopped(&self) -> bool {
        search_stopped(self.cancelled, self.budget)
    }

    fn log(&mut self, message: &str) {
        log(self.app, self.run_id, self.logs, message);
    }

    fn acquire_request_budget(&mut self) -> bool {
        match self.budget.try_acquire() {
            Ok(()) => true,
            Err(message) => {
                let message = sanitize_log_message(&message);
                if !self.logs.iter().any(|log| log == &message) {
                    self.log(&message);
                }
                false
            }
        }
    }
}

fn search_stopped(cancelled: &AtomicBool, budget: &RequestBudget) -> bool {
    cancelled.load(Ordering::SeqCst) || budget.is_exhausted()
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
            if context.is_stopped() {
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

                if (settings.full_search || results.len() == before) && !context.is_stopped() {
                    let bioc_results = search_bioconductor(&mut context, package).await;
                    for result in bioc_results {
                        push_result(app, run_id, &mut results, result);
                    }
                }

                if (settings.full_search || results.len() == before) && !context.is_stopped() {
                    let github_results = search_github(&mut context, package).await;
                    for result in github_results {
                        push_result(app, run_id, &mut results, result);
                    }
                }
            }

            if results.len() == before && !context.is_stopped() {
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

        let stopped = context.is_stopped();
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
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());
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
        if context.is_stopped() {
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
        if context.is_stopped() {
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
    let description = github_description(context, &repository).await?;
    Some(found_result(
        package,
        &description.version,
        &repository,
        &description.package_name,
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
            if let Some(real_name) = object.get("Package").and_then(Value::as_str) {
                if !github_package_name_matches_request(real_name, &package.name) {
                    // 忽略不可信的 r-universe 命中，继续尝试 GitHub API 检索。
                } else {
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
        }
    }

    if !results.is_empty() && !context.settings.full_search {
        return results;
    }

    let url = format!(
        "https://api.github.com/search/repositories?q={}+language:R&sort=stars&per_page=10",
        urlencoding::encode(&package.name)
    );
    let request = match authorized_get(context.client, &url, context.settings) {
        Ok(request) => request,
        Err(error) => {
            context.log(&error);
            return results;
        }
    };
    if !context.acquire_request_budget() {
        return results;
    }
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
        if context.is_stopped() {
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
            if let Some(description) = github_description(context, &repository_name).await {
                if !github_package_name_matches_request(&description.package_name, &package.name) {
                    continue;
                }
                seen.insert(repository_name.to_ascii_lowercase());
                results.push(found_result(
                    package,
                    &description.version,
                    &repository_name,
                    &description.package_name,
                    "github",
                ));
            }
        }
    }
    results
}

async fn github_description(
    context: &mut SearchContext<'_>,
    repository: &str,
) -> Option<GithubDescription> {
    for branch in ["HEAD", "master", "main", "devel"] {
        if context.is_stopped() {
            return None;
        }
        let url = format!("https://raw.githubusercontent.com/{repository}/{branch}/DESCRIPTION");
        let response = match authorized_get(context.client, &url, context.settings) {
            Ok(request) => {
                if !context.acquire_request_budget() {
                    return None;
                }
                request.send().await.ok()?
            }
            Err(error) => {
                context.log(&error);
                return None;
            }
        };
        if response.status().is_success() {
            let description = read_limited_text(response, MAX_DESCRIPTION_BYTES)
                .await
                .ok()?;
            if let Some(description) = extract_description_metadata(&description) {
                return Some(description);
            }
        }
    }
    None
}

fn authorized_get(
    client: &Client,
    url: &str,
    settings: &Settings,
) -> Result<RequestBuilder, String> {
    validate_search_request_url(url)?;
    let request = client
        .get(url)
        .header("Accept", "application/vnd.github+json");
    Ok(if should_attach_github_token(url, settings) {
        request.bearer_auth(settings.github_token.trim())
    } else {
        request
    })
}

async fn get_text(context: &mut SearchContext<'_>, url: &str) -> Option<String> {
    if context.is_stopped() {
        return None;
    }
    if let Err(error) = validate_search_request_url(url) {
        context.log(&error);
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
    if context.is_stopped() {
        return None;
    }
    let request = match authorized_get(context.client, url, context.settings) {
        Ok(request) => request,
        Err(error) => {
            context.log(&error);
            return None;
        }
    };
    if !context.acquire_request_budget() {
        return None;
    }
    let response = request.send().await.ok()?;
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
        && Url::parse(url).ok().is_some_and(|parsed| {
            parsed
                .host_str()
                .is_some_and(|host| host == "api.github.com")
                && validate_search_request_url(url).is_ok()
        })
}

fn validate_search_request_url(value: &str) -> Result<(), String> {
    let parsed = Url::parse(value).map_err(|_| "检索 URL 无效，已阻止请求".to_string())?;
    if parsed.scheme() != "https"
        || parsed.port().is_some()
        || url_has_explicit_port(value)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err("检索 URL 不在允许范围内，已阻止请求".to_string());
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "检索 URL 缺少主机名，已阻止请求".to_string())?;
    let path = parsed.path();
    let allowed = match host {
        "cloud.r-project.org" => parsed.query().is_none() && is_allowed_cran_package_path(&parsed),
        "bioconductor.org" => parsed.query().is_none() && is_allowed_bioc_package_path(&parsed),
        "r-universe.dev" => path == "/api/search" && is_allowed_r_universe_query(&parsed),
        "api.github.com" => {
            path == "/search/repositories" && is_allowed_github_search_query(&parsed)
        }
        "raw.githubusercontent.com" => {
            parsed.query().is_none()
                && parsed.path_segments().is_some_and(|segments| {
                    let segments = segments.collect::<Vec<_>>();
                    segments.len() == 4
                        && normalize_github_repository(&format!("{}/{}", segments[0], segments[1]))
                            .is_some()
                        && matches!(segments[2], "HEAD" | "master" | "main" | "devel")
                        && segments[3] == "DESCRIPTION"
                })
        }
        _ => false,
    };

    if allowed {
        Ok(())
    } else {
        Err("检索 URL 不在允许范围内，已阻止请求".to_string())
    }
}

fn is_allowed_cran_package_path(url: &Url) -> bool {
    url.path_segments().is_some_and(|segments| {
        let segments = segments.collect::<Vec<_>>();
        segments.len() == 4
            && segments[0] == "web"
            && segments[1] == "packages"
            && is_valid_search_package_query(segments[2])
            && segments[3] == "index.html"
    })
}

fn is_allowed_bioc_package_path(url: &Url) -> bool {
    url.path_segments().is_some_and(|segments| {
        let segments = segments.collect::<Vec<_>>();
        if segments.len() < 5
            || segments[0] != "packages"
            || !is_allowed_bioc_release_segment(segments[1])
        {
            return false;
        }
        match segments.as_slice() {
            [_, _, "bioc" | "workflows", "html", file] => is_allowed_bioc_package_file(file),
            [_, _, "data", "annotation" | "experiment", "html", file] => {
                is_allowed_bioc_package_file(file)
            }
            _ => false,
        }
    })
}

fn is_allowed_bioc_release_segment(value: &str) -> bool {
    value == "release"
        || value.split_once('.').is_some_and(|(major, minor)| {
            !major.is_empty()
                && !minor.is_empty()
                && major.chars().all(|character| character.is_ascii_digit())
                && minor.chars().all(|character| character.is_ascii_digit())
        })
}

fn is_allowed_bioc_package_file(file: &str) -> bool {
    file.strip_suffix(".html")
        .is_some_and(is_valid_search_package_query)
}

fn is_allowed_r_universe_query(url: &Url) -> bool {
    let mut pairs = url.query_pairs();
    let Some((query_key, query_value)) = pairs.next() else {
        return false;
    };
    if query_key != "q"
        || !query_value
            .as_ref()
            .strip_prefix("package:")
            .is_some_and(is_valid_search_package_query)
    {
        return false;
    }

    let Some((limit_key, limit_value)) = pairs.next() else {
        return false;
    };
    limit_key == "limit" && limit_value == "1" && pairs.next().is_none()
}

fn is_allowed_github_search_query(url: &Url) -> bool {
    let mut pairs = url.query_pairs();
    let Some((query_key, query_value)) = pairs.next() else {
        return false;
    };
    if query_key != "q"
        || !query_value
            .as_ref()
            .strip_suffix(" language:R")
            .is_some_and(is_valid_search_package_query)
    {
        return false;
    }

    let Some((sort_key, sort_value)) = pairs.next() else {
        return false;
    };
    if sort_key != "sort" || sort_value != "stars" {
        return false;
    }

    let Some((page_key, page_value)) = pairs.next() else {
        return false;
    };
    page_key == "per_page" && page_value == "10" && pairs.next().is_none()
}

fn is_valid_search_package_query(value: &str) -> bool {
    !value.contains('/') && is_valid_package_name(value)
}

fn extract_html_version(html: &str) -> Option<String> {
    let regex = HTML_VERSION_RE.get_or_init(|| {
        Regex::new(r"(?is)<td[^>]*>\s*Version[^<]*</td>\s*<td[^>]*>\s*([^<\s][^<]*)</td>")
            .expect("固定 HTML 版本正则必须有效")
    });
    regex
        .captures(html)
        .and_then(|capture| capture.get(1))
        .and_then(|value| clean_version(value.as_str()))
}

fn extract_description_metadata(description: &str) -> Option<GithubDescription> {
    let mut package_name = None;
    let mut version = None;
    let mut continuation_allowed = false;

    for raw_line in description.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continuation_allowed = false;
            continue;
        }
        if line.starts_with([' ', '\t']) {
            if !continuation_allowed {
                return None;
            }
            continue;
        }

        let (field, value) = line.split_once(':')?;
        if !is_valid_description_field_name(field) {
            return None;
        }
        let value = value.trim();
        match field {
            "Package" => {
                if package_name.is_some() {
                    return None;
                }
                package_name = Some(clean_result_package_name(value)?);
                continuation_allowed = false;
            }
            "Version" => {
                if version.is_some() {
                    return None;
                }
                version = Some(clean_version(value)?);
                continuation_allowed = false;
            }
            _ => {
                continuation_allowed = true;
            }
        }
    }

    Some(GithubDescription {
        package_name: package_name?,
        version: version?,
    })
}

fn is_valid_description_field_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii()
                && !character.is_ascii_control()
                && !character.is_ascii_whitespace()
                && character != ':'
        })
}

fn github_package_name_matches_request(real_name: &str, requested: &str) -> bool {
    clean_result_package_name(real_name)
        .zip(clean_result_package_name(requested))
        .is_some_and(|(real_name, requested)| real_name.eq_ignore_ascii_case(&requested))
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
        Value::Object(object) if object.contains_key("Package") => Some(object),
        Value::Array(values) => values.iter().find_map(|item| match item {
            Value::Object(object) if object.contains_key("Package") => Some(object),
            _ => None,
        }),
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
    let package_name =
        clean_result_package_name(&package.name).unwrap_or_else(|| package.name.clone());
    let latest_version = clean_version(version).unwrap_or_default();
    let source = clean_result_source(source);
    let repository = clean_result_repository(&source, repository).unwrap_or_default();
    let Some(real_name) = clean_result_real_name(&source, real_name, &package_name) else {
        return SearchResult {
            package: package_name.clone(),
            requested_version: package.version.clone(),
            latest_version: String::new(),
            repository: String::new(),
            real_name: package_name,
            source: "none".to_string(),
            found: false,
            message: "结果真实包名无效，已忽略".to_string(),
        };
    };
    SearchResult {
        package: package_name,
        requested_version: package.version.clone(),
        latest_version,
        repository,
        real_name,
        source,
        found: true,
        message: "验证成功".to_string(),
    }
}

fn clean_result_package_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    is_valid_package_name(trimmed).then(|| trimmed.to_string())
}

fn clean_result_real_name(source: &str, value: &str, fallback: &str) -> Option<String> {
    match clean_result_package_name(value) {
        Some(real_name) => Some(real_name),
        None if source == "github" => None,
        None => Some(fallback.to_string()),
    }
}

fn clean_result_repository(source: &str, value: &str) -> Option<String> {
    let trimmed = value.trim();
    match source {
        "github" => {
            if trimmed.is_empty() {
                Some(String::new())
            } else {
                normalize_github_repository(trimmed)
            }
        }
        "biocGit" => {
            if trimmed.is_empty() || is_valid_bioc_version(trimmed) {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
        _ => trimmed.is_empty().then(String::new),
    }
}

fn clean_result_source(value: &str) -> String {
    match value.trim() {
        "cran" | "bioc" | "biocGit" | "github" | "none" => value.trim().to_string(),
        _ => "none".to_string(),
    }
}

fn sanitize_result_message(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_RESULT_MESSAGE_CHARS)
        .collect()
}

fn sanitize_search_result_for_emit(mut result: SearchResult) -> SearchResult {
    let fallback_package = clean_result_package_name(&result.package).unwrap_or_default();
    result.package = fallback_package.clone();
    result.requested_version = clean_version(&result.requested_version).unwrap_or_default();
    result.latest_version = clean_version(&result.latest_version).unwrap_or_default();
    result.source = clean_result_source(&result.source);
    result.repository =
        clean_result_repository(&result.source, &result.repository).unwrap_or_default();
    result.real_name = clean_result_package_name(&result.real_name).unwrap_or(fallback_package);
    result.message = sanitize_result_message(&result.message);
    result
}

fn sanitize_log_message(value: &str) -> String {
    let message = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_SEARCH_LOG_CHARS)
        .collect::<String>();
    if message.is_empty() {
        SEARCH_LOG_EMPTY_MESSAGE.to_string()
    } else {
        message
    }
}

fn is_valid_bioc_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

fn append_search_log(logs: &mut Vec<String>, message: &str) -> Option<String> {
    if logs.len() >= MAX_SEARCH_LOGS {
        return None;
    }

    let message = if logs.len() + 1 == MAX_SEARCH_LOGS {
        SEARCH_LOGS_TRUNCATED_MESSAGE.to_string()
    } else {
        sanitize_log_message(message)
    };
    logs.push(message.clone());
    Some(message)
}

fn log(app: &AppHandle, run_id: u64, logs: &mut Vec<String>, message: &str) {
    if let Some(message) = append_search_log(logs, message) {
        let _ = app.emit("search-log", SearchLogEvent { run_id, message });
    }
}

fn push_result(
    app: &AppHandle,
    run_id: u64,
    results: &mut Vec<SearchResult>,
    result: SearchResult,
) {
    let result = sanitize_search_result_for_emit(result);
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
            extract_description_metadata("Package: demo\nVersion: 0.4.1\n"),
            Some(GithubDescription {
                package_name: "demo".to_string(),
                version: "0.4.1".to_string(),
            })
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
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=stars&per_page=10",
            &settings
        ));
        assert!(!should_attach_github_token(
            "http://api.github.com/search/repositories?q=demo",
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
        assert!(!should_attach_github_token(
            "https://api.github.com/search/repositories?q=demo",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://api.github.com/search/repositories?q=owner%2Frepo+language%3AR&sort=stars&per_page=10",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=stars&per_page=10",
            &Settings::default()
        ));
    }

    #[test]
    fn validates_search_request_url_scope() {
        for url in [
            "https://cloud.r-project.org/web/packages/demo/index.html",
            "https://bioconductor.org/packages/release/bioc/html/demo.html",
            "https://bioconductor.org/packages/3.18/bioc/html/demo.html",
            "https://bioconductor.org/packages/release/data/annotation/html/demo.html",
            "https://bioconductor.org/packages/3.18/data/experiment/html/demo.html",
            "https://r-universe.dev/api/search?q=package%3Ademo&limit=1",
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=stars&per_page=10",
            "https://raw.githubusercontent.com/owner/repo/HEAD/DESCRIPTION",
        ] {
            assert!(validate_search_request_url(url).is_ok(), "{url}");
        }

        for url in [
            "http://cloud.r-project.org/web/packages/demo/index.html",
            "https://user:pass@api.github.com/search/repositories?q=demo",
            "https://api.github.com:443/search/repositories?q=demo",
            "https://cloud.r-project.org:443/web/packages/demo/index.html",
            "https://api.github.com/search/repositories?q=demo#token",
            "https://example.com/search/repositories?q=demo",
            "https://raw.githubusercontent.com/owner/repo/HEAD/DESCRIPTION?token=secret",
            "https://cloud.r-project.org/web/packages/demo/index.html?mirror=evil",
            "https://cloud.r-project.org/web/packages/owner/repo/index.html",
            "https://cloud.r-project.org/web/packages/demo/extra/index.html",
            "https://bioconductor.org/packages/release/bioc/html/owner/repo.html",
            "https://bioconductor.org/packages/release/unknown/html/demo.html",
            "https://bioconductor.org/packages/release/data/unknown/html/demo.html",
            "https://bioconductor.org/packages/release/bioc/html/demo.html/extra",
            "https://r-universe.dev/api/search?q=package%3Ademo&limit=100",
            "https://r-universe.dev/api/search?limit=1&q=package%3Ademo",
            "https://r-universe.dev/api/search?q=package%3Ademo&q=package%3Aother&limit=1",
            "https://r-universe.dev/api/search?q=package%3A&limit=1",
            "https://r-universe.dev/api/search?q=owner%2Frepo&limit=1",
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=updated&per_page=10",
            "https://api.github.com/search/repositories?sort=stars&q=demo+language%3AR&per_page=10",
            "https://api.github.com/search/repositories?q=demo+language%3AR&q=other+language%3AR&sort=stars&per_page=10",
            "https://api.github.com/search/repositories?q=+language%3AR&sort=stars&per_page=10",
            "https://api.github.com/search/repositories?q=owner%2Frepo+language%3AR&sort=stars&per_page=10",
            "https://raw.githubusercontent.com/owner/repo/feature/DESCRIPTION",
            "https://raw.githubusercontent.com/owner/repo/HEAD/path/DESCRIPTION",
        ] {
            assert!(validate_search_request_url(url).is_err(), "{url}");
        }
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
        assert!(extract_description_metadata("Version: 1.0.0\n").is_none());
        assert!(extract_description_metadata("Package: demo\nVersion: 1.0.0<script>\n").is_none());
        assert!(extract_description_metadata("Package: demo\nVersion: 1.0.0\n").is_some());
    }

    #[test]
    fn validates_github_description_package_identity() {
        assert!(github_package_name_matches_request("Demo", "demo"));
        assert!(!github_package_name_matches_request("demoExtra", "demo"));
        assert!(!github_package_name_matches_request("demo\nbad", "demo"));
        assert!(extract_description_metadata("Package: demo\nVersion: 1.2.3\n").is_some());
        assert!(extract_description_metadata("Package: demo\nbad\nVersion: 1.2.3\n").is_none());
    }

    #[test]
    fn finds_package_object_only_at_expected_response_depth() {
        let top_level = serde_json::json!({
            "Package": "demo",
            "Version": "1.0.0"
        });
        let array_response = serde_json::json!([
            {
                "Package": "demo",
                "Version": "1.0.0"
            }
        ]);
        let nested_response = serde_json::json!({
            "meta": {
                "Package": "wrong",
                "Version": "9.9.9"
            }
        });

        assert_eq!(
            find_package_object(&top_level).and_then(|object| object.get("Package")),
            Some(&serde_json::json!("demo"))
        );
        assert_eq!(
            find_package_object(&array_response).and_then(|object| object.get("Package")),
            Some(&serde_json::json!("demo"))
        );
        assert!(find_package_object(&nested_response).is_none());
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
    fn sanitizes_progress_results_before_emit() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "demo".to_string(),
            requested_version: "1.2.3\nbad".to_string(),
            latest_version: "9".repeat(65),
            repository: "https://example.com/owner/demo".to_string(),
            real_name: "demo\nbad".to_string(),
            source: "github<script>".to_string(),
            found: true,
            message: format!("ok\n{}", "x".repeat(MAX_RESULT_MESSAGE_CHARS + 20)),
        });

        assert_eq!(result.package, "demo");
        assert!(result.requested_version.is_empty());
        assert!(result.latest_version.is_empty());
        assert!(result.repository.is_empty());
        assert_eq!(result.real_name, "demo");
        assert_eq!(result.source, "none");
        assert!(!result.message.contains('\n'));
        assert!(result.message.len() <= MAX_RESULT_MESSAGE_CHARS);
    }

    #[test]
    fn sanitizes_search_log_messages() {
        let message = sanitize_log_message(&format!(
            " ok\nbad\t{} ",
            "x".repeat(MAX_SEARCH_LOG_CHARS + 20)
        ));

        assert!(!message.contains('\n'));
        assert!(!message.contains('\t'));
        assert!(message.starts_with("okbad"));
        assert_eq!(message.chars().count(), MAX_SEARCH_LOG_CHARS);
        assert_eq!(sanitize_log_message("\n\t"), SEARCH_LOG_EMPTY_MESSAGE);
    }

    #[test]
    fn bounds_search_log_count() {
        let mut logs = Vec::new();

        for index in 0..(MAX_SEARCH_LOGS + 10) {
            let emitted = append_search_log(&mut logs, &format!("log {index}"));
            if index < MAX_SEARCH_LOGS {
                assert!(emitted.is_some());
            } else {
                assert!(emitted.is_none());
            }
        }

        assert_eq!(logs.len(), MAX_SEARCH_LOGS);
        assert_eq!(
            logs.last().map(String::as_str),
            Some(SEARCH_LOGS_TRUNCATED_MESSAGE)
        );
    }

    #[test]
    fn preserves_bioc_git_repository_version_in_results() {
        let package = PackageInput {
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: String::new(),
        };

        let result = found_result(&package, "1.2.3", "3.18", "demo", "biocGit");

        assert_eq!(result.repository, "3.18");
        assert_eq!(result.source, "biocGit");
    }

    #[test]
    fn invalid_github_real_name_downgrades_result() {
        let package = PackageInput {
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: String::new(),
        };

        let result = found_result(&package, "1.2.3", "owner/demo", "demo\nbad", "github");

        assert!(!result.found);
        assert_eq!(result.source, "none");
        assert!(result.repository.is_empty());
        assert_eq!(result.real_name, "demo");
    }

    #[test]
    fn request_budget_rejects_after_limit() {
        let budget = RequestBudget::new(2);

        assert!(budget.try_acquire().is_ok());
        assert!(budget.try_acquire().is_ok());
        assert!(budget.try_acquire().is_err());
        assert!(budget.is_exhausted());
        assert_eq!(budget.remaining_for_test(), 0);
    }

    #[test]
    fn search_stops_when_cancelled_or_budget_exhausted() {
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(1);

        assert!(!search_stopped(&cancelled, &budget));
        assert!(budget.try_acquire().is_ok());
        assert!(!search_stopped(&cancelled, &budget));
        assert!(budget.try_acquire().is_err());
        assert!(search_stopped(&cancelled, &budget));

        let fresh_budget = RequestBudget::new(1);
        cancelled.store(true, Ordering::SeqCst);
        assert!(search_stopped(&cancelled, &fresh_budget));
    }
}
