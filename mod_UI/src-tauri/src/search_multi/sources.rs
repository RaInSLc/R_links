use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::{Client, StatusCode};

use super::{conda_version, CondaResponse, PypiResponse, RequestBudget, SearchResult};

#[allow(clippy::too_many_arguments)]
pub(super) async fn search_one_multi(
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

pub(super) fn not_found_result(
    name: &str,
    requested: &str,
    ecosystem: &str,
    message: &str,
) -> SearchResult {
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
