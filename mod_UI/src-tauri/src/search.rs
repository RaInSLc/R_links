use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::logic::{infer_bioc_version, is_valid_github_repository, parse_inputs};
use crate::models::{PackageInput, SearchResponse, SearchResult, Settings};

const BIOC_VERSIONS: &[&str] = &[
    "3.23", "3.22", "3.21", "3.20", "3.19", "3.18", "3.17", "3.16", "3.15", "3.14", "3.13", "3.12",
    "3.11", "3.10", "3.9", "3.8", "3.7", "3.6", "3.5", "3.4", "3.3", "3.2", "3.1", "3.0",
];
const BIOC_CATEGORIES: &[&str] = &["bioc", "data/annotation", "data/experiment", "workflows"];
const MAX_TEXT_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_DESCRIPTION_BYTES: usize = 64 * 1024;
const MAX_JSON_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
struct GithubSearchResponse {
    items: Vec<GithubRepository>,
}

#[derive(Debug, Deserialize)]
struct GithubRepository {
    full_name: String,
}

pub async fn search_packages(
    app: &AppHandle,
    cancelled: &AtomicBool,
    input: &str,
    settings: &Settings,
) -> Result<SearchResponse, String> {
    let packages = parse_inputs(input)?;
    if packages.is_empty() {
        return Err("请输入至少一个有效的 R 包".to_string());
    }

    let client = build_client(settings)?;
    let mut results = Vec::new();
    let mut logs = Vec::new();

    log(app, &mut logs, "开始多源检索");
    for (index, package) in packages.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        log(
            app,
            &mut logs,
            &format!(
                "[{}/{}] 检索 {}{}",
                index + 1,
                packages.len(),
                package.name,
                if package.version.is_empty() {
                    String::new()
                } else {
                    format!(" {}", package.version)
                }
            ),
        );

        let before = results.len();
        if package.name.contains('/') {
            if let Some(result) =
                search_explicit_github(&client, package, settings, cancelled, app, &mut logs).await
            {
                push_result(app, &mut results, result);
            }
        } else {
            if let Some(result) = search_cran(&client, package, cancelled, app, &mut logs).await {
                push_result(app, &mut results, result);
            }

            if (settings.full_search || results.len() == before)
                && !cancelled.load(Ordering::SeqCst)
            {
                let bioc_results =
                    search_bioconductor(&client, package, cancelled, app, &mut logs).await;
                for result in bioc_results {
                    push_result(app, &mut results, result);
                }
            }

            if (settings.full_search || results.len() == before)
                && !cancelled.load(Ordering::SeqCst)
            {
                let github_results =
                    search_github(&client, package, settings, cancelled, app, &mut logs).await;
                for result in github_results {
                    push_result(app, &mut results, result);
                }
            }
        }

        if results.len() == before && !cancelled.load(Ordering::SeqCst) {
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
            push_result(app, &mut results, result);
        }
    }

    let stopped = cancelled.load(Ordering::SeqCst);
    log(
        app,
        &mut logs,
        if stopped {
            "检索任务已停止"
        } else {
            "检索任务已完成"
        },
    );
    Ok(SearchResponse {
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
    client: &Client,
    package: &PackageInput,
    cancelled: &AtomicBool,
    app: &AppHandle,
    logs: &mut Vec<String>,
) -> Option<SearchResult> {
    log(app, logs, "查询 CRAN");
    let url = format!(
        "https://cloud.r-project.org/web/packages/{}/index.html",
        urlencoding::encode(&package.name)
    );
    let html = get_text(client, &url, cancelled).await?;
    let version = extract_html_version(&html)?;
    log(app, logs, &format!("CRAN 命中版本 {version}"));
    Some(found_result(package, &version, "", &package.name, "cran"))
}

async fn search_bioconductor(
    client: &Client,
    package: &PackageInput,
    cancelled: &AtomicBool,
    app: &AppHandle,
    logs: &mut Vec<String>,
) -> Vec<SearchResult> {
    log(app, logs, "查询 Bioconductor");
    for category in BIOC_CATEGORIES {
        if cancelled.load(Ordering::SeqCst) {
            return Vec::new();
        }
        let release_url = format!(
            "https://bioconductor.org/packages/release/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        if let Some(html) = get_text(client, &release_url, cancelled).await {
            if let Some(release_version) = extract_html_version(&html) {
                if !package.version.is_empty()
                    && !version_compatible(&release_version, &package.version)
                {
                    if let Some(history) =
                        find_bioc_history(client, package, category, cancelled, app, logs).await
                    {
                        return vec![history];
                    }
                }
                log(
                    app,
                    logs,
                    &format!("Bioconductor Release 命中版本 {release_version}"),
                );
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
    client: &Client,
    package: &PackageInput,
    category: &str,
    cancelled: &AtomicBool,
    app: &AppHandle,
    logs: &mut Vec<String>,
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
        if cancelled.load(Ordering::SeqCst) {
            return None;
        }
        let url = format!(
            "https://bioconductor.org/packages/{bioc_version}/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        if let Some(html) = get_text(client, &url, cancelled).await {
            if let Some(version) = extract_html_version(&html) {
                if version_compatible(&version, &package.version) {
                    log(
                        app,
                        logs,
                        &format!("Bioconductor {bioc_version} 匹配版本 {version}"),
                    );
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
    client: &Client,
    package: &PackageInput,
    settings: &Settings,
    cancelled: &AtomicBool,
    app: &AppHandle,
    logs: &mut Vec<String>,
) -> Option<SearchResult> {
    log(app, logs, "验证指定 GitHub 仓库");
    if !is_valid_github_repository(&package.name) {
        log(app, logs, "GitHub 仓库格式无效，已跳过");
        return None;
    }
    let version = github_description_version(client, &package.name, settings, cancelled).await?;
    let real_name = package.name.rsplit('/').next().unwrap_or(&package.name);
    Some(found_result(
        package,
        &version,
        &package.name,
        real_name,
        "github",
    ))
}

async fn search_github(
    client: &Client,
    package: &PackageInput,
    settings: &Settings,
    cancelled: &AtomicBool,
    app: &AppHandle,
    logs: &mut Vec<String>,
) -> Vec<SearchResult> {
    log(app, logs, "查询 r-universe 与 GitHub");
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let universe_url = format!(
        "https://r-universe.dev/api/search?q=package:{}&limit=1",
        urlencoding::encode(&package.name)
    );
    if let Some(value) = get_json(client, &universe_url, settings, cancelled).await {
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
                .and_then(|url| url.split_once("github.com/").map(|(_, path)| path))
                .unwrap_or_default()
                .trim_end_matches(".git")
                .trim_end_matches('/');
            if !repository.is_empty() {
                seen.insert(repository.to_ascii_lowercase());
                results.push(found_result(
                    package, version, repository, real_name, "github",
                ));
            }
        }
    }

    if !results.is_empty() && !settings.full_search {
        return results;
    }

    let url = format!(
        "https://api.github.com/search/repositories?q={}+language:R&sort=stars&per_page=10",
        urlencoding::encode(&package.name)
    );
    let request = authorized_get(client, &url, settings);
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            log(app, logs, &format!("GitHub 请求失败: {error}"));
            return results;
        }
    };
    if response.status() == StatusCode::FORBIDDEN {
        log(app, logs, "GitHub API 已触发频率限制");
        return results;
    }
    let body = match response.json::<GithubSearchResponse>().await {
        Ok(value) => value,
        Err(error) => {
            log(app, logs, &format!("GitHub 响应解析失败: {error}"));
            return results;
        }
    };

    for repository in body.items {
        if cancelled.load(Ordering::SeqCst) {
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
        if is_valid_github_repository(&repository.full_name) {
            if let Some(version) =
                github_description_version(client, &repository.full_name, settings, cancelled).await
            {
                seen.insert(repository.full_name.to_ascii_lowercase());
                results.push(found_result(
                    package,
                    &version,
                    &repository.full_name,
                    repo_name,
                    "github",
                ));
            }
        }
    }
    results
}

async fn github_description_version(
    client: &Client,
    repository: &str,
    settings: &Settings,
    cancelled: &AtomicBool,
) -> Option<String> {
    for branch in ["HEAD", "master", "main", "devel"] {
        if cancelled.load(Ordering::SeqCst) {
            return None;
        }
        let url = format!("https://raw.githubusercontent.com/{repository}/{branch}/DESCRIPTION");
        let response = authorized_get(client, &url, settings).send().await.ok()?;
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
    if settings.github_token.trim().is_empty() {
        request
    } else {
        request.bearer_auth(settings.github_token.trim())
    }
}

async fn get_text(client: &Client, url: &str, cancelled: &AtomicBool) -> Option<String> {
    if cancelled.load(Ordering::SeqCst) {
        return None;
    }
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    read_limited_text(response, MAX_TEXT_RESPONSE_BYTES)
        .await
        .ok()
}

async fn get_json(
    client: &Client,
    url: &str,
    settings: &Settings,
    cancelled: &AtomicBool,
) -> Option<Value> {
    if cancelled.load(Ordering::SeqCst) {
        return None;
    }
    let response = authorized_get(client, url, settings).send().await.ok()?;
    let text = read_limited_text(response, MAX_JSON_RESPONSE_BYTES)
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}

async fn read_limited_text(response: reqwest::Response, limit: usize) -> Result<String, String> {
    if let Some(length) = response.content_length() {
        if length > limit as u64 {
            return Err("响应内容超过大小限制".to_string());
        }
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "读取响应失败".to_string())?;
    if bytes.len() > limit {
        return Err("响应内容超过大小限制".to_string());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "响应不是有效 UTF-8".to_string())
}

fn extract_html_version(html: &str) -> Option<String> {
    let regex = Regex::new(r"(?is)<td[^>]*>\s*Version[^<]*</td>\s*<td[^>]*>\s*([^<\s][^<]*)</td>")
        .expect("固定 HTML 版本正则必须有效");
    regex
        .captures(html)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn extract_description_version(description: &str) -> Option<String> {
    description.lines().find_map(|line| {
        line.strip_prefix("Version:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
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

fn log(app: &AppHandle, logs: &mut Vec<String>, message: &str) {
    logs.push(message.to_string());
    let _ = app.emit("search-log", message);
}

fn push_result(app: &AppHandle, results: &mut Vec<SearchResult>, result: SearchResult) {
    let _ = app.emit("search-progress", &result);
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
}
