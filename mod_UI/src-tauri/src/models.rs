use serde::{Deserialize, Serialize};
use url::Url;

pub const MAX_INPUT_CHARS: usize = 100_000;
pub const MAX_PACKAGE_LINES: usize = 500;
pub const MAX_FIELD_CHARS: usize = 2_048;
pub const MAX_TOKEN_CHARS: usize = 512;
pub const MAX_HISTORY_RECORDS: usize = 100;
pub const MAX_HISTORY_COMMAND_CHARS: usize = 8_000;
pub const MAX_SCRIPT_CHARS: usize = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub proxy: String,
    pub github_token: String,
    pub cran_mirror: String,
    pub full_search: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub proxy: String,
    pub github_token_configured: bool,
    pub cran_mirror: String,
    pub full_search: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            github_token: String::new(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: false,
        }
    }
}

impl Settings {
    pub fn normalized(&self) -> Result<Self, String> {
        let proxy = normalize_proxy(&self.proxy)?;
        let github_token = normalize_token(&self.github_token)?;
        let cran_mirror = normalize_https_url(&self.cran_mirror, "CRAN 镜像")?;

        Ok(Self {
            proxy,
            github_token,
            cran_mirror,
            full_search: self.full_search,
        })
    }

    pub fn public_view(&self) -> PublicSettings {
        PublicSettings {
            proxy: self.proxy.clone(),
            github_token_configured: !self.github_token.trim().is_empty(),
            cran_mirror: self.cran_mirror.clone(),
            full_search: self.full_search,
        }
    }

    pub fn merged_with_existing_token(&self, existing: &Settings) -> Result<Self, String> {
        let mut normalized = self.normalized()?;
        if normalized.github_token.is_empty() {
            normalized.github_token = existing.github_token.clone();
        }
        Ok(normalized)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOptions {
    pub method: String,
    pub conditional: bool,
    pub install_dependencies: bool,
    pub mirror: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub package: String,
    pub requested_version: String,
    pub latest_version: String,
    pub repository: String,
    pub real_name: String,
    pub source: String,
    pub found: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub logs: Vec<String>,
    pub stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: String,
    pub command: String,
    pub package_name: String,
    pub version: String,
    pub tool_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInput {
    pub raw: String,
    pub name: String,
    pub version: String,
}

pub fn normalize_http_url(value: &str, field_name: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field_name}不能为空"));
    }
    if trimmed.len() > MAX_FIELD_CHARS || trimmed.chars().any(|character| character.is_control()) {
        return Err(format!("{field_name}包含非法字符或长度过长"));
    }

    let parsed = Url::parse(trimmed).map_err(|_| format!("{field_name}必须是有效 URL"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(format!("{field_name}仅支持 http 或 https")),
    }
    if parsed.host_str().is_none() {
        return Err(format!("{field_name}缺少主机名"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!("{field_name}不允许包含用户名或密码"));
    }
    Ok(trimmed.to_string())
}

pub fn normalize_https_url(value: &str, field_name: &str) -> Result<String, String> {
    let normalized = normalize_http_url(value, field_name)?;
    let parsed = Url::parse(&normalized).map_err(|_| format!("{field_name}必须是有效 URL"))?;
    if parsed.scheme() != "https" {
        return Err(format!("{field_name}仅支持 https"));
    }
    Ok(normalized)
}

fn normalize_proxy(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > MAX_FIELD_CHARS || trimmed.chars().any(|character| character.is_control()) {
        return Err("网络代理包含非法字符或长度过长".to_string());
    }

    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let parsed = Url::parse(&candidate).map_err(|_| "网络代理格式无效".to_string())?;
    match parsed.scheme() {
        "http" | "https" | "socks5" | "socks5h" => {}
        _ => return Err("网络代理仅支持 http、https、socks5 或 socks5h".to_string()),
    }
    if parsed.host_str().is_none() {
        return Err("网络代理缺少主机名".to_string());
    }
    Ok(candidate)
}

fn normalize_token(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.len() > MAX_TOKEN_CHARS {
        return Err("GitHub Token 长度超过限制".to_string());
    }
    if trimmed.chars().any(|character| character.is_control()) {
        return Err("GitHub Token 包含非法控制字符".to_string());
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_proxy_without_scheme() {
        let settings = Settings {
            proxy: "127.0.0.1:7890".to_string(),
            ..Settings::default()
        };
        assert_eq!(
            settings.normalized().expect("代理应合法").proxy,
            "http://127.0.0.1:7890"
        );
    }

    #[test]
    fn rejects_credentialed_mirror_url() {
        assert!(normalize_http_url("https://user:pass@example.com/CRAN/", "CRAN 镜像").is_err());
    }

    #[test]
    fn rejects_plain_http_package_source_url() {
        assert!(normalize_https_url("http://example.com/pkg_1.0.tar.gz", "安装 URL").is_err());
        assert!(normalize_https_url("https://example.com/pkg_1.0.tar.gz", "安装 URL").is_ok());
    }

    #[test]
    fn public_settings_do_not_expose_token() {
        let settings = Settings {
            github_token: "ghp_secret".to_string(),
            ..Settings::default()
        };
        let public = settings.public_view();
        assert!(public.github_token_configured);
        let encoded = serde_json::to_string(&public).expect("公开设置应可序列化");
        assert!(!encoded.contains("ghp_secret"));
        assert!(!encoded.contains("githubToken\":\""));
        assert!(encoded.contains("githubTokenConfigured"));
    }

    #[test]
    fn empty_token_preserves_existing_saved_token() {
        let existing = Settings {
            github_token: "ghp_existing".to_string(),
            ..Settings::default()
        };
        let incoming = Settings {
            github_token: String::new(),
            ..Settings::default()
        };
        let merged = incoming
            .merged_with_existing_token(&existing)
            .expect("空 Token 应保留旧值");
        assert_eq!(merged.github_token, "ghp_existing");
    }
}
