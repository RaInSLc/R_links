use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_util::stream::StreamExt;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

use crate::models::{PackageCacheEntry, SearchResponse, SearchResult, Settings};
use crate::search::{
    build_client, cache_entry_from_result, SearchLogBatchEvent, SearchProgressEvent,
};
use crate::storage;
use crate::SearchState;

const MAX_MULTI_DURATION: Duration = Duration::from_secs(300);
const MAX_MULTI_HTTP_REQUESTS: usize = 200;
const MAX_MULTI_RESULTS: usize = 16_000;
const MAX_MULTI_LOGS: usize = 1_000;
const MULTI_STOP_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Deserialize)]
struct PypiInfo {
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PypiResponse {
    info: PypiInfo,
    #[serde(default)]
    releases: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CondaResponse {
    latest_version: Option<String>,
    versions: Option<Vec<String>>,
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn split_requirement(line: &str) -> (String, String) {
    let line = line.trim().trim_matches(['"', '\'']);
    if let Some(index) = line.find(['=', '>', '<', '!', '~']) {
        return (
            line[..index].trim().to_string(),
            line[index..].split_whitespace().collect(),
        );
    }
    (line.to_string(), String::new())
}

fn inputs(input: &str) -> Vec<(String, String)> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
        .map(split_requirement)
        .filter(|(name, _)| valid_name(name))
        .collect()
}

fn conda_version(payload: &CondaResponse, requested: &str) -> Option<String> {
    let mut versions = payload.versions.clone().unwrap_or_default();
    if let Some(latest) = payload
        .latest_version
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        versions.push(latest.to_string());
    }
    versions.sort_by(|left, right| {
        let left_parts = left.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
        let right_parts = right
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0));
        left_parts.cmp(right_parts)
    });
    versions.dedup();
    versions.reverse();

    versions
        .into_iter()
        .find(|version| requirement_matches(version, requested))
}

fn requirement_matches(version: &str, requested: &str) -> bool {
    if requested.is_empty() {
        return true;
    }
    let numeric = |value: &str| {
        value
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
    };
    let Ok(actual) = numeric(version) else {
        return false;
    };
    requested.split(',').all(|constraint| {
        let constraint = constraint.trim();
        let operator = ["==", "!=", ">=", "<=", "~=", ">", "<", "="]
            .into_iter()
            .find(|operator| constraint.starts_with(operator))
            .unwrap_or("=");
        let target = constraint.strip_prefix(operator).unwrap_or(constraint);
        let Ok(mut expected) = numeric(target) else {
            return false;
        };
        let original = expected.clone();
        let mut actual = actual.clone();
        let length = actual.len().max(expected.len());
        actual.resize(length, 0);
        expected.resize(length, 0);
        match operator {
            "==" => actual == expected,
            "!=" => actual != expected,
            ">=" => actual >= expected,
            "<=" => actual <= expected,
            ">" => actual > expected,
            "<" => actual < expected,
            "~=" => {
                actual >= expected
                    && original.len() >= 2
                    && actual[..original.len() - 1] == original[..original.len() - 1]
            }
            _ => actual.starts_with(&original),
        }
    })
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

    fn try_acquire(&self) -> bool {
        loop {
            let current = self.remaining.load(Ordering::SeqCst);
            if current == 0 {
                return false;
            }
            if self
                .remaining
                .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }

    fn is_exhausted(&self) -> bool {
        self.remaining.load(Ordering::SeqCst) == 0
    }
}

fn append_log(logs: &mut Vec<String>, message: &str) -> Option<String> {
    if logs.len() >= MAX_MULTI_LOGS {
        return None;
    }
    let msg = if logs.len() + 1 == MAX_MULTI_LOGS {
        "检索日志达到上限，后续日志已停止记录".to_string()
    } else {
        message.to_string()
    };
    logs.push(msg.clone());
    Some(msg)
}

fn emit_log(app: &AppHandle, run_id: u64, logs: &mut Vec<String>, message: &str) {
    if let Some(msg) = append_log(logs, message) {
        let _ = app.emit(
            "search-log-batch",
            SearchLogBatchEvent {
                run_id,
                messages: vec![msg],
            },
        );
    }
}

fn emit_progress(app: &AppHandle, run_id: u64, result: &SearchResult) {
    let _ = app.emit(
        "search-progress",
        SearchProgressEvent {
            run_id,
            result: result.clone(),
        },
    );
}

fn is_stopped(cancelled: &AtomicBool, budget: &RequestBudget) -> bool {
    cancelled.load(Ordering::SeqCst) || budget.is_exhausted()
}

#[allow(clippy::too_many_arguments)]
pub async fn search(
    app: &AppHandle,
    run_id: u64,
    cancelled: &AtomicBool,
    state: &SearchState,
    input: &str,
    ecosystem: &str,
    pip_index: &str,
    conda_channels: &[String],
    settings: &Settings,
) -> Result<SearchResponse, String> {
    if run_id == 0 {
        return Err("检索任务 ID 无效".to_string());
    }
    if !matches!(ecosystem, "pip" | "conda") {
        return Err("不支持的多生态类型".to_string());
    }
    if ecosystem == "conda" && input.contains("~=") {
        return Err("Conda 暂不支持 ~=，请使用 >= 和 < 范围".to_string());
    }

    let mut packages = inputs(input);
    if ecosystem == "pip" {
        for (_, requested) in &mut packages {
            if requested.starts_with('=') && !requested.starts_with("==") {
                requested.insert(0, '=');
            }
        }
    }
    let grammar = regex::Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]*(?:(?:==|!=|>=|<=|~=|>|<|=)[0-9]+(?:\.[0-9]+)*(?:,(?:==|!=|>=|<=|~=|>|<|=)[0-9]+(?:\.[0-9]+)*)*)?$").map_err(|error| error.to_string())?;
    for line in input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let normalized: String = line.trim_matches(['"', '\'']).split_whitespace().collect();
        if !grammar.is_match(&normalized) {
            return Err("包声明格式不支持：请使用包名及数字版本约束".to_string());
        }
    }
    if packages.is_empty() {
        return Err("请输入至少一个有效的包名".to_string());
    }

    let client = build_client(settings)?;
    let budget = RequestBudget::new(MAX_MULTI_HTTP_REQUESTS);
    let deadline = Instant::now() + MAX_MULTI_DURATION;
    let mut results = Vec::new();
    let mut logs = Vec::new();
    let total = packages.len();

    emit_log(
        app,
        run_id,
        &mut logs,
        &format!(
            "开始{}多源检索（超时 {} 秒，最大并发 {}）",
            if ecosystem == "pip" { " Pip" } else { " Conda" },
            MAX_MULTI_DURATION.as_secs(),
            settings.search_concurrency,
        ),
    );

    let cache = if settings.use_cache {
        match storage::load_cache(app) {
            Ok(cache) => cache,
            Err(error) => {
                emit_log(app, run_id, &mut logs, &format!("缓存加载失败: {error}"));
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };
    let mut cache = cache;
    let mut cache_update: HashMap<String, PackageCacheEntry> = HashMap::new();

    let concurrency = settings.search_concurrency.max(1);
    let mut processed = 0usize;
    let cache_index = crate::search::index_cache(&cache);

    while processed < packages.len() {
        while state.is_paused(run_id)
            && !cancelled.load(Ordering::SeqCst)
            && Instant::now() < deadline
        {
            sleep(MULTI_STOP_POLL).await;
        }
        if is_stopped(cancelled, &budget) || Instant::now() >= deadline {
            break;
        }

        let batch_size = packages.len() - processed;
        let batch = &packages[processed..processed + batch_size];
        let batch_start = processed;
        let mut batch_tasks = Vec::new();
        let cached_start = results.len();

        for (offset, (name, requested)) in batch.iter().enumerate() {
            if cancelled.load(Ordering::SeqCst) || Instant::now() >= deadline {
                break;
            }
            let index = batch_start + offset;
            if state.is_package_cancelled(run_id, name) {
                emit_log(
                    app,
                    run_id,
                    &mut logs,
                    &format!("[{}/{}] {} 已取消", index + 1, total, name),
                );
                continue;
            }

            if let Some(cached_entry) = cache_index
                .get(name)
                .into_iter()
                .flatten()
                .copied()
                .filter(|entry| entry.source == ecosystem && entry.package_name == *name)
                .filter(|entry| {
                    if ecosystem == "pip" {
                        entry.repository.trim_end_matches('/') == pip_index.trim_end_matches('/')
                    } else {
                        conda_channels.contains(&entry.repository)
                    }
                })
                .filter(|entry| requirement_matches(&entry.version, requested))
                .filter(|entry| entry.is_trusted())
                .min_by_key(|entry| {
                    (
                        conda_channels
                            .iter()
                            .position(|channel| channel == &entry.repository)
                            .unwrap_or(0),
                        &entry.repository,
                    )
                })
            {
                emit_log(
                    app,
                    run_id,
                    &mut logs,
                    &format!("[{}/{}] {} (缓存命中)", index + 1, total, name),
                );
                let result = SearchResult {
                    package: name.clone(),
                    requested_version: requested.clone(),
                    latest_version: cached_entry.version.clone(),
                    repository: cached_entry.repository.clone(),
                    real_name: cached_entry.real_name.clone(),
                    source: cached_entry.source.clone(),
                    found: true,
                    message: "缓存命中".to_string(),
                    status: "found".to_string(),
                    stage: "cacheHit".to_string(),
                };
                results.push(result);
                tokio::task::yield_now().await;
                continue;
            }

            batch_tasks.push((index, name.clone(), requested.clone()));
        }

        crate::search::emit_result_batch(app, run_id, &results[cached_start..]);
        if batch_tasks.is_empty() {
            processed += batch_size;
            continue;
        }

        let mut futures = futures_util::stream::iter(batch_tasks)
            .map(|(index, name, requested)| {
                let client_ref = &client;
                let cancelled_ref = cancelled;
                let budget_ref = &budget;
                let ecosystem_val = ecosystem.to_string();
                let pip_index_val = pip_index.to_string();
                let conda_channels_val = conda_channels.to_vec();
                async move {
                    let mut task_logs = Vec::new();
                    while state.is_paused(run_id)
                        && !cancelled_ref.load(Ordering::SeqCst)
                        && Instant::now() < deadline
                    {
                        sleep(MULTI_STOP_POLL).await;
                    }
                    if state.is_package_cancelled(run_id, &name)
                        || cancelled_ref.load(Ordering::SeqCst)
                        || Instant::now() >= deadline
                    {
                        return (
                            not_found_result(&name, &requested, &ecosystem_val, "检索已取消"),
                            task_logs,
                        );
                    }
                    let result = search_one_multi(
                        client_ref,
                        cancelled_ref,
                        budget_ref,
                        &ecosystem_val,
                        &name,
                        &requested,
                        &pip_index_val,
                        &conda_channels_val,
                        index,
                        total,
                        &mut task_logs,
                    )
                    .await;
                    (result, task_logs)
                }
            })
            .buffer_unordered(concurrency);

        while let Some((result, task_logs)) = tokio::select! {
            result = futures.next() => result,
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => None,
            _ = async { while !cancelled.load(Ordering::SeqCst) { sleep(MULTI_STOP_POLL).await; } } => None,
        } {
            let messages: Vec<_> = task_logs
                .iter()
                .filter_map(|message| append_log(&mut logs, message))
                .collect();
            crate::search::emit_log_batch(app, run_id, &messages);

            if result.found {
                let cache_key = storage::package_cache_key(
                    &result.source,
                    &result.real_name,
                    &result.repository,
                );
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .to_string();
                let existing = cache_update
                    .get(&cache_key)
                    .or_else(|| cache.get(&cache_key));
                cache_update.insert(cache_key, cache_entry_from_result(&result, existing, now));
            }

            if results.len() >= MAX_MULTI_RESULTS {
                emit_log(
                    app,
                    run_id,
                    &mut logs,
                    "检索结果达到上限，后续来源请求已停止",
                );
                break;
            }
            emit_progress(app, run_id, &result);
            results.push(result);
            tokio::task::yield_now().await;
        }

        processed += batch_size;
    }

    drop(cache_index);
    for (key, entry) in &cache_update {
        cache.insert(key.clone(), entry.clone());
    }

    if settings.use_cache {
        if let Err(error) = storage::save_cache(app, &cache) {
            emit_log(app, run_id, &mut logs, &format!("缓存保存失败: {error}"));
        }
    }

    let final_message = if is_stopped(cancelled, &budget) {
        "检索任务已停止"
    } else if Instant::now() >= deadline {
        "检索任务已超时停止"
    } else {
        "检索任务已完成"
    };
    emit_log(app, run_id, &mut logs, final_message);

    Ok(SearchResponse {
        run_id,
        results,
        logs,
        stopped: is_stopped(cancelled, &budget) || Instant::now() >= deadline,
        stage_timings: Vec::new(),
        dependency_graph: None,
    })
}

#[allow(clippy::too_many_arguments)]
async fn search_one_multi(
    client: &Client,
    cancelled: &AtomicBool,
    budget: &RequestBudget,
    ecosystem: &str,
    name: &str,
    requested: &str,
    pip_index: &str,
    conda_channels: &[String],
    index: usize,
    total: usize,
    logs: &mut Vec<String>,
) -> SearchResult {
    logs.push(format!(
        "[{}/{}] 检索 {}{}",
        index + 1,
        total,
        name,
        if requested.is_empty() {
            String::new()
        } else {
            format!(" {requested}")
        }
    ));

    if cancelled.load(Ordering::SeqCst) || budget.is_exhausted() {
        return not_found_result(name, requested, ecosystem, "检索已停止");
    }

    let mut found = false;
    let mut version = String::new();
    let mut repository = String::new();

    if ecosystem == "pip" {
        let base = pip_index
            .trim()
            .trim_end_matches('/')
            .trim_end_matches("/simple");
        if !base.starts_with("https://") || base.contains('?') || base.contains('#') {
            logs.push(format!("Pip Index URL 无效: {base}"));
            return not_found_result(name, requested, ecosystem, "Pip Index URL 无效");
        }
        if !budget.try_acquire() {
            return not_found_result(name, requested, ecosystem, "请求预算耗尽");
        }
        let url = format!("{base}/pypi/{}/json", urlencoding::encode(name));
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(payload) = read_metadata::<PypiResponse>(response).await {
                    version = if requested.is_empty() {
                        payload.info.version.unwrap_or_default()
                    } else {
                        let metadata = CondaResponse {
                            latest_version: None,
                            versions: Some(
                                payload
                                    .releases
                                    .iter()
                                    .filter(|(_, files)| {
                                        files.as_array().is_some_and(|files| !files.is_empty())
                                    })
                                    .map(|(version, _)| version.clone())
                                    .collect(),
                            ),
                        };
                        conda_version(&metadata, requested).unwrap_or_default()
                    };
                    found = !version.is_empty();
                }
            }
            Ok(response) if response.status() == StatusCode::NOT_FOUND => {}
            Ok(response) => logs.push(format!(
                "Pip {} 返回 HTTP {}",
                name,
                response.status().as_u16()
            )),
            Err(error) => logs.push(format!("Pip {} 请求失败: {error}", name)),
        }
        repository = pip_index.trim().trim_end_matches('/').to_string();
    } else {
        for channel in conda_channels
            .iter()
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
        {
            if cancelled.load(Ordering::SeqCst) || budget.is_exhausted() {
                break;
            }
            if !budget.try_acquire() {
                break;
            }
            let url = format!(
                "https://api.anaconda.org/package/{}/{}",
                urlencoding::encode(channel),
                urlencoding::encode(name)
            );
            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    if let Ok(payload) = read_metadata::<CondaResponse>(response).await {
                        if let Some(matched) = conda_version(&payload, requested) {
                            version = matched;
                            found = true;
                            repository = channel.to_string();
                            break;
                        }
                    }
                }
                Ok(response) if response.status() == StatusCode::NOT_FOUND => {}
                Ok(response) => logs.push(format!(
                    "Conda {}/{} 返回 HTTP {}",
                    channel,
                    name,
                    response.status().as_u16()
                )),
                Err(error) => logs.push(format!("Conda {}/{} 请求失败: {error}", channel, name)),
            }
        }
    }

    SearchResult {
        package: name.to_string(),
        requested_version: requested.to_string(),
        latest_version: version,
        repository,
        real_name: name.to_string(),
        source: ecosystem.to_string(),
        found,
        message: if found {
            "检索成功"
        } else {
            "所有配置来源均未找到"
        }
        .to_string(),
        status: if found { "found" } else { "notFound" }.to_string(),
        stage: "final".to_string(),
    }
}

fn not_found_result(name: &str, requested: &str, ecosystem: &str, message: &str) -> SearchResult {
    SearchResult {
        package: name.to_string(),
        requested_version: requested.to_string(),
        latest_version: String::new(),
        repository: String::new(),
        real_name: name.to_string(),
        source: ecosystem.to_string(),
        found: false,
        message: message.to_string(),
        status: "notFound".to_string(),
        stage: "final".to_string(),
    }
}

async fn read_metadata<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T, String> {
    const LIMIT: usize = 8 * 1024 * 1024;
    if response
        .content_length()
        .is_some_and(|length| length > LIMIT as u64)
    {
        return Err("包元数据超过 8 MiB 限制".to_string());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if body.len().saturating_add(chunk.len()) > LIMIT {
            return Err("包元数据超过 8 MiB 限制".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| "包元数据格式无效".to_string())
}

#[cfg(test)]
mod tests;
