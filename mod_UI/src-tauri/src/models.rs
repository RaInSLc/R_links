use serde::{Deserialize, Serialize};
use url::{Host, Url};

pub const MAX_INPUT_CHARS: usize = 100_000;
pub const MAX_PACKAGE_LINES: usize = 500;
pub const MAX_FIELD_CHARS: usize = 2_048;
pub const MAX_TOKEN_CHARS: usize = 512;
pub const MAX_HISTORY_RECORDS: usize = 10000;
pub const MAX_HISTORY_COMMAND_CHARS: usize = 8_000;
pub const MAX_SCRIPT_CHARS: usize = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub proxy: String,
    pub github_token: String,
    pub cran_mirror: String,
    pub full_search: bool,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub proxy: String,
    pub github_token_configured: bool,
    pub cran_mirror: String,
    pub full_search: bool,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            github_token: String::new(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: false,
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
        }
    }
}

impl Settings {
    pub fn normalized(&self) -> Result<Self, String> {
        let proxy = normalize_proxy(&self.proxy)?;
        let github_token = normalize_token(&self.github_token)?;
        let cran_mirror = normalize_cran_mirror_url(&self.cran_mirror)?;
        let max_cache_entries = self.max_cache_entries.clamp(1, 10000);
        let max_dependency_depth = self.max_dependency_depth.clamp(1, 5);
        let max_dependency_nodes = self.max_dependency_nodes.clamp(1, 500);

        Ok(Self {
            proxy,
            github_token,
            cran_mirror,
            full_search: self.full_search,
            conditional: self.conditional,
            install_dependencies: self.install_dependencies,
            show_remote_version: self.show_remote_version,
            use_cache: self.use_cache,
            max_cache_entries,
            use_filter: self.use_filter,
            resolve_dependencies: self.resolve_dependencies,
            max_dependency_depth,
            include_light_dependencies: self.include_light_dependencies,
            max_dependency_nodes,
        })
    }

    pub fn public_view(&self) -> PublicSettings {
        PublicSettings {
            proxy: self.proxy.clone(),
            github_token_configured: !self.github_token.trim().is_empty(),
            cran_mirror: self.cran_mirror.clone(),
            full_search: self.full_search,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOptions {
    pub method: String,
    pub conditional: bool,
    pub install_dependencies: bool,
    pub mirror: String,
    #[serde(default)]
    pub append_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorSpeedResult {
    pub mirror: String,
    pub label: String,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseDependenciesInfo {
    pub package: String,
    pub depends: usize,
    pub imports: usize,
    pub suggests: usize,
    pub linking_to: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraph {
    pub roots: Vec<String>,
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    pub summary: DependencySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencyNode {
    pub package: String,
    pub source: String,
    pub version: String,
    pub depth: usize,
    pub root_packages: Vec<String>,
    pub direct_dependency_count: usize,
    pub heavy_dependency_count: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub strength: String,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencySummary {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub heavy_nodes: usize,
    pub light_nodes: usize,
    pub shared_nodes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub run_id: u64,
    pub results: Vec<SearchResult>,
    pub logs: Vec<String>,
    pub stopped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_graph: Option<DependencyGraph>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageCacheEntry {
    pub package_name: String,
    pub source: String,
    pub version: String,
    pub repository: String,
    pub real_name: String,
    pub cached_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInput {
    pub raw: String,
    pub name: String,
    pub version: String,
    pub source_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputRules {
    /// 行内分隔符，用于将一行拆分为多个包名（如逗号、分号）
    pub separators: Vec<String>,
    /// 是否去除包名两端的引号（" 和 '）
    pub strip_quotes: bool,
    /// 是否去除 R 的 c(...) 或 list(...) 包裹
    pub strip_c_parens: bool,
    /// 注释字符前缀列表
    pub comment_chars: Vec<String>,
    /// 是否将空格也作为分隔符（开启后将禁用版本号提取）
    pub split_spaces: bool,
    /// 自定义排除正则列表，匹配的行/段在解析时将被静默忽略
    #[serde(default)]
    pub exclude_regex: Vec<String>,
    /// 自定义排除包名关键词列表，被匹配的包名在解析时将被静默忽略
    #[serde(default)]
    pub exclude_keywords: Vec<String>,
}

impl Default for InputRules {
    fn default() -> Self {
        Self {
            separators: vec![",".to_string(), ";".to_string()],
            strip_quotes: true,
            strip_c_parens: true,
            comment_chars: vec!["#".to_string()],
            split_spaces: false,
            exclude_regex: Vec::new(),
            exclude_keywords: Vec::new(),
        }
    }
}

impl InputRules {
    pub fn normalized(&self) -> Self {
        let mut separators: Vec<String> = self
            .separators
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 16 && !s.chars().any(char::is_control))
            .take(20)
            .collect();
        if separators.is_empty() {
            separators = vec![",".to_string(), ";".to_string()];
        }

        let mut comment_chars: Vec<String> = self
            .comment_chars
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 16 && !s.chars().any(char::is_control))
            .take(20)
            .collect();
        if comment_chars.is_empty() {
            comment_chars = vec!["#".to_string()];
        }

        let exclude_regex: Vec<String> = self
            .exclude_regex
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .filter(|s| regex::Regex::new(s).is_ok())
            .take(10)
            .collect();

        let exclude_keywords: Vec<String> = self
            .exclude_keywords
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 64 && !s.chars().any(char::is_control))
            .take(50)
            .collect();

        Self {
            separators,
            strip_quotes: self.strip_quotes,
            strip_c_parens: self.strip_c_parens,
            comment_chars,
            split_spaces: self.split_spaces,
            exclude_regex,
            exclude_keywords,
        }
    }
}

pub const INPUT_RULES_FILE_NAME: &str = "input_rules.json";

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
    if parsed.port().is_some() || url_has_explicit_port(trimmed) {
        return Err(format!("{field_name}不允许包含显式端口"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!("{field_name}不允许包含用户名或密码"));
    }
    Ok(parsed.to_string())
}

pub fn normalize_https_url(value: &str, field_name: &str) -> Result<String, String> {
    let normalized = normalize_http_url(value, field_name)?;
    let parsed = Url::parse(&normalized).map_err(|_| format!("{field_name}必须是有效 URL"))?;
    if parsed.scheme() != "https" {
        return Err(format!("{field_name}仅支持 https"));
    }
    Ok(normalized)
}

pub fn normalize_cran_mirror_url(value: &str) -> Result<String, String> {
    let normalized = normalize_https_url(value, "CRAN 镜像")?;
    let parsed = Url::parse(&normalized).map_err(|_| "CRAN 镜像必须是有效 URL".to_string())?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("CRAN 镜像不允许包含查询参数或片段".to_string());
    }
    let mut mirror = normalized.trim_end_matches('/').to_string();
    mirror.push('/');
    Ok(mirror)
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
    let host = match parsed.host() {
        Some(Host::Domain(domain)) => {
            Host::parse(domain).map_err(|_| "网络代理主机名无效".to_string())?
        }
        Some(Host::Ipv4(address)) => Host::Ipv4(address),
        Some(Host::Ipv6(address)) => Host::Ipv6(address),
        None => return Err("网络代理缺少主机名".to_string()),
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("网络代理不允许包含用户名或密码".to_string());
    }
    if !matches!(parsed.path(), "" | "/") || parsed.query().is_some() || parsed.fragment().is_some()
    {
        return Err("网络代理不允许包含路径、查询参数或片段".to_string());
    }
    let port = parsed
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    Ok(format!("{}://{host}{port}", parsed.scheme()))
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

pub fn url_has_explicit_port(value: &str) -> bool {
    let trimmed = value.trim();
    let Some(scheme_end) = trimmed.find("://") else {
        return false;
    };
    let authority_start = scheme_end + 3;
    let authority_end = trimmed[authority_start..]
        .find(['/', '?', '#'])
        .map(|index| authority_start + index)
        .unwrap_or(trimmed.len());
    let authority = &trimmed[authority_start..authority_end];
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, host_port)| host_port)
        .unwrap_or(authority);

    if let Some(rest) = host_port.strip_prefix('[') {
        return rest
            .find(']')
            .is_some_and(|index| rest[index + 1..].starts_with(':'));
    }
    host_port.contains(':')
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
    fn canonicalizes_proxy_authority_before_use() {
        for (proxy, expected) in [
            ("HTTP://LOCALHOST:7890", "http://localhost:7890"),
            ("socks5://[0:0:0:0:0:0:0:1]:1080", "socks5://[::1]:1080"),
            (
                "socks5h://例子.测试:1080",
                "socks5h://xn--fsqu00a.xn--0zwm56d:1080",
            ),
        ] {
            let settings = Settings {
                proxy: proxy.to_string(),
                ..Settings::default()
            };
            assert_eq!(
                settings.normalized().expect("代理应可规范化").proxy,
                expected
            );
        }
    }

    #[test]
    fn rejects_credentialed_or_scoped_proxy_url() {
        for proxy in [
            "http://user:pass@127.0.0.1:7890",
            "https://127.0.0.1:7890/proxy",
            "socks5://127.0.0.1:7890?target=example",
            "socks5h://127.0.0.1:7890#fragment",
            r"socks5://example.com\redirect:1080",
            "socks5h://example.com%2Fredirect:1080",
        ] {
            let settings = Settings {
                proxy: proxy.to_string(),
                ..Settings::default()
            };
            assert!(settings.normalized().is_err());
        }
    }

    #[test]
    fn rejects_credentialed_mirror_url() {
        assert!(normalize_http_url("https://user:pass@example.com/CRAN/", "CRAN 镜像").is_err());
    }

    #[test]
    fn normalizes_cran_mirror_directory_url() {
        assert_eq!(
            normalize_cran_mirror_url(" https://cloud.r-project.org ")
                .expect("CRAN 镜像应可规范化"),
            "https://cloud.r-project.org/"
        );
        assert!(normalize_cran_mirror_url("https://cloud.r-project.org?token=secret").is_err());
        assert!(normalize_cran_mirror_url("https://cloud.r-project.org/#cran").is_err());
        assert!(normalize_cran_mirror_url("https://user:pass@example.com/CRAN/").is_err());
        assert!(normalize_cran_mirror_url("https://cloud.r-project.org:443/").is_err());
    }

    #[test]
    fn rejects_plain_http_package_source_url() {
        assert!(normalize_https_url("http://example.com/pkg_1.0.tar.gz", "安装 URL").is_err());
        assert!(normalize_https_url("https://example.com/pkg_1.0.tar.gz", "安装 URL").is_ok());
        assert!(normalize_https_url("https://example.com:443/pkg_1.0.tar.gz", "安装 URL").is_err());
    }

    #[test]
    fn canonicalizes_valid_urls_before_use() {
        assert_eq!(
            normalize_https_url(r"https://example.com\src\demo_1.0.tar.gz", "安装 URL")
                .expect("反斜杠路径应规范化"),
            "https://example.com/src/demo_1.0.tar.gz"
        );
        assert_eq!(
            normalize_https_url(
                "https://example.com/src package/demo_1.0.tar.gz",
                "安装 URL"
            )
            .expect("空格应编码"),
            "https://example.com/src%20package/demo_1.0.tar.gz"
        );
        assert_eq!(
            normalize_https_url("https://example.com/src/../demo_1.0.tar.gz", "安装 URL")
                .expect("点路径应规范化"),
            "https://example.com/demo_1.0.tar.gz"
        );
    }

    #[test]
    fn detects_explicit_url_ports_before_url_normalization() {
        assert!(url_has_explicit_port("https://example.com:443/path"));
        assert!(url_has_explicit_port(
            "https://user:pass@example.com:443/path"
        ));
        assert!(url_has_explicit_port("https://[::1]:443/path"));
        assert!(!url_has_explicit_port("https://example.com/path"));
        assert!(!url_has_explicit_port("https://[::1]/path"));
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

    #[test]
    fn rejects_token_with_whitespace_or_non_ascii() {
        assert_eq!(
            Settings {
                github_token: " ghp_demo\n".to_string(),
                ..Settings::default()
            }
            .normalized()
            .expect("首尾空白应被清理")
            .github_token,
            "ghp_demo"
        );

        for token in [
            "ghp_demo token",
            "ghp_demo\tvalue",
            "ghp_demo\rvalue",
            "ghp_demo\u{7f}value",
            "ghp_令牌",
        ] {
            let settings = Settings {
                github_token: token.to_string(),
                ..Settings::default()
            };
            assert!(settings.normalized().is_err(), "{token:?}");
        }
    }

    #[test]
    fn test_normalizes_cache_entries_limit() {
        let settings = Settings {
            max_cache_entries: 0,
            ..Settings::default()
        };
        assert_eq!(
            settings.normalized().expect("应能规范化").max_cache_entries,
            1
        );

        let settings = Settings {
            max_cache_entries: 20000,
            ..Settings::default()
        };
        assert_eq!(
            settings.normalized().expect("应能规范化").max_cache_entries,
            10000
        );
    }
}
