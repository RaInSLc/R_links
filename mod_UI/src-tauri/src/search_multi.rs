use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_util::stream::StreamExt;
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

use crate::models::{PackageCacheEntry, SearchResponse, SearchResult, Settings};
use crate::search::{
    build_client, cache_entry_from_result, SearchLogBatchEvent, SearchProgressEvent,
};
use crate::storage;
use crate::SearchState;

mod requirement;
mod sources;

use requirement::{conda_version, inputs, requirement_matches};
use sources::{not_found_result, search_one_multi};

#[cfg(test)]
use requirement::split_requirement;
#[cfg(test)]
use reqwest::Client;

const MAX_MULTI_DURATION: Duration = Duration::from_secs(300);
const MAX_MULTI_HTTP_REQUESTS: usize = 200;
/// 与前端 `utils-types.ts` 的 `MAX_SEARCH_RESULTS`（`MAX_PACKAGE_LINES * 16`）
/// 严格对齐：后端若产出更多条，前端 `sanitizeSearchResponse` 会静默丢弃超出部分，
/// 既浪费检索配额，也让"结果条数"在两个层面出现不一致的语义。
const MAX_MULTI_RESULTS: usize = crate::models::MAX_PACKAGE_LINES * 16;
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

    let cache_revision = storage::cache_revision();
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
        if let Err(error) = storage::save_search_cache(app, &cache, cache_revision) {
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

#[cfg(test)]
mod tests;
