use url::Url;

pub const MAX_INPUT_CHARS: usize = 100_000;
pub const MAX_PACKAGE_LINES: usize = 500;
pub const MAX_FIELD_CHARS: usize = 2_048;
pub const CACHE_TRUST_THRESHOLD: u32 = 3;
pub const MAX_TOKEN_CHARS: usize = 512;
pub const MAX_HISTORY_RECORDS: usize = 10000;
pub const MAX_HISTORY_COMMAND_CHARS: usize = 8_000;
pub const MAX_SCRIPT_CHARS: usize = 1_000_000;

#[path = "dependency_model.rs"]
mod dependency_model;
#[path = "history_model.rs"]
mod history_model;
#[path = "search_model.rs"]
mod search_model;
#[path = "settings_model.rs"]
mod settings_model;

pub use dependency_model::*;
pub use history_model::*;
pub use search_model::*;
pub use settings_model::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputRules {
    pub separators: Vec<String>,
    pub strip_quotes: bool,
    pub strip_c_parens: bool,
    pub comment_chars: Vec<String>,
    pub split_spaces: bool,
    #[serde(default)]
    pub exclude_regex: Vec<String>,
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
    if Url::parse(&normalized)
        .map_err(|_| format!("{field_name}必须是有效 URL"))?
        .scheme()
        != "https"
    {
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
    Ok(format!("{}/", normalized.trim_end_matches('/')))
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
            settings.normalized().unwrap().proxy,
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
            assert_eq!(settings.normalized().unwrap().proxy, expected);
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
            assert!(Settings {
                proxy: proxy.to_string(),
                ..Settings::default()
            }
            .normalized()
            .is_err());
        }
    }
    #[test]
    fn rejects_credentialed_mirror_url() {
        assert!(normalize_http_url("https://user:pass@example.com/CRAN/", "CRAN 镜像").is_err());
    }
    #[test]
    fn normalizes_cran_mirror_directory_url() {
        assert_eq!(
            normalize_cran_mirror_url(" https://cloud.r-project.org ").unwrap(),
            "https://cloud.r-project.org/"
        );
        assert!(normalize_cran_mirror_url("https://cloud.r-project.org?token=secret").is_err());
        assert!(normalize_cran_mirror_url("https://cloud.r-project.org/#cran").is_err());
        assert!(normalize_cran_mirror_url("https://user:pass@example.com/CRAN/").is_err());
        assert!(normalize_cran_mirror_url("https://cloud.r-project.org:443/").is_err());
    }
    #[test]
    fn accepts_rspm_mirror_as_https_cran_repository() {
        assert_eq!(
            normalize_cran_mirror_url("https://packagemanager.posit.co/cran/latest").unwrap(),
            "https://packagemanager.posit.co/cran/latest/"
        );
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
            normalize_https_url(r"https://example.com\src\demo_1.0.tar.gz", "安装 URL").unwrap(),
            "https://example.com/src/demo_1.0.tar.gz"
        );
        assert_eq!(
            normalize_https_url(
                "https://example.com/src package/demo_1.0.tar.gz",
                "安装 URL"
            )
            .unwrap(),
            "https://example.com/src%20package/demo_1.0.tar.gz"
        );
        assert_eq!(
            normalize_https_url("https://example.com/src/../demo_1.0.tar.gz", "安装 URL").unwrap(),
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
        let encoded = serde_json::to_string(&public).unwrap();
        assert!(!encoded.contains("ghp_secret"));
        assert!(!encoded.contains("githubToken\":\""));
        assert!(encoded.contains("githubTokenConfigured"));
    }
    #[test]
    fn normalizes_and_exposes_pinned_methods() {
        let settings = Settings {
            pinned_methods: vec![
                "github".to_string(),
                "invalid".to_string(),
                "base".to_string(),
                "github".to_string(),
            ],
            ..Settings::default()
        };
        let normalized = settings.normalized().unwrap();
        assert_eq!(normalized.pinned_methods, vec!["github", "base"]);
        assert_eq!(
            normalized.public_view().pinned_methods,
            vec!["github", "base"]
        );
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
        assert_eq!(
            incoming
                .merged_with_existing_token(&existing)
                .unwrap()
                .github_token,
            "ghp_existing"
        );
    }
    #[test]
    fn rejects_token_with_whitespace_or_non_ascii() {
        assert_eq!(
            Settings {
                github_token: " ghp_demo\n".to_string(),
                ..Settings::default()
            }
            .normalized()
            .unwrap()
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
            assert!(
                Settings {
                    github_token: token.to_string(),
                    ..Settings::default()
                }
                .normalized()
                .is_err(),
                "{token:?}"
            );
        }
    }
    #[test]
    fn test_normalizes_cache_entries_limit() {
        assert_eq!(
            Settings {
                max_cache_entries: 0,
                ..Settings::default()
            }
            .normalized()
            .unwrap()
            .max_cache_entries,
            1
        );
        assert_eq!(
            Settings {
                max_cache_entries: 20000,
                ..Settings::default()
            }
            .normalized()
            .unwrap()
            .max_cache_entries,
            10000
        );
    }
    #[test]
    fn normalizes_search_concurrency_limit() {
        assert_eq!(
            Settings {
                search_concurrency: 0,
                ..Settings::default()
            }
            .normalized()
            .unwrap()
            .search_concurrency,
            1
        );
        assert_eq!(
            Settings {
                search_concurrency: 99,
                ..Settings::default()
            }
            .normalized()
            .unwrap()
            .search_concurrency,
            12
        );
    }
    #[test]
    fn normalizes_archive_github_major_gap_limit() {
        assert_eq!(
            Settings {
                archive_github_major_gap: 99,
                ..Settings::default()
            }
            .normalized()
            .unwrap()
            .archive_github_major_gap,
            10
        );
        assert_eq!(GenerateOptions::default().archive_github_major_gap, 1);
    }
}
