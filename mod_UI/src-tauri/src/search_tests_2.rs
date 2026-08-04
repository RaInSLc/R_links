use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_sanitize::{MAX_SEARCH_LOG_CHARS, SEARCH_LOG_EMPTY_MESSAGE};
    #[test]
    fn downgrades_untrusted_progress_results_before_emit() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "demo".to_string(),
            requested_version: String::new(),
            latest_version: "1.2.3".to_string(),
            repository: String::new(),
            real_name: "demo".to_string(),
            source: "github".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
            stage: "final".to_string(),
        });

        assert!(!result.found);
        assert_eq!(result.source, "none");
        assert!(result.latest_version.is_empty());
        assert!(result.repository.is_empty());
        assert_eq!(result.message, "结果字段无效，已忽略");
    }

    #[test]
    fn preserves_explicit_github_progress_result_identity() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "owner/repo".to_string(),
            requested_version: String::new(),
            latest_version: "1.2.3".to_string(),
            repository: "owner/repo".to_string(),
            real_name: "actualPkg".to_string(),
            source: "github".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
            stage: "final".to_string(),
        });

        assert!(result.found);
        assert_eq!(result.source, "github");
        assert_eq!(result.repository, "owner/repo");
        assert_eq!(result.real_name, "actualPkg");
    }

    #[test]
    fn found_result_tracking_ignores_downgraded_or_unrelated_results() {
        let results = vec![
            SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: String::new(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "none".to_string(),
                found: false,
                message: "未找到".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            },
            SearchResult {
                package: "other".to_string(),
                requested_version: String::new(),
                latest_version: "1.0.0".to_string(),
                repository: String::new(),
                real_name: "other".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            },
        ];

        assert!(!has_found_result_for_package(&results, "demo"));
        assert!(has_found_result_for_package(&results, "other"));
    }

    #[test]
    fn found_result_tracking_accepts_explicit_github_package_identity() {
        let results = vec![SearchResult {
            package: "owner/repo".to_string(),
            requested_version: String::new(),
            latest_version: "1.0.0".to_string(),
            repository: "owner/repo".to_string(),
            real_name: "actualPkg".to_string(),
            source: "github".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
            stage: "final".to_string(),
        }];

        assert!(has_found_result_for_package(&results, "Owner/Repo"));
        assert!(!has_found_result_for_package(&results, "owner/other"));
    }

    #[test]
    fn sanitizes_search_log_messages() {
        let message = sanitize_log_message(&format!(
            " ok\nbad\t{} ",
            "x".repeat(MAX_SEARCH_LOG_CHARS + 20)
        ));

        assert!(!message.contains('\n'));
        assert!(!message.contains('\t'));
        assert!(message.starts_with("okbad"));
        assert_eq!(message.chars().count(), MAX_SEARCH_LOG_CHARS);
        assert_eq!(sanitize_log_message("\n\t"), SEARCH_LOG_EMPTY_MESSAGE);
    }

    #[test]
    fn bounds_search_log_count() {
        let mut logs = Vec::new();

        for index in 0..(MAX_SEARCH_LOGS + 10) {
            let emitted = append_search_log(&mut logs, &format!("log {index}"));
            if index < MAX_SEARCH_LOGS {
                assert!(emitted.is_some());
            } else {
                assert!(emitted.is_none());
            }
        }

        assert_eq!(logs.len(), MAX_SEARCH_LOGS);
        assert_eq!(
            logs.last().map(String::as_str),
            Some(SEARCH_LOGS_TRUNCATED_MESSAGE)
        );
    }

    #[test]
    fn bounds_search_result_count_directly() {
        let package = PackageInput {
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: String::new(),
            source_hint: None,
        };
        let mut results = Vec::new();

        for index in 0..5 {
            let result = found_result(&package, &format!("1.0.{index}"), "", "demo", "cran");
            assert_eq!(
                append_bounded_search_result(&mut results, result, 3),
                index < 3
            );
        }

        assert_eq!(results.len(), 3);
        assert_eq!(results[2].latest_version, "1.0.2");
    }

    #[test]
    fn preserves_bioc_git_repository_version_in_results() {
        let package = PackageInput {
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: String::new(),
            source_hint: None,
        };

        let result = found_result(&package, "1.2.3", "3.18", "demo", "biocGit");

        assert_eq!(result.repository, "3.18");
        assert_eq!(result.source, "biocGit");
    }

    #[test]
    fn invalid_github_real_name_downgrades_result() {
        let package = PackageInput {
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: String::new(),
            source_hint: None,
        };

        let result = found_result(&package, "1.2.3", "owner/demo", "demo\nbad", "github");

        assert!(!result.found);
        assert_eq!(result.source, "none");
        assert!(result.repository.is_empty());
        assert_eq!(result.real_name, "demo");
    }

    #[test]
    fn request_budget_rejects_after_limit() {
        let budget = RequestBudget::new(2);

        assert!(budget.try_acquire().is_ok());
        assert!(budget.try_acquire().is_ok());
        assert!(budget.try_acquire().is_err());
        assert!(budget.is_exhausted());
        assert_eq!(budget.remaining_for_test(), 0);
    }

    #[test]
    fn search_stops_when_cancelled_or_budget_exhausted() {
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(1);

        assert!(!search_stopped(&cancelled, &budget));
        assert!(budget.try_acquire().is_ok());
        assert!(!search_stopped(&cancelled, &budget));
        assert!(budget.try_acquire().is_err());
        assert!(search_stopped(&cancelled, &budget));

        let fresh_budget = RequestBudget::new(1);
        cancelled.store(true, Ordering::SeqCst);
        assert!(search_stopped(&cancelled, &fresh_budget));
    }

    #[test]
    fn search_context_log_appends_without_emitter() {
        let mut logs = Vec::new();
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(1);
        let timed_out = AtomicBool::new(false);
        let settings = Settings::default();
        let client = build_client(&settings).unwrap();

        let mut context = SearchContext {
            log_emitter: None,
            client: &client,
            settings: &settings,
            cancelled: &cancelled,
            budget: &budget,
            deadline: Instant::now() + Duration::from_secs(10),
            timed_out: &timed_out,
            logs: &mut logs,
            result_limit_reached: false,
            github_rate_limited: false,
        };

        context.log("流式日志测试");

        assert_eq!(logs, vec!["流式日志测试"]);
    }

    #[tokio::test]
    async fn test_search_cran_mock_network() {
        let mut logs = Vec::new();
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(10);
        let timed_out = AtomicBool::new(false);
        let settings = Settings::default();
        let client = build_client(&settings).unwrap();

        let mut context = SearchContext {
            log_emitter: None,
            client: &client,
            settings: &settings,
            cancelled: &cancelled,
            budget: &budget,
            deadline: Instant::now() + Duration::from_secs(10),
            timed_out: &timed_out,
            logs: &mut logs,
            result_limit_reached: false,
            github_rate_limited: false,
        };

        let package = PackageInput {
            raw: "mockPkg".to_string(),
            name: "mockPkg".to_string(),
            version: "".to_string(),
            source_hint: None,
        };

        MOCK_GET_TEXT.with(|mock| {
            *mock.borrow_mut() = Some(Box::new(|url| {
                if url.contains("index.html") {
                    Ok(Some("<td>Version:</td><td>9.9.9</td>".to_string()))
                } else {
                    Ok(None)
                }
            }));
        });

        let result = search_cran(&mut context, &package).await;

        MOCK_GET_TEXT.with(|mock| *mock.borrow_mut() = None);

        assert!(result.is_ok());
        let opt = result.unwrap();
        assert!(opt.is_some());
        let res = opt.unwrap();
        assert_eq!(res.latest_version, "9.9.9");
        assert_eq!(res.source, "cran");
    }

    #[tokio::test]
    async fn search_cran_archive_uses_tarball_repository_url() {
        let mut logs = Vec::new();
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(10);
        let timed_out = AtomicBool::new(false);
        let settings = Settings::default();
        let client = build_client(&settings).unwrap();

        let mut context = SearchContext {
            log_emitter: None,
            client: &client,
            settings: &settings,
            cancelled: &cancelled,
            budget: &budget,
            deadline: Instant::now() + Duration::from_secs(10),
            timed_out: &timed_out,
            logs: &mut logs,
            result_limit_reached: false,
            github_rate_limited: false,
        };

        let package = PackageInput {
            raw: "fastshap".to_string(),
            name: "fastshap".to_string(),
            version: String::new(),
            source_hint: None,
        };

        MOCK_GET_TEXT.with(|mock| {
            *mock.borrow_mut() = Some(Box::new(|url| {
                if url.contains("/web/packages/fastshap/index.html") {
                    Ok(None)
                } else if url.contains("/src/contrib/Archive/fastshap/") {
                    Ok(Some(
                        r#"<a href="fastshap_0.1.1.tar.gz">fastshap_0.1.1.tar.gz</a>"#.to_string(),
                    ))
                } else {
                    Ok(None)
                }
            }));
        });

        let result = search_cran(&mut context, &package).await;

        MOCK_GET_TEXT.with(|mock| *mock.borrow_mut() = None);

        let result = result
            .expect("Archive 检索不应报错")
            .expect("应命中 Archive");
        assert_eq!(result.latest_version, "0.1.1");
        assert_eq!(result.source, "cran");
        assert_eq!(
            result.repository,
            "https://cran.r-project.org/src/contrib/Archive/fastshap/fastshap_0.1.1.tar.gz"
        );
    }

    #[tokio::test]
    async fn test_search_github_mock_network() {
        let mut logs = Vec::new();
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(10);
        let timed_out = AtomicBool::new(false);
        let settings = Settings::default();
        let client = build_client(&settings).unwrap();

        let mut context = SearchContext {
            log_emitter: None,
            client: &client,
            settings: &settings,
            cancelled: &cancelled,
            budget: &budget,
            deadline: Instant::now() + Duration::from_secs(10),
            timed_out: &timed_out,
            logs: &mut logs,
            result_limit_reached: false,
            github_rate_limited: false,
        };

        let package = PackageInput {
            raw: "mockRepo".to_string(),
            name: "mockRepo".to_string(),
            version: "".to_string(),
            source_hint: None,
        };

        MOCK_GET_JSON.with(|mock| {
            *mock.borrow_mut() = Some(Box::new(|url| {
                if url.contains("r-universe.dev") {
                    Ok(Some(serde_json::json!({
                        "Package": "mockRepo",
                        "Version": "1.0.0",
                        "RemoteUrl": "https://github.com/owner/mockRepo"
                    })))
                } else {
                    Ok(None)
                }
            }));
        });

        let result = search_github(&mut context, &package).await;
        MOCK_GET_JSON.with(|mock| *mock.borrow_mut() = None);

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(!res.is_empty());
        assert_eq!(res[0].latest_version, "1.0.0");
        assert_eq!(res[0].repository, "owner/mockRepo");
    }

    #[test]
    fn search_facade_preserves_event_type_identity() {
        let event = SearchProgressEvent {
            run_id: 7,
            result: SearchResult::default(),
        };
        assert_eq!(event.run_id, 7);
        assert_eq!(event.result, SearchResult::default());
    }
}
