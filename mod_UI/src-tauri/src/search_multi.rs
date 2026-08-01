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
    version: Option<String>,
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
                            version = payload.version.unwrap_or_default();
                            found = !version.is_empty();
                        }
                        repository = channel.to_string();
                        break;
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
    Ok(SearchResponse { run_id: 0, results, logs, stopped: false, dependency_graph: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pip_requirements() {
        assert_eq!(split_requirement("numpy==1.26.4"), ("numpy".to_string(), "1.26.4".to_string()));
        assert_eq!(split_requirement("pandas"), ("pandas".to_string(), String::new()));
    }
}
