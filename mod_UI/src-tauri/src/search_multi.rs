use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;

use crate::models::{SearchResponse, SearchResult};

#[derive(Debug, Deserialize)]
struct PypiInfo {
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PypiResponse {
    info: PypiInfo,
}

#[derive(Debug, Deserialize)]
struct CondaResponse {
    latest_version: Option<String>,
    versions: Option<Vec<String>>,
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '=' | '>' | '<' | '!'))
}

fn split_requirement(line: &str) -> (String, String) {
    let line = line.trim().trim_matches(['"', '\'']);
    for operator in ["==", ">=", "<=", ">", "<", "!=", "~=", "="] {
        if let Some((name, version)) = line.split_once(operator) {
            return (name.trim().to_string(), version.trim().to_string());
        }
    }
    (line.to_string(), String::new())
}

fn inputs(input: &str) -> Vec<(String, String)> {
    input.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
        .map(split_requirement)
        .filter(|(name, _)| valid_name(name))
        .collect()
}

fn conda_version(payload: &CondaResponse, requested: &str) -> Option<String> {
    let mut versions = payload.versions.clone().unwrap_or_default();
    if let Some(latest) = payload.latest_version.as_deref().filter(|value| !value.is_empty()) {
        versions.push(latest.to_string());
    }
    versions.sort_by(|left, right| {
        let left_parts = left.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
        let right_parts = right.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
        left_parts.cmp(right_parts)
    });
    versions.dedup();
    versions.reverse();

    versions.into_iter().find(|version| {
        requested.is_empty()
            || version == requested
            || (requested.matches('.').count() >= 1 && version.starts_with(&format!("{requested}.")))
    })
}

pub async fn search(
    input: &str,
    ecosystem: &str,
    pip_index: &str,
    conda_channels: &[String],
) -> Result<SearchResponse, String> {
    if !matches!(ecosystem, "pip" | "conda") {
        return Err("不支持的多生态类型".to_string());
    }
    let client = Client::builder()
        .user_agent("RLinkModUI/0.2 multi-ecosystem")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut logs = Vec::new();
    for (name, requested) in inputs(input) {
        let mut found = false;
        let mut version = String::new();
        let mut repository = String::new();
        if ecosystem == "pip" {
            let base = pip_index.trim().trim_end_matches('/');
            if !(base.starts_with("https://") || base.starts_with("http://")) || base.contains('?') || base.contains('#') {
                return Err("Pip Index URL 必须是无查询参数的 HTTP(S) 地址".to_string());
            }
            let url = format!("{base}/pypi/{}/json", urlencoding::encode(&name));
            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    if let Ok(payload) = response.json::<PypiResponse>().await {
                        version = payload.info.version.unwrap_or_default();
                        found = !version.is_empty();
                    }
                }
                Ok(response) if response.status() == StatusCode::NOT_FOUND => {}
                Ok(response) => logs.push(format!("Pip {} 返回 HTTP {}", name, response.status().as_u16())),
                Err(error) => logs.push(format!("Pip {} 请求失败: {}", name, error)),
            }
            repository = base.to_string();
        } else {
            for channel in conda_channels.iter().map(|c| c.trim()).filter(|c| !c.is_empty()) {
                let url = format!("https://api.anaconda.org/package/{}/{}", urlencoding::encode(channel), urlencoding::encode(&name));
                match client.get(&url).send().await {
                    Ok(response) if response.status().is_success() => {
                        if let Ok(payload) = response.json::<CondaResponse>().await {
                            if let Some(matched_version) = conda_version(&payload, &requested) {
                                version = matched_version;
                                found = true;
                                repository = channel.to_string();
                                break;
                            }
                        }
                    }
                    Ok(response) if response.status() == StatusCode::NOT_FOUND => {}
                    Ok(response) => logs.push(format!("Conda {}/{} 返回 HTTP {}", channel, name, response.status().as_u16())),
                    Err(error) => logs.push(format!("Conda {}/{} 请求失败: {}", channel, name, error)),
                }
            }
        }
        results.push(SearchResult {
            package: name.clone(),
            requested_version: requested,
            latest_version: version,
            repository,
            real_name: name,
            source: ecosystem.to_string(),
            found,
            message: if found { "检索成功" } else { "所有配置来源均未找到" }.to_string(),
            status: if found { "found" } else { "notFound" }.to_string(),
            stage: "final".to_string(),
        });
    }
    Ok(SearchResponse { run_id: 0, results, logs, stopped: false, stage_timings: Vec::new(), dependency_graph: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pip_requirements() {
        assert_eq!(split_requirement("numpy==1.26.4"), ("numpy".to_string(), "1.26.4".to_string()));
        assert_eq!(split_requirement("pandas"), ("pandas".to_string(), String::new()));
    }

    #[test]
    fn selects_conda_latest_version_from_metadata() {
        let payload = CondaResponse {
            latest_version: Some("1.10.0".to_string()),
            versions: Some(vec!["1.2.0".to_string(), "1.10.0".to_string()]),
        };
        assert_eq!(conda_version(&payload, ""), Some("1.10.0".to_string()));
        assert_eq!(conda_version(&payload, "1.2"), Some("1.2.0".to_string()));
        assert_eq!(conda_version(&payload, "9.0"), None);
    }

    #[test]
    fn empty_conda_metadata_is_not_a_hit() {
        let payload = CondaResponse {
            latest_version: None,
            versions: Some(Vec::new()),
        };
        assert_eq!(conda_version(&payload, ""), None);
    }
}
