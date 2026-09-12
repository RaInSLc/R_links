// Search facade: implementations are split by responsibility while preserving crate::search::*.
use crate::logic::{infer_bioc_version, normalize_github_repository, parse_inputs_filtered};
use crate::models::{
    InputRules, PackageCacheEntry, PackageInput, SearchResponse, SearchResult, Settings,
    MAX_FIELD_CHARS,
};
#[cfg(test)]
pub(crate) use crate::search_sanitize::sanitize_log_message;
use crate::search_sanitize::{
    clean_result_package_name, clean_result_real_name, clean_result_repository,
    clean_result_source, clean_version, sanitize_search_result_for_emit,
};
use crate::search_urls::{validate_search_request_url, validate_search_request_url_with_mirror};
use crate::storage;
use futures_util::StreamExt;
use regex::Regex;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;
use url::Url;
#[path = "search_binary.rs"]
mod search_binary;
#[path = "search_bioc.rs"]
mod search_bioc;
#[path = "search_cache.rs"]
mod search_cache;
#[path = "search_context.rs"]
mod search_context;
#[path = "search_cran.rs"]
mod search_cran;
#[path = "search_github.rs"]
mod search_github;
#[path = "search_http.rs"]
mod search_http;
#[path = "search_orchestration.rs"]
mod search_orchestration;
#[path = "search_result.rs"]
mod search_result;
pub(crate) use search_binary::*;
pub(crate) use search_bioc::*;
pub(crate) use search_cache::*;
pub(crate) use search_context::*;
pub(crate) use search_cran::*;
pub(crate) use search_github::*;
pub(crate) use search_http::*;
pub(crate) use search_orchestration::*;
pub(crate) use search_result::*;
#[cfg(test)]
#[path = "search_tests_1.rs"]
mod search_tests_1;
#[cfg(test)]
#[path = "search_tests_2.rs"]
mod search_tests_2;
