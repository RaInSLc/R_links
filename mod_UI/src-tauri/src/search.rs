use futures_util::future::join_all;
use regex::Regex;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use url::Url;

use crate::logic::{infer_bioc_version, normalize_github_repository, parse_inputs_filtered};
use crate::models::{
    InputRules, PackageCacheEntry, PackageInput, SearchResponse, SearchResult, Settings, MAX_FIELD_CHARS,
    MAX_PACKAGE_LINES,
};
use crate::storage;

const BIOC_VERSIONS: &[&str] = &[
    "3.23", "3.22", "3.21", "3.20", "3.19", "3.18", "3.17", "3.16", "3.15", "3.14", "3.13", "3.12",
    "3.11", "3.10", "3.9", "3.8", "3.7", "3.6", "3.5", "3.4", "3.3", "3.2", "3.1", "3.0",
];
const BIOC_CATEGORIES: &[&str] = &["bioc", "data/annotation", "data/experiment", "workflows"];
const MAX_TEXT_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_DESCRIPTION_BYTES: usize = 64 * 1024;
const MAX_DESCRIPTION_LINES: usize = 1_000;
const MAX_DESCRIPTION_LINE_CHARS: usize = 2_048;
const MAX_JSON_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_GITHUB_SEARCH_ITEMS: usize = 10;
const MAX_GITHUB_REPOSITORY_CHARS: usize = 200;
const MAX_SEARCH_HTTP_REQUESTS: usize = 200;
const MAX_SEARCH_DURATION: Duration = Duration::from_secs(300);
const MAX_CONCURRENT_PACKAGES: usize = 6;
const MAX_SEARCH_RESULTS: usize = MAX_PACKAGE_LINES * 16;
const MAX_SEARCH_LOGS: usize = 1_000;
const SEARCH_STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);
const SEARCH_STOPPED_ERROR: &str = "检索已停止";
const SEARCH_LOGS_TRUNCATED_MESSAGE: &str = "检索日志达到上限，后续日志已停止记录";
const SEARCH_RESULTS_TRUNCATED_MESSAGE: &str = "检索结果达到上限，后续来源请求已停止";
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
pub struct SearchLogBatchEvent {
    pub run_id: u64,
    pub messages: Vec<String>,
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
    deadline: Instant,
    timed_out: &'a AtomicBool,
    logs: &'a mut Vec<String>,
    result_limit_reached: bool,
    github_rate_limited: bool,
}

impl SearchContext<'_> {
    fn is_stopped(&self) -> bool {
        self.result_limit_reached || search_stopped(self.cancelled, self.budget)
    }

    fn is_expired(&self) -> bool {
        if self.timed_out.load(Ordering::SeqCst) {
            return true;
        }
        if Instant::now() >= self.deadline {
            self.timed_out.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    fn should_stop(&self) -> bool {
        self.is_stopped() || self.is_expired()
    }

    fn log(&mut self, message: &str) {
        if append_search_log(self.logs, message).is_some() {
            // Note: We don't emit immediately here to avoid duplicate/frequent emits.
            // They will be collected and emitted as a batch in `search_packages`.
        }
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

async fn wait_until_search_stopped(cancelled: &AtomicBool, budget: &RequestBudget) {
    while !search_stopped(cancelled, budget) {
        tokio::time::sleep(SEARCH_STOP_POLL_INTERVAL).await;
    }
}

async fn await_or_stop<T>(
    future: impl Future<Output = T>,
    cancelled: &AtomicBool,
    budget: &RequestBudget,
    deadline: Instant,
) -> Result<T, String> {
    if search_stopped(cancelled, budget) {
        return Err(SEARCH_STOPPED_ERROR.to_string());
    }

    tokio::select! {
        biased;
        _ = wait_until_search_stopped(cancelled, budget) => Err(SEARCH_STOPPED_ERROR.to_string()),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err("检索任务超时".to_string()),
        result = future => Ok(result),
    }
}

async fn send_request(
    context: &SearchContext<'_>,
    request: RequestBuilder,
) -> Result<reqwest::Response, String> {
    await_or_stop(
        request.send(),
        context.cancelled,
        context.budget,
        context.deadline,
    )
    .await?
    .map_err(|error| error.to_string())
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
    let rules = if settings.use_filter {
        storage::load_input_rules(app)
    } else {
        InputRules {
            separators: Vec::new(),
            strip_quotes: true,
            strip_c_parens: true,
            comment_chars: Vec::new(),
            split_spaces: false,
            exclude_regex: Vec::new(),
            exclude_keywords: Vec::new(),
        }
    };
    let packages = parse_inputs_filtered(input, &rules)?;
    if packages.is_empty() {
        return Err("请输入至少一个有效的 R 包".to_string());
    }

    let client = build_client(settings)?;
    let budget = RequestBudget::new(MAX_SEARCH_HTTP_REQUESTS);
    let timed_out = AtomicBool::new(false);
    let deadline = Instant::now() + MAX_SEARCH_DURATION;
    let mut results = Vec::new();
    let mut logs = Vec::new();
    let mut cache_update: HashMap<String, PackageCacheEntry> = HashMap::new();

    let cache = if settings.use_cache {
        match storage::load_cache(app) {
            Ok(cache) => cache,
            Err(error) => {
                log(app, run_id, &mut logs, &format!("缓存加载失败: {error}"));
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    log(
        app,
        run_id,
        &mut logs,
        &format!(
            "开始多源检索（超时 {} 秒，最大并发 {}）",
            MAX_SEARCH_DURATION.as_secs(),
            MAX_CONCURRENT_PACKAGES
        ),
    );

    let total = packages.len();
    let mut cache = cache;
    let mut processed = 0usize;

    while processed < packages.len() {
        if search_stopped(cancelled, &budget) || timed_out.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() >= deadline {
            timed_out.store(true, Ordering::SeqCst);
            break;
        }

        let batch_size = MAX_CONCURRENT_PACKAGES.min(packages.len() - processed);
        let batch = &packages[processed..processed + batch_size];
        let batch_start = processed;

        let mut batch_tasks = Vec::new();
        for (offset, package) in batch.iter().enumerate() {
            let index = batch_start + offset;
            let cache_key = package.name.to_ascii_lowercase();

            if let Some(cached_entry) = cache.get(&cache_key) {
                log(
                    app,
                    run_id,
                    &mut logs,
                    &format!("[{}/{}] {} (缓存命中)", index + 1, total, package.name),
                );
                results.push(SearchResult {
                    package: package.name.clone(),
                    requested_version: package.version.clone(),
                    latest_version: cached_entry.version.clone(),
                    repository: cached_entry.repository.clone(),
                    real_name: cached_entry.real_name.clone(),
                    source: cached_entry.source.clone(),
                    found: true,
                    message: "缓存命中".to_string(),
                    status: "found".to_string(),
                });
                let _ = app.emit(
                    "search-progress",
                    SearchProgressEvent {
                        run_id,
                        result: results.last().unwrap().clone(),
                    },
                );
                continue;
            }

            batch_tasks.push((index, package.clone()));
        }

        if batch_tasks.is_empty() {
            processed += batch_size;
            continue;
        }

        let futures: Vec<_> = batch_tasks
            .iter()
            .map(|(index, package)| {
                let pkg = package.clone();
                let client_ref = &client;
                let settings_ref = settings;
                let cancelled_ref = cancelled;
                let budget_ref = &budget;
                let timed_out_ref = &timed_out;
                async move {
                    let mut task_logs = Vec::new();
                    let mut task_results = Vec::new();
                    let mut context = SearchContext {
                        client: client_ref,
                        settings: settings_ref,
                        cancelled: cancelled_ref,
                        budget: budget_ref,
                        deadline,
                        timed_out: timed_out_ref,
                        logs: &mut task_logs,
                        result_limit_reached: false,
                        github_rate_limited: false,
                    };
                    search_one_package(&mut context, &mut task_results, &pkg, *index, total).await;
                    (task_results, task_logs)
                }
            })
            .collect();

        let outputs = join_all(futures).await;

        let mut task_results = Vec::new();
        let mut batch_new_logs = Vec::new();
        for (task_results_inner, task_logs) in outputs {
            for msg in &task_logs {
                if let Some(msg) = append_search_log(&mut logs, msg) {
                    batch_new_logs.push(msg);
                }
            }

            for result in &task_results_inner {
                let result_key = result.package.to_ascii_lowercase();
                if result.found
                    && !cache.contains_key(&result_key)
                    && matches!(
                        result.source.as_str(),
                        "cran" | "bioc" | "biocGit" | "github"
                    )
                {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .to_string();
                    cache_update.insert(
                        result_key,
                        PackageCacheEntry {
                            package_name: result.real_name.clone(),
                            source: result.source.clone(),
                            version: result.latest_version.clone(),
                            repository: result.repository.clone(),
                            real_name: result.real_name.clone(),
                            cached_at: now,
                        },
                    );
                }
            }

            task_results.extend(task_results_inner);
        }

        for result in &task_results {
            if results.len() >= MAX_SEARCH_RESULTS {
                log(app, run_id, &mut logs, SEARCH_RESULTS_TRUNCATED_MESSAGE);
                break;
            }
            let sanitized = sanitize_search_result_for_emit(result.clone());
            results.push(sanitized.clone());
            let _ = app.emit(
                "search-progress",
                SearchProgressEvent {
                    run_id,
                    result: sanitized,
                },
            );
        }

        if !batch_new_logs.is_empty() {
            let _ = app.emit(
                "search-log-batch",
                SearchLogBatchEvent {
                    run_id,
                    messages: batch_new_logs,
                },
            );
        }

        processed += batch_size;
    }

    for (key, entry) in &cache_update {
        cache.insert(key.clone(), entry.clone());
    }

    let final_message = if timed_out.load(Ordering::SeqCst) {
        "检索任务已超时停止"
    } else if search_stopped(cancelled, &budget) {
        "检索任务已停止"
    } else {
        "检索任务已完成"
    };
    log(app, run_id, &mut logs, final_message);

    if settings.use_cache {
        if let Err(error) = storage::save_cache(app, &cache) {
            log(app, run_id, &mut logs, &format!("缓存保存失败: {error}"));
        }
    }
    Ok(SearchResponse {
        run_id,
        results,
        logs,
        stopped: timed_out.load(Ordering::SeqCst) || search_stopped(cancelled, &budget),
    })
}

async fn search_one_package(
    context: &mut SearchContext<'_>,
    results: &mut Vec<SearchResult>,
    package: &PackageInput,
    index: usize,
    total: usize,
) {
    let mut loop_package = package.clone();
    let mut has_retried_casing = false;
    let mut errors = Vec::new();

    loop {
        if context.should_stop() {
            break;
        }

        context.log(&format!(
            "[{}/{}] 检索 {}{}",
            index + 1,
            total,
            loop_package.name,
            if loop_package.version.is_empty() {
                String::new()
            } else {
                format!(" {}", loop_package.version)
            }
        ));

        let had_found_before = has_found_result_for_package(results, &loop_package.name);
        if loop_package.name.contains('/') {
            match search_explicit_github(context, &loop_package).await {
                Ok(Some(result)) => {
                    results.push(result);
                }
                Ok(None) => {}
                Err(error) => {
                    errors.push(format!("GitHub仓库验证失败: {error}"));
                }
            }
        } else {
            match search_cran(context, &loop_package).await {
                Ok(Some(result)) => {
                    results.push(result);
                }
                Ok(None) => {}
                Err(error) => {
                    errors.push(format!("CRAN 检索失败: {error}"));
                }
            }

            if (context.settings.full_search
                || !has_found_result_for_package(results, &loop_package.name))
                && !context.should_stop()
            {
                match search_bioconductor(context, &loop_package).await {
                    Ok(bioc_results) => {
                        for result in bioc_results {
                            results.push(result);
                        }
                    }
                    Err(error) => {
                        errors.push(format!("Bioconductor 检索失败: {error}"));
                    }
                }
            }

            if (context.settings.full_search
                || !has_found_result_for_package(results, &loop_package.name))
                && !context.should_stop()
            {
                match search_github(context, &loop_package).await {
                    Ok(github_results) => {
                        let mut casing_diff_name = None;
                        if !has_retried_casing {
                            for res in &github_results {
                                if res.found
                                    && !res.real_name.is_empty()
                                    && res.real_name != loop_package.name
                                    && res.real_name.eq_ignore_ascii_case(&loop_package.name)
                                {
                                    casing_diff_name = Some(res.real_name.clone());
                                    break;
                                }
                            }
                        }

                        if let Some(corrected_name) = casing_diff_name {
                            context.log(&format!(
                                "检测到包名大小写差异，纠正为: {}，重新进行检索...",
                                corrected_name
                            ));
                            results.retain(|r| !r.package.eq_ignore_ascii_case(&loop_package.name));

                            loop_package.name = corrected_name;
                            has_retried_casing = true;
                            continue;
                        }

                        for result in github_results {
                            results.push(result);
                        }
                    }
                    Err(error) => {
                        errors.push(format!("GitHub 检索失败: {error}"));
                    }
                }
            }
        }

        if !had_found_before
            && !has_found_result_for_package(results, &loop_package.name)
            && !context.should_stop()
        {
            let (message, status) = if context.timed_out.load(Ordering::SeqCst) {
                ("检索超时，部分来源未查询".to_string(), "timeout")
            } else if context.github_rate_limited {
                (
                    "GitHub API 频率限制，部分来源未查询".to_string(),
                    "rateLimited",
                )
            } else if !errors.is_empty() {
                (errors.join("; "), "error")
            } else {
                ("所有来源均未找到".to_string(), "notFound")
            };
            results.push(SearchResult {
                package: loop_package.name.clone(),
                requested_version: loop_package.version.clone(),
                latest_version: String::new(),
                repository: String::new(),
                real_name: loop_package.name.clone(),
                source: "none".to_string(),
                found: false,
                message,
                status: status.to_string(),
            });
        }

        break;
    }
}

fn has_found_result_for_package(results: &[SearchResult], package_name: &str) -> bool {
    results.iter().any(|result| {
        result.found
            && (result.package.eq_ignore_ascii_case(package_name)
                || normalize_github_repository(package_name)
                    .as_deref()
                    .is_some_and(|repository| result.package.eq_ignore_ascii_case(repository)))
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

fn parse_version(v: &str) -> Vec<i32> {
    v.split(['.', '-', '_'])
        .filter_map(|s| s.parse::<i32>().ok())
        .collect()
}

fn compare_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
    parse_version(v1).cmp(&parse_version(v2))
}

fn extract_archive_versions(html: &str, package_name: &str) -> Vec<String> {
    let pattern = format!(
        r#"{}_([0-9A-Za-z.-]+)\.tar\.gz"#,
        regex::escape(package_name)
    );
    let Ok(re) = Regex::new(&pattern) else {
        return Vec::new();
    };
    let mut versions = Vec::new();
    for cap in re.captures_iter(html) {
        if let Some(version) = cap.get(1) {
            versions.push(version.as_str().to_string());
        }
    }
    versions
}

async fn search_cran(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Option<SearchResult>, String> {
    let url = format!(
        "https://cloud.r-project.org/web/packages/{}/index.html",
        urlencoding::encode(&package.name)
    );
    let html = get_text(context, &url).await?;
    let version = html.as_deref().and_then(extract_html_version);

    if let Some(version) = version {
        context.log(&format!("CRAN 命中版本 {version}"));
        return Ok(Some(found_result(
            package,
            &version,
            "",
            &package.name,
            "cran",
        )));
    }

    // 如果主页请求失败（404），或者主页中无法提取出版本号（例如包已被移出 CRAN 官方主页并归档）
    // 尝试从 CRAN Archive 归档区寻找包的历史版本
    let archive_url = format!(
        "https://cloud.r-project.org/src/contrib/Archive/{}/",
        urlencoding::encode(&package.name)
    );
    context.log(&format!(
        "CRAN 主页未找到包 {} 的有效版本，尝试检索 Archive 归档...",
        package.name
    ));
    match get_text(context, &archive_url).await? {
        Some(archive_html) => {
            let versions = extract_archive_versions(&archive_html, &package.name);
            if versions.is_empty() {
                return Ok(None);
            }
            let mut latest_version = versions[0].clone();
            for v in &versions {
                if compare_versions(v, &latest_version) == std::cmp::Ordering::Greater {
                    latest_version = v.clone();
                }
            }

            let target_version = if !package.version.is_empty() {
                if versions
                    .iter()
                    .any(|v| version_compatible(v, &package.version))
                {
                    let matched = versions
                        .iter()
                        .find(|v| version_compatible(v, &package.version))
                        .unwrap();
                    matched.clone()
                } else {
                    return Ok(None);
                }
            } else {
                latest_version
            };

            context.log(&format!("CRAN Archive 命中归档版本 {target_version}"));
            return Ok(Some(found_result(
                package,
                &target_version,
                "archive",
                &package.name,
                "cran",
            )));
        }
        None => Ok(None),
    }
}

async fn search_bioconductor(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Vec<SearchResult>, String> {
    for category in BIOC_CATEGORIES {
        if context.should_stop() {
            return Ok(Vec::new());
        }
        let release_url = format!(
            "https://bioconductor.org/packages/release/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        match get_text(context, &release_url).await {
            Ok(Some(html)) => {
                if let Some(release_version) = extract_html_version(&html) {
                    if !package.version.is_empty()
                        && !version_compatible(&release_version, &package.version)
                    {
                        if let Some(history) = find_bioc_history(context, package, category).await?
                        {
                            return Ok(vec![history]);
                        }
                    }
                    context.log(&format!("Bioconductor Release 命中版本 {release_version}"));
                    return Ok(vec![found_result(
                        package,
                        &release_version,
                        "",
                        &package.name,
                        "bioc",
                    )]);
                }
            }
            Ok(None) => {}
            Err(error) => {
                return Err(error);
            }
        }
    }
    Ok(Vec::new())
}

async fn find_bioc_history(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
    category: &str,
) -> Result<Option<SearchResult>, String> {
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
        if context.should_stop() {
            return Ok(None);
        }
        let url = format!(
            "https://bioconductor.org/packages/{bioc_version}/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        match get_text(context, &url).await {
            Ok(Some(html)) => {
                if let Some(version) = extract_html_version(&html) {
                    if version_compatible(&version, &package.version) {
                        context.log(&format!("Bioconductor {bioc_version} 匹配版本 {version}"));
                        return Ok(Some(found_result(
                            package,
                            &version,
                            bioc_version,
                            &package.name,
                            "biocGit",
                        )));
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                return Err(error);
            }
        }
    }
    Ok(None)
}

async fn search_explicit_github(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Option<SearchResult>, String> {
    context.log("验证指定 GitHub 仓库");
    let Some(repository) = normalize_github_repository(&package.name) else {
        context.log("GitHub 仓库格式无效，已跳过");
        return Ok(None);
    };
    let description = match github_description(context, &repository).await {
        Ok(Some(desc)) => desc,
        Ok(None) => {
            let package_name = repository
                .rsplit('/')
                .next()
                .unwrap_or(&repository)
                .to_string();
            GithubDescription {
                package_name,
                version: "unknown".to_string(),
            }
        }
        Err(error) => {
            return Err(error);
        }
    };
    Ok(Some(found_result(
        package,
        &description.version,
        &repository,
        &description.package_name,
        "github",
    )))
}

async fn search_github(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let universe_url = format!(
        "https://r-universe.dev/api/search?q=package:{}&limit=1",
        urlencoding::encode(&package.name)
    );
    let universe_res = get_json(context, &universe_url).await;
    if let Ok(Some(value)) = universe_res {
        if let Some(object) = r_universe_package_object(&value) {
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
    } else if let Err(error) = universe_res {
        context.log(&format!("R-Universe 检索异常: {error}"));
    }

    if !results.is_empty() && !context.settings.full_search {
        return Ok(results);
    }

    let url = format!(
        "https://api.github.com/search/repositories?q={}+language:R&sort=stars&per_page=10",
        urlencoding::encode(&package.name)
    );
    let request = authorized_get(context.client, &url, context.settings)?;
    if !context.acquire_request_budget() {
        return Ok(results);
    }
    let response = send_request(context, request).await?;
    if response.status() == StatusCode::FORBIDDEN {
        context.log("GitHub API 已触发频率限制（rateLimited）");
        context.github_rate_limited = true;
        return Ok(results);
    }
    if !response.status().is_success() {
        let err_msg = format!("GitHub API 返回 HTTP {}", response.status().as_u16());
        context.log(&err_msg);
        return Err(err_msg);
    }
    let text = read_limited_text(
        response,
        MAX_JSON_RESPONSE_BYTES,
        context.cancelled,
        context.budget,
        context.deadline,
    )
    .await?;
    let body = serde_json::from_str::<GithubSearchResponse>(&text)
        .map_err(|e| format!("GitHub 响应解析失败: {e}"))?;

    for full_name in bounded_github_response_repositories(body) {
        if context.is_stopped() {
            break;
        }
        let repo_name = full_name.rsplit('/').next().unwrap_or_default();
        let lower_repo = repo_name.to_ascii_lowercase();
        let lower_package = package.name.to_ascii_lowercase();
        if !lower_repo.contains(&lower_package) || seen.contains(&full_name.to_ascii_lowercase()) {
            continue;
        }
        if let Some(repository_name) = normalize_github_repository(&full_name) {
            match github_description(context, &repository_name).await {
                Ok(Some(description)) => {
                    if !github_package_name_matches_request(
                        &description.package_name,
                        &package.name,
                    ) {
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
                Ok(None) => {
                    if lower_repo == lower_package {
                        // 兜底逻辑：如果拿不到 DESCRIPTION（比如 mono-repo），但仓库名精确匹配请求包名，则信任该结果
                        seen.insert(repository_name.to_ascii_lowercase());
                        results.push(found_result(
                            package,
                            "unknown",
                            &repository_name,
                            repo_name,
                            "github",
                        ));
                    }
                }
                Err(error) => {
                    context.log(&format!("获取 GitHub DESCRIPTION 异常: {error}"));
                }
            }
        }
    }
    Ok(results)
}

async fn github_description(
    context: &mut SearchContext<'_>,
    repository: &str,
) -> Result<Option<GithubDescription>, String> {
    let mut last_error = None;
    for branch in ["HEAD", "master", "main", "devel"] {
        if context.is_stopped() {
            return Ok(None);
        }
        let url = format!("https://raw.githubusercontent.com/{repository}/{branch}/DESCRIPTION");
        let request = match authorized_get(context.client, &url, context.settings) {
            Ok(request) => request,
            Err(error) => {
                context.log(&error);
                return Err(error);
            }
        };
        if !context.acquire_request_budget() {
            return Ok(None);
        }
        match send_request(context, request).await {
            Ok(response) => {
                if response.status() == StatusCode::NOT_FOUND {
                    continue;
                }
                if !response.status().is_success() {
                    let err = format!("HTTP 错误: {}", response.status());
                    context.log(&err);
                    last_error = Some(err);
                    continue;
                }
                let description_text = match read_limited_text(
                    response,
                    MAX_DESCRIPTION_BYTES,
                    context.cancelled,
                    context.budget,
                    context.deadline,
                )
                .await
                {
                    Ok(text) => text,
                    Err(e) => {
                        context.log(&e);
                        last_error = Some(e);
                        continue;
                    }
                };
                if let Some(description) = extract_description_metadata(&description_text) {
                    return Ok(Some(description));
                }
            }
            Err(error) => {
                if !context.is_stopped() {
                    context.log(&format!("GitHub DESCRIPTION 请求失败: {error}"));
                }
                last_error = Some(error);
            }
        }
    }
    if let Some(err) = last_error {
        Err(err)
    } else {
        Ok(None)
    }
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

async fn get_text(context: &mut SearchContext<'_>, url: &str) -> Result<Option<String>, String> {
    if context.is_stopped() {
        return Ok(None);
    }
    if let Err(error) = validate_search_request_url(url) {
        context.log(&error);
        return Err(error);
    }
    if !context.acquire_request_budget() {
        return Ok(None);
    }
    let response = send_request(context, context.client.get(url)).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("HTTP错误: {}", response.status()));
    }
    let text = read_limited_text(
        response,
        MAX_TEXT_RESPONSE_BYTES,
        context.cancelled,
        context.budget,
        context.deadline,
    )
    .await?;
    Ok(Some(text))
}

async fn get_json(context: &mut SearchContext<'_>, url: &str) -> Result<Option<Value>, String> {
    if context.is_stopped() {
        return Ok(None);
    }
    let request = match authorized_get(context.client, url, context.settings) {
        Ok(request) => request,
        Err(error) => {
            context.log(&error);
            return Err(error);
        }
    };
    if !context.acquire_request_budget() {
        return Ok(None);
    }
    let response = send_request(context, request).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("HTTP错误: {}", response.status()));
    }
    let text = read_limited_text(
        response,
        MAX_JSON_RESPONSE_BYTES,
        context.cancelled,
        context.budget,
        context.deadline,
    )
    .await?;
    serde_json::from_str(&text)
        .map_err(|e| format!("JSON解析失败: {}", e))
        .map(Some)
}

async fn read_limited_text(
    mut response: reqwest::Response,
    limit: usize,
    cancelled: &AtomicBool,
    budget: &RequestBudget,
    deadline: Instant,
) -> Result<String, String> {
    if let Some(length) = response.content_length() {
        if length > limit as u64 {
            return Err("响应内容超过大小限制".to_string());
        }
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = await_or_stop(response.chunk(), cancelled, budget, deadline)
        .await?
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

use crate::search_sanitize::{
    clean_result_package_name, clean_result_real_name, clean_result_repository,
    clean_result_source, clean_version, sanitize_log_message, sanitize_search_result_for_emit,
};
use crate::search_urls::validate_search_request_url;

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

    for (line_index, raw_line) in description.lines().enumerate() {
        if line_index >= MAX_DESCRIPTION_LINES || raw_line.len() > MAX_DESCRIPTION_LINE_CHARS {
            return None;
        }
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

fn r_universe_package_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    match value {
        Value::Object(object) if r_universe_object_has_bounded_fields(object) => Some(object),
        Value::Array(values) => values.first().and_then(|item| match item {
            Value::Object(object) if r_universe_object_has_bounded_fields(object) => Some(object),
            _ => None,
        }),
        _ => None,
    }
}

fn r_universe_object_has_bounded_fields(object: &serde_json::Map<String, Value>) -> bool {
    ["Package", "Version", "RemoteUrl"].iter().all(|field| {
        object
            .get(*field)
            .and_then(Value::as_str)
            .is_some_and(|value| {
                value.len() <= MAX_FIELD_CHARS && !value.chars().any(char::is_control)
            })
    })
}

fn clean_github_response_repository_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() > MAX_GITHUB_REPOSITORY_CHARS
        || trimmed.chars().any(|character| character.is_control())
    {
        return None;
    }
    normalize_github_repository(trimmed).map(|_| trimmed.to_string())
}

fn bounded_github_response_repositories(body: GithubSearchResponse) -> Vec<String> {
    body.items
        .into_iter()
        .take(MAX_GITHUB_SEARCH_ITEMS)
        .filter_map(|repository| clean_github_response_repository_name(&repository.full_name))
        .collect()
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
            status: "notFound".to_string(),
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
        status: "found".to_string(),
    }
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
        let _ = app.emit(
            "search-log-batch",
            SearchLogBatchEvent {
                run_id,
                messages: vec![message],
            },
        );
    }
}

#[cfg(test)]
fn append_bounded_search_result(
    results: &mut Vec<SearchResult>,
    result: SearchResult,
    limit: usize,
) -> bool {
    if results.len() >= limit {
        return false;
    }
    results.push(result);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_sanitize::{
        MAX_RESULT_MESSAGE_CHARS, MAX_SEARCH_LOG_CHARS, SEARCH_LOG_EMPTY_MESSAGE,
    };

    #[test]
    fn builds_clients_for_supported_proxy_schemes() {
        for proxy in [
            "http://127.0.0.1:7890",
            "https://127.0.0.1:7890",
            "socks5://127.0.0.1:1080",
            "socks5h://127.0.0.1:1080",
        ] {
            let settings = Settings {
                proxy: proxy.to_string(),
                ..Settings::default()
            }
            .normalized()
            .expect("supported proxy should normalize");

            assert!(
                build_client(&settings).is_ok(),
                "supported proxy should build a client: {proxy}"
            );
        }
    }

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
            "https://cloud.r-project.org/src/contrib/Archive/demo/",
            "https://cloud.r-project.org/src/contrib/Archive/demo",
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
            "https://cloud.r-project.org/src/contrib/Archive/demo/extra",
            "https://cloud.r-project.org/src/contrib/Archive/demo/index.html",
            "https://cloud.r-project.org/src/contrib/Archive/demo/?mirror=evil",
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
    fn bounds_github_search_response_repositories() {
        let mut items = vec![
            GithubRepository {
                full_name: "owner/demo".to_string(),
            },
            GithubRepository {
                full_name: "owner/bad\nrepo".to_string(),
            },
            GithubRepository {
                full_name: format!("owner/{}", "x".repeat(MAX_GITHUB_REPOSITORY_CHARS + 1)),
            },
        ];
        items.extend((0..MAX_GITHUB_SEARCH_ITEMS).map(|index| GithubRepository {
            full_name: format!("owner/repo{index}"),
        }));

        let repositories = bounded_github_response_repositories(GithubSearchResponse { items });

        assert_eq!(repositories.len(), MAX_GITHUB_SEARCH_ITEMS - 2);
        assert_eq!(repositories.first().map(String::as_str), Some("owner/demo"));
        assert!(!repositories
            .iter()
            .any(|repository| repository == "owner/repo9"));
        assert!(repositories
            .iter()
            .all(|repository| repository.len() <= MAX_GITHUB_REPOSITORY_CHARS));
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
    fn bounds_github_description_metadata_scan() {
        assert!(extract_description_metadata(
            "Package: demo\nTitle: Demo package\n  continuation is allowed\nVersion: 1.2.3\n"
        )
        .is_some());

        let too_many_lines = format!(
            "{}Package: demo\nVersion: 1.2.3\n",
            "Author: demo\n".repeat(MAX_DESCRIPTION_LINES)
        );
        assert!(extract_description_metadata(&too_many_lines).is_none());

        let oversized_line = format!(
            "Package: demo\nTitle: {}\nVersion: 1.2.3\n",
            "x".repeat(MAX_DESCRIPTION_LINE_CHARS + 1)
        );
        assert!(extract_description_metadata(&oversized_line).is_none());
    }

    #[test]
    fn bounds_r_universe_package_object_shape() {
        let top_level = serde_json::json!({
            "Package": "demo",
            "Version": "1.0.0",
            "RemoteUrl": "https://github.com/owner/demo"
        });
        let array_response = serde_json::json!([
            {
                "Package": "demo",
                "Version": "1.0.0",
                "RemoteUrl": "https://github.com/owner/demo"
            },
            {
                "Package": "other",
                "Version": "9.9.9",
                "RemoteUrl": "https://github.com/owner/other"
            }
        ]);
        let invalid_first_array_response = serde_json::json!([
            {
                "Package": 42,
                "Version": "1.0.0",
                "RemoteUrl": "https://github.com/owner/wrong"
            },
            {
                "Package": "demo",
                "Version": "1.0.0",
                "RemoteUrl": "https://github.com/owner/demo"
            }
        ]);
        let oversized_response = serde_json::json!({
            "Package": "demo",
            "Version": "1.0.0",
            "RemoteUrl": "x".repeat(MAX_FIELD_CHARS + 1)
        });
        let nested_response = serde_json::json!({
            "meta": {
                "Package": "wrong",
                "Version": "9.9.9",
                "RemoteUrl": "https://github.com/owner/wrong"
            }
        });

        assert_eq!(
            r_universe_package_object(&top_level).and_then(|object| object.get("Package")),
            Some(&serde_json::json!("demo"))
        );
        assert_eq!(
            r_universe_package_object(&array_response).and_then(|object| object.get("Package")),
            Some(&serde_json::json!("demo"))
        );
        assert!(r_universe_package_object(&invalid_first_array_response).is_none());
        assert!(r_universe_package_object(&oversized_response).is_none());
        assert!(r_universe_package_object(&nested_response).is_none());
    }

    #[test]
    fn serializes_search_events_with_run_id() {
        let event = SearchLogBatchEvent {
            run_id: 42,
            messages: vec!["开始".to_string()],
        };
        let encoded = serde_json::to_string(&event).expect("事件应可序列化");

        assert!(encoded.contains("\"runId\":42"));
        assert!(encoded.contains("\"messages\""));
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
            status: "found".to_string(),
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
    fn downgrades_untrusted_progress_results_before_emit() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "demo".to_string(),
            requested_version: String::new(),
            latest_version: "1.2.3".to_string(),
            repository: String::new(),
            real_name: "demo".to_string(),
            source: "github".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
        });

        assert!(!result.found);
        assert_eq!(result.source, "none");
        assert!(result.latest_version.is_empty());
        assert!(result.repository.is_empty());
        assert_eq!(result.message, "结果字段无效，已忽略");
    }

    #[test]
    fn preserves_explicit_github_progress_result_identity() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "owner/repo".to_string(),
            requested_version: String::new(),
            latest_version: "1.2.3".to_string(),
            repository: "owner/repo".to_string(),
            real_name: "actualPkg".to_string(),
            source: "github".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
        });

        assert!(result.found);
        assert_eq!(result.source, "github");
        assert_eq!(result.repository, "owner/repo");
        assert_eq!(result.real_name, "actualPkg");
    }

    #[test]
    fn found_result_tracking_ignores_downgraded_or_unrelated_results() {
        let results = vec![
            SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: String::new(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "none".to_string(),
                found: false,
                message: "未找到".to_string(),
                status: "found".to_string(),
            },
            SearchResult {
                package: "other".to_string(),
                requested_version: String::new(),
                latest_version: "1.0.0".to_string(),
                repository: String::new(),
                real_name: "other".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            },
        ];

        assert!(!has_found_result_for_package(&results, "demo"));
        assert!(has_found_result_for_package(&results, "other"));
    }

    #[test]
    fn found_result_tracking_accepts_explicit_github_package_identity() {
        let results = vec![SearchResult {
            package: "owner/repo".to_string(),
            requested_version: String::new(),
            latest_version: "1.0.0".to_string(),
            repository: "owner/repo".to_string(),
            real_name: "actualPkg".to_string(),
            source: "github".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
        }];

        assert!(has_found_result_for_package(&results, "Owner/Repo"));
        assert!(!has_found_result_for_package(&results, "owner/other"));
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
    fn bounds_search_result_count_directly() {
        let package = PackageInput {
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: String::new(),
            source_hint: None,
        };
        let mut results = Vec::new();

        for index in 0..5 {
            let result = found_result(&package, &format!("1.0.{index}"), "", "demo", "cran");
            assert_eq!(
                append_bounded_search_result(&mut results, result, 3),
                index < 3
            );
        }

        assert_eq!(results.len(), 3);
        assert_eq!(results[2].latest_version, "1.0.2");
    }

    #[test]
    fn preserves_bioc_git_repository_version_in_results() {
        let package = PackageInput {
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: String::new(),
            source_hint: None,
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
            source_hint: None,
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
