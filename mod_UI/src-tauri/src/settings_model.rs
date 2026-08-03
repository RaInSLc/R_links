use serde::{Deserialize, Serialize};

use crate::models::MAX_FIELD_CHARS;
use crate::models::MAX_TOKEN_CHARS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub proxy: String,
    pub github_token: String,
    pub cran_mirror: String,
    #[serde(default)]
    pub r_lib_path: String,
    pub full_search: bool,
    pub search_concurrency: usize,
    pub archive_github_major_gap: usize,
    pub conditional: bool,
    pub install_dependencies: bool,
    pub show_remote_version: bool,
    pub use_cache: bool,
    pub max_cache_entries: usize,
    pub use_filter: bool,
    pub resolve_dependencies: bool,
    pub max_dependency_depth: usize,
    pub include_light_dependencies: bool,
    pub max_dependency_nodes: usize,
    #[serde(default = "default_pinned_methods")]
    pub pinned_methods: Vec<String>,
    #[serde(default = "default_pip_index")]
    pub pip_index: String,
    #[serde(default = "default_conda_channels")]
    pub conda_channels: Vec<String>,
}

fn default_pip_index() -> String {
    "https://pypi.org".to_string()
}
fn default_conda_channels() -> Vec<String> {
    vec!["conda-forge".to_string(), "bioconda".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub proxy: String,
    pub github_token_configured: bool,
    pub cran_mirror: String,
    pub r_lib_path: String,
    pub full_search: bool,
    pub search_concurrency: usize,
    pub archive_github_major_gap: usize,
    pub conditional: bool,
    pub install_dependencies: bool,
    pub show_remote_version: bool,
    pub use_cache: bool,
    pub max_cache_entries: usize,
    pub use_filter: bool,
    pub resolve_dependencies: bool,
    pub max_dependency_depth: usize,
    pub include_light_dependencies: bool,
    pub max_dependency_nodes: usize,
    pub pinned_methods: Vec<String>,
    pub pip_index: String,
    pub conda_channels: Vec<String>,
}

fn default_pinned_methods() -> Vec<String> {
    ["auto", "base", "biocManager", "github"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            github_token: String::new(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            r_lib_path: String::new(),
            full_search: false,
            search_concurrency: 6,
            archive_github_major_gap: 1,
            conditional: true,
            install_dependencies: true,
            show_remote_version: true,
            use_cache: true,
            max_cache_entries: 1000,
            use_filter: true,
            resolve_dependencies: true,
            max_dependency_depth: 2,
            include_light_dependencies: false,
            max_dependency_nodes: 100,
            pinned_methods: default_pinned_methods(),
            pip_index: default_pip_index(),
            conda_channels: default_conda_channels(),
        }
    }
}

impl Settings {
    pub fn normalized(&self) -> Result<Self, String> {
        Ok(Self {
            proxy: normalize_proxy(&self.proxy)?,
            github_token: normalize_token(&self.github_token)?,
            cran_mirror: crate::models::normalize_cran_mirror_url(&self.cran_mirror)?,
            r_lib_path: normalize_r_lib_path(&self.r_lib_path)?,
            full_search: self.full_search,
            search_concurrency: self.search_concurrency.clamp(1, 12),
            archive_github_major_gap: self.archive_github_major_gap.clamp(0, 10),
            conditional: self.conditional,
            install_dependencies: self.install_dependencies,
            show_remote_version: self.show_remote_version,
            use_cache: self.use_cache,
            max_cache_entries: self.max_cache_entries.clamp(1, 10000),
            use_filter: self.use_filter,
            resolve_dependencies: self.resolve_dependencies,
            max_dependency_depth: self.max_dependency_depth.clamp(1, 5),
            include_light_dependencies: self.include_light_dependencies,
            max_dependency_nodes: self.max_dependency_nodes.clamp(1, 500),
            pinned_methods: normalize_pinned_methods(&self.pinned_methods),
            pip_index: self.pip_index.trim().to_string(),
            conda_channels: self
                .conda_channels
                .iter()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .take(20)
                .collect(),
        })
    }

    pub fn public_view(&self) -> PublicSettings {
        PublicSettings {
            proxy: self.proxy.clone(),
            github_token_configured: !self.github_token.trim().is_empty(),
            cran_mirror: self.cran_mirror.clone(),
            r_lib_path: self.r_lib_path.clone(),
            full_search: self.full_search,
            search_concurrency: self.search_concurrency,
            archive_github_major_gap: self.archive_github_major_gap,
            conditional: self.conditional,
            install_dependencies: self.install_dependencies,
            show_remote_version: self.show_remote_version,
            use_cache: self.use_cache,
            max_cache_entries: self.max_cache_entries,
            use_filter: self.use_filter,
            resolve_dependencies: self.resolve_dependencies,
            max_dependency_depth: self.max_dependency_depth,
            include_light_dependencies: self.include_light_dependencies,
            max_dependency_nodes: self.max_dependency_nodes,
            pinned_methods: self.pinned_methods.clone(),
            pip_index: self.pip_index.clone(),
            conda_channels: self.conda_channels.clone(),
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

fn normalize_r_lib_path(value: &str) -> Result<String, String> {
    let path = value.trim();
    if path.len() > MAX_FIELD_CHARS || path.chars().any(|character| character.is_control()) {
        return Err(format!(
            "R 库路径无效，最多允许 {MAX_FIELD_CHARS} 字节且不能包含控制字符"
        ));
    }
    Ok(path.to_string())
}

fn normalize_pinned_methods(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        if matches!(
            value.as_str(),
            "auto"
                | "devtools"
                | "remotes"
                | "github"
                | "base"
                | "version"
                | "biocManager"
                | "checkSystem"
        ) && !normalized.iter().any(|existing| existing == value)
        {
            normalized.push(value.clone());
        }
    }
    if normalized.is_empty() {
        default_pinned_methods()
    } else {
        normalized
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOptions {
    pub method: String,
    pub conditional: bool,
    pub install_dependencies: bool,
    pub mirror: String,
    #[serde(default)]
    pub r_lib_path: String,
    #[serde(default = "default_archive_github_major_gap")]
    pub archive_github_major_gap: usize,
    #[serde(default)]
    pub append_verify: bool,
    #[serde(default)]
    pub parallel_install: bool,
}

pub fn default_archive_github_major_gap() -> usize {
    1
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            method: String::new(),
            conditional: false,
            install_dependencies: false,
            mirror: String::new(),
            r_lib_path: String::new(),
            archive_github_major_gap: 1,
            append_verify: false,
            parallel_install: false,
        }
    }
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
    let parsed = url::Url::parse(&candidate).map_err(|_| "网络代理格式无效".to_string())?;
    match parsed.scheme() {
        "http" | "https" | "socks5" | "socks5h" => {}
        _ => return Err("网络代理仅支持 http、https、socks5 或 socks5h".to_string()),
    }
    let host = match parsed.host() {
        Some(url::Host::Domain(domain)) => {
            url::Host::parse(domain).map_err(|_| "网络代理主机名无效".to_string())?
        }
        Some(url::Host::Ipv4(address)) => url::Host::Ipv4(address),
        Some(url::Host::Ipv6(address)) => url::Host::Ipv6(address),
        None => return Err("网络代理缺少主机名".to_string()),
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("网络代理不允许包含用户名或密码".to_string());
    }
    if !matches!(parsed.path(), "" | "/") || parsed.query().is_some() || parsed.fragment().is_some()
    {
        return Err("网络代理不允许包含路径、查询参数或片段".to_string());
    }
    Ok(format!(
        "{}://{host}{}",
        parsed.scheme(),
        parsed
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default()
    ))
}

fn normalize_token(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.len() > MAX_TOKEN_CHARS {
        return Err("GitHub Token 长度超过限制".to_string());
    }
    if trimmed
        .chars()
        .any(|character| !character.is_ascii_graphic())
    {
        return Err("GitHub Token 包含非法字符".to_string());
    }
    Ok(trimmed.to_string())
}
