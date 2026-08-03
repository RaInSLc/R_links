use super::*;

#[cfg(test)]
mod cache_tests {
    use super::version_compatible;

    #[test]
    fn cache_version_must_match_requested_version() {
        assert!(version_compatible("1.2.3", "1.2.3"));
        assert!(version_compatible("1.2.3", "1.2"));
        assert!(!version_compatible("1.3.0", "1.2"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_sanitize::MAX_RESULT_MESSAGE_CHARS;
    #[test]
    fn builds_clients_for_supported_proxy_schemes() {
        for proxy in [
            "http://127.0.0.1:7890",
            "https://127.0.0.1:7890",
            "socks5://127.0.0.1:1080",
            "socks5h://127.0.0.1:1080",
        ] {
            let settings = Settings {
                proxy: proxy.to_string(),
                ..Settings::default()
            }
            .normalized()
            .expect("supported proxy should normalize");

            assert!(
                build_client(&settings).is_ok(),
                "supported proxy should build a client: {proxy}"
            );
        }
    }

    #[test]
    fn extracts_versions_from_sources() {
        assert_eq!(
            extract_html_version("<td>Version:</td><td>1.2.3</td>"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            extract_description_metadata("Package: demo\nVersion: 0.4.1\n"),
            Some(GithubDescription {
                package_name: "demo".to_string(),
                version: "0.4.1".to_string(),
            })
        );
    }

    #[test]
    fn accepts_major_minor_request() {
        assert!(version_compatible("1.50.2", "1.50"));
        assert!(!version_compatible("1.52.0", "1.50"));
    }

    #[test]
    fn sends_token_only_to_github_api() {
        let settings = Settings {
            github_token: "ghp_demo".to_string(),
            ..Settings::default()
        };
        assert!(should_attach_github_token(
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=stars&per_page=10",
            &settings
        ));
        assert!(!should_attach_github_token(
            "http://api.github.com/search/repositories?q=demo",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://r-universe.dev/api/search?q=package:demo",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://raw.githubusercontent.com/owner/repo/HEAD/DESCRIPTION",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://api.github.com/search/repositories?q=demo",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://api.github.com/search/repositories?q=owner%2Frepo+language%3AR&sort=stars&per_page=10",
            &settings
        ));
        assert!(!should_attach_github_token(
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=stars&per_page=10",
            &Settings::default()
        ));
    }

    #[test]
    fn validates_search_request_url_scope() {
        for url in [
            "https://cloud.r-project.org/web/packages/demo/index.html",
            "https://cloud.r-project.org/src/contrib/Archive/demo/",
            "https://cloud.r-project.org/src/contrib/Archive/demo",
            "https://bioconductor.org/packages/release/bioc/html/demo.html",
            "https://bioconductor.org/packages/3.18/bioc/html/demo.html",
            "https://bioconductor.org/packages/release/data/annotation/html/demo.html",
            "https://bioconductor.org/packages/3.18/data/experiment/html/demo.html",
            "https://r-universe.dev/api/search?q=package%3Ademo&limit=1",
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=stars&per_page=10",
            "https://raw.githubusercontent.com/owner/repo/HEAD/DESCRIPTION",
            "https://raw.githubusercontent.com/owner/repo/HEAD/path/DESCRIPTION",
        ] {
            assert!(validate_search_request_url(url).is_ok(), "{url}");
        }

        for url in [
            "http://cloud.r-project.org/web/packages/demo/index.html",
            "https://user:pass@api.github.com/search/repositories?q=demo",
            "https://api.github.com:443/search/repositories?q=demo",
            "https://cloud.r-project.org:443/web/packages/demo/index.html",
            "https://api.github.com/search/repositories?q=demo#token",
            "https://example.com/search/repositories?q=demo",
            "https://raw.githubusercontent.com/owner/repo/HEAD/DESCRIPTION?token=secret",
            "https://cloud.r-project.org/web/packages/demo/index.html?mirror=evil",
            "https://cloud.r-project.org/web/packages/owner/repo/index.html",
            "https://cloud.r-project.org/web/packages/demo/extra/index.html",
            "https://cloud.r-project.org/src/contrib/Archive/demo/extra",
            "https://cloud.r-project.org/src/contrib/Archive/demo/index.html",
            "https://cloud.r-project.org/src/contrib/Archive/demo/?mirror=evil",
            "https://bioconductor.org/packages/release/bioc/html/owner/repo.html",
            "https://bioconductor.org/packages/release/unknown/html/demo.html",
            "https://bioconductor.org/packages/release/data/unknown/html/demo.html",
            "https://bioconductor.org/packages/release/bioc/html/demo.html/extra",
            "https://r-universe.dev/api/search?q=package%3Ademo&limit=100",
            "https://r-universe.dev/api/search?limit=1&q=package%3Ademo",
            "https://r-universe.dev/api/search?q=package%3Ademo&q=package%3Aother&limit=1",
            "https://r-universe.dev/api/search?q=package%3A&limit=1",
            "https://r-universe.dev/api/search?q=owner%2Frepo&limit=1",
            "https://api.github.com/search/repositories?q=demo+language%3AR&sort=updated&per_page=10",
            "https://api.github.com/search/repositories?sort=stars&q=demo+language%3AR&per_page=10",
            "https://api.github.com/search/repositories?q=demo+language%3AR&q=other+language%3AR&sort=stars&per_page=10",
            "https://api.github.com/search/repositories?q=+language%3AR&sort=stars&per_page=10",
            "https://api.github.com/search/repositories?q=owner%2Frepo+language%3AR&sort=stars&per_page=10",
            "https://raw.githubusercontent.com/owner/repo/feature/DESCRIPTION",
            "https://raw.githubusercontent.com/owner/repo/HEAD/../DESCRIPTION",
        ] {
            assert!(validate_search_request_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn rejects_untrusted_github_repository_hosts() {
        assert_eq!(
            normalize_github_repository("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert!(normalize_github_repository("https://example.com/github.com/owner/repo").is_none());
        assert!(normalize_github_repository("https://github.com/owner/repo/issues").is_none());
        assert!(normalize_github_repository("https://github.com/owner/repo?tab=readme").is_none());
    }

    #[test]
    fn bounds_github_search_response_repositories() {
        let mut items = vec![
            GithubRepository {
                full_name: "owner/demo".to_string(),
            },
            GithubRepository {
                full_name: "owner/bad\nrepo".to_string(),
            },
            GithubRepository {
                full_name: format!("owner/{}", "x".repeat(MAX_GITHUB_REPOSITORY_CHARS + 1)),
            },
        ];
        items.extend((0..MAX_GITHUB_SEARCH_ITEMS).map(|index| GithubRepository {
            full_name: format!("owner/repo{index}"),
        }));

        let repositories = bounded_github_response_repositories(GithubSearchResponse { items });

        assert_eq!(repositories.len(), MAX_GITHUB_SEARCH_ITEMS - 2);
        assert_eq!(repositories.first().map(String::as_str), Some("owner/demo"));
        assert!(!repositories
            .iter()
            .any(|repository| repository == "owner/repo9"));
        assert!(repositories
            .iter()
            .all(|repository| repository.len() <= MAX_GITHUB_REPOSITORY_CHARS));
    }

    #[test]
    fn rejects_unbounded_or_controlled_versions() {
        assert_eq!(clean_version(" 1.2.3-rc1 "), Some("1.2.3-rc1".to_string()));
        assert!(clean_version("1.2.3\nInjected: yes").is_none());
        assert!(clean_version(&"1".repeat(65)).is_none());
        assert!(extract_description_metadata("Version: 1.0.0\n").is_none());
        assert!(extract_description_metadata("Package: demo\nVersion: 1.0.0<script>\n").is_none());
        assert!(extract_description_metadata("Package: demo\nVersion: 1.0.0\n").is_some());
    }

    #[test]
    fn validates_github_description_package_identity() {
        assert!(github_package_name_matches_request("Demo", "demo"));
        assert!(!github_package_name_matches_request("demoExtra", "demo"));
        assert!(!github_package_name_matches_request("demo\nbad", "demo"));
        assert!(extract_description_metadata("Package: demo\nVersion: 1.2.3\n").is_some());
        assert!(extract_description_metadata("Package: demo\nbad\nVersion: 1.2.3\n").is_none());
    }

    #[test]
    fn parses_ggsankey_description_metadata() {
        let description = "Package: ggsankey\nType: Package\nTitle: Sankey, Alluvial and Sankey Bump Plots\nVersion: 0.0.99999\nImports: \n    ggplot2,\n    dplyr,\n    stringr\n";
        let metadata = extract_description_metadata(description).expect("ggsankey DESCRIPTION 应可解析");
        assert_eq!(metadata.package_name, "ggsankey");
        assert_eq!(metadata.version, "0.0.99999");
    }

    #[test]
    fn bounds_github_description_metadata_scan() {
        assert!(extract_description_metadata(
            "Package: demo\nTitle: Demo package\n  continuation is allowed\nVersion: 1.2.3\n"
        )
        .is_some());

        let too_many_lines = format!(
            "{}Package: demo\nVersion: 1.2.3\n",
            "Author: demo\n".repeat(MAX_DESCRIPTION_LINES)
        );
        assert!(extract_description_metadata(&too_many_lines).is_none());

        let oversized_line = format!(
            "Package: demo\nTitle: {}\nVersion: 1.2.3\n",
            "x".repeat(MAX_DESCRIPTION_LINE_CHARS + 1)
        );
        assert!(extract_description_metadata(&oversized_line).is_none());
    }

    #[test]
    fn bounds_r_universe_package_object_shape() {
        let top_level = serde_json::json!({
            "Package": "demo",
            "Version": "1.0.0",
            "RemoteUrl": "https://github.com/owner/demo"
        });
        let array_response = serde_json::json!([
            {
                "Package": "demo",
                "Version": "1.0.0",
                "RemoteUrl": "https://github.com/owner/demo"
            },
            {
                "Package": "other",
                "Version": "9.9.9",
                "RemoteUrl": "https://github.com/owner/other"
            }
        ]);
        let invalid_first_array_response = serde_json::json!([
            {
                "Package": 42,
                "Version": "1.0.0",
                "RemoteUrl": "https://github.com/owner/wrong"
            },
            {
                "Package": "demo",
                "Version": "1.0.0",
                "RemoteUrl": "https://github.com/owner/demo"
            }
        ]);
        let oversized_response = serde_json::json!({
            "Package": "demo",
            "Version": "1.0.0",
            "RemoteUrl": "x".repeat(MAX_FIELD_CHARS + 1)
        });
        let nested_response = serde_json::json!({
            "meta": {
                "Package": "wrong",
                "Version": "9.9.9",
                "RemoteUrl": "https://github.com/owner/wrong"
            }
        });

        assert_eq!(
            r_universe_package_object(&top_level).and_then(|object| object.get("Package")),
            Some(&serde_json::json!("demo"))
        );
        assert_eq!(
            r_universe_package_object(&array_response).and_then(|object| object.get("Package")),
            Some(&serde_json::json!("demo"))
        );
        assert!(r_universe_package_object(&invalid_first_array_response).is_none());
        assert!(r_universe_package_object(&oversized_response).is_none());
        assert!(r_universe_package_object(&nested_response).is_none());
    }

    #[test]
    fn serializes_search_events_with_run_id() {
        let event = SearchLogBatchEvent {
            run_id: 42,
            messages: vec!["开始".to_string()],
        };
        let encoded = serde_json::to_string(&event).expect("事件应可序列化");

        assert!(encoded.contains("\"runId\":42"));
        assert!(encoded.contains("\"messages\""));
    }

    #[test]
    fn sanitizes_progress_results_before_emit() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "demo".to_string(),
            requested_version: "1.2.3\nbad".to_string(),
            latest_version: "9".repeat(65),
            repository: "https://example.com/owner/demo".to_string(),
            real_name: "demo\nbad".to_string(),
            source: "github<script>".to_string(),
            found: true,
            message: format!("ok\n{}", "x".repeat(MAX_RESULT_MESSAGE_CHARS + 20)),
            status: "found".to_string(),
            stage: "final".to_string(),
        });

        assert_eq!(result.package, "demo");
        assert!(result.requested_version.is_empty());
        assert!(result.latest_version.is_empty());
        assert!(result.repository.is_empty());
        assert_eq!(result.real_name, "demo");
        assert_eq!(result.source, "none");
        assert!(!result.message.contains('\n'));
        assert!(result.message.len() <= MAX_RESULT_MESSAGE_CHARS);
    }

}
