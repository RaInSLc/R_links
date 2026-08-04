use regex::Regex;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use crate::models::{SearchResult, Settings, MAX_PACKAGE_LINES};
use crate::search_sanitize::sanitize_log_message;
pub(crate) const BIOC_VERSIONS: &[&str] = &[
    "3.23", "3.22", "3.21", "3.20", "3.19", "3.18", "3.17", "3.16", "3.15", "3.14", "3.13", "3.12",
    "3.11", "3.10", "3.9", "3.8", "3.7", "3.6", "3.5", "3.4", "3.3", "3.2", "3.1", "3.0",
];
pub(crate) const BIOC_CATEGORIES: &[&str] = &["bioc", "data/annotation", "data/experiment", "workflows"];
pub(crate) const MAX_TEXT_RESPONSE_BYTES: usize = 512 * 1024;
pub(crate) const MAX_DESCRIPTION_BYTES: usize = 64 * 1024;
pub(crate) const MAX_DESCRIPTION_LINES: usize = 1_000;
pub(crate) const MAX_DESCRIPTION_LINE_CHARS: usize = 2_048;
pub(crate) const MAX_JSON_RESPONSE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_GITHUB_SEARCH_ITEMS: usize = 10;
pub(crate) const MAX_GITHUB_REPOSITORY_CHARS: usize = 200;
pub(crate) const MAX_SEARCH_HTTP_REQUESTS: usize = 200;
pub(crate) const MAX_SEARCH_DURATION: Duration = Duration::from_secs(300);
pub(crate) const MAX_SEARCH_RESULTS: usize = MAX_PACKAGE_LINES * 16;
pub(crate) const MAX_SEARCH_LOGS: usize = 1_000;
pub(crate) const SEARCH_STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const SEARCH_STOPPED_ERROR: &str = "检索已停止";
pub(crate) const SEARCH_LOGS_TRUNCATED_MESSAGE: &str = "检索日志达到上限，后续日志已停止记录";
pub(crate) const SEARCH_RESULTS_TRUNCATED_MESSAGE: &str = "检索结果达到上限，后续来源请求已停止";
pub(crate) const STREAM_RESULT_PAUSE: Duration = Duration::from_millis(35);
pub(crate) const AUTO_RETRY_LIMIT: usize = 1;
pub(crate) const R_FORGE_PACKAGES_URL: &str = "https://r-forge.r-project.org/src/contrib/PACKAGES";
pub(crate) const R_FORGE_REPOS_URL: &str = "http://R-Forge.R-project.org";
pub(crate) static HTML_VERSION_RE: OnceLock<Regex> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub(crate) struct GithubSearchResponse {
    pub(crate) items: Vec<GithubRepository>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubRepository {
    pub(crate) full_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GithubDescription {
    pub(crate) package_name: String,
    pub(crate) version: String,
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

pub(crate) struct RequestBudget {
    remaining: AtomicUsize,
    exhausted: AtomicBool,
}

impl RequestBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            remaining: AtomicUsize::new(limit),
            exhausted: AtomicBool::new(false),
        }
    }

    pub(crate) fn try_acquire(&self) -> Result<(), String> {
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

    pub(crate) fn is_exhausted(&self) -> bool {
        self.exhausted.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn remaining_for_test(&self) -> usize {
        self.remaining.load(Ordering::SeqCst)
    }
}

pub(crate) struct SearchContext<'a> {
    pub(crate) log_emitter: Option<(&'a AppHandle, u64)>,
    pub(crate) client: &'a Client,
    pub(crate) settings: &'a Settings,
    pub(crate) cancelled: &'a AtomicBool,
    pub(crate) budget: &'a RequestBudget,
    pub(crate) deadline: Instant,
    pub(crate) timed_out: &'a AtomicBool,
    pub(crate) logs: &'a mut Vec<String>,
    pub(crate) result_limit_reached: bool,
    pub(crate) github_rate_limited: bool,
}

impl SearchContext<'_> {
    pub(crate) fn is_stopped(&self) -> bool {
        self.result_limit_reached || search_stopped(self.cancelled, self.budget)
    }

    pub(crate) fn is_expired(&self) -> bool {
        if self.timed_out.load(Ordering::SeqCst) {
            return true;
        }
        if Instant::now() >= self.deadline {
            self.timed_out.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub(crate) fn should_stop(&self) -> bool {
        self.is_stopped() || self.is_expired()
    }

    pub(crate) fn log(&mut self, message: &str) {
        if let Some((app, run_id)) = self.log_emitter {
            log(app, run_id, self.logs, message);
        } else {
            let _ = append_search_log(self.logs, message);
        }
    }

    pub(crate) fn acquire_request_budget(&mut self) -> bool {
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

pub(crate) fn search_stopped(cancelled: &AtomicBool, budget: &RequestBudget) -> bool {
    cancelled.load(Ordering::SeqCst) || budget.is_exhausted()
}

pub(crate) async fn wait_until_search_stopped(cancelled: &AtomicBool, budget: &RequestBudget) {
    while !search_stopped(cancelled, budget) {
        tokio::time::sleep(SEARCH_STOP_POLL_INTERVAL).await;
    }
}

pub(crate) async fn await_or_stop<T>(
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

pub(crate) async fn send_request(
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

pub(crate) fn append_search_log(logs: &mut Vec<String>, message: &str) -> Option<String> {
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

pub(crate) fn log(app: &AppHandle, run_id: u64, logs: &mut Vec<String>, message: &str) {
    if let Some(message) = append_search_log(logs, message) {
        let _ = app.emit("search-log-batch", SearchLogBatchEvent {
            run_id,
            messages: vec![message],
        });
    }
}
