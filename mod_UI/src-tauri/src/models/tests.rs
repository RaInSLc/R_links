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
