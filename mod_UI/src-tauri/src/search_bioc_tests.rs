use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bioc_history_continues_after_single_version_request_failure() {
        let mut logs = Vec::new();
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(50);
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
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: "1.50.0".to_string(),
            source_hint: None,
        };

        let mut attempts = 0usize;
        MOCK_GET_TEXT.with(|mock| {
            *mock.borrow_mut() = Some(Box::new(move |url| {
                if !url.ends_with("/html/demo.html") {
                    return Ok(None);
                }
                attempts += 1;
                if attempts == 1 {
                    return Err("模拟首个 Bioc 版本请求失败".to_string());
                }
                Ok(Some("<td>Version:</td><td>1.50.0</td>".to_string()))
            }));
        });

        let result = find_bioc_history(&mut context, &package, "bioc").await;
        MOCK_GET_TEXT.with(|mock| *mock.borrow_mut() = None);

        let result = result
            .expect("单版本请求失败不应中断整个版本遍历")
            .expect("应命中后续 Bioc 版本");
        assert_eq!(result.latest_version, "1.50.0");
        assert_eq!(result.source, "biocGit");
    }

    #[tokio::test]
    async fn bioc_history_requests_inferred_version_first() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut logs = Vec::new();
        let cancelled = AtomicBool::new(false);
        let budget = RequestBudget::new(50);
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
            raw: "demo".to_string(),
            name: "demo".to_string(),
            version: "1.50.0".to_string(),
            source_hint: None,
        };

        let first_url = Rc::new(RefCell::new(String::new()));
        let recorder = Rc::clone(&first_url);
        MOCK_GET_TEXT.with(|mock| {
            *mock.borrow_mut() = Some(Box::new(move |url| {
                let mut slot = recorder.borrow_mut();
                if slot.is_empty() {
                    *slot = url.to_string();
                }
                Ok(None)
            }));
        });

        let result = find_bioc_history(&mut context, &package, "bioc").await;
        MOCK_GET_TEXT.with(|mock| *mock.borrow_mut() = None);

        assert!(result.is_ok());
        assert!(
            first_url.borrow().contains("/packages/3.18/"),
            "首个请求应命中推断出的 Bioc 版本 3.18，实际为: {}",
            first_url.borrow()
        );
    }
}
