use super::*;
use super::{append_search_log, log};

pub async fn search_packages(
    app: &AppHandle,
    run_id: u64,
    cancelled: &AtomicBool,
    state: &crate::SearchState,
    input: &str,
    settings: &Settings,
) -> Result<SearchResponse, String> {
    if run_id == 0 {
        return Err("检索任务 ID 无效".to_string());
    }
    let rules = if settings.use_filter {
        storage::load_input_rules(app)
    } else {
        InputRules {
            separators: Vec::new(),
            strip_quotes: true,
            strip_c_parens: true,
            comment_chars: Vec::new(),
            split_spaces: false,
            exclude_regex: Vec::new(),
            exclude_keywords: Vec::new(),
        }
    };
    let packages = parse_inputs_filtered(input, &rules)?;
    if packages.is_empty() {
        return Err("请输入至少一个有效的 R 包".to_string());
    }

    let client = build_client(settings)?;
    let budget = RequestBudget::new(MAX_SEARCH_HTTP_REQUESTS);
    let timed_out = AtomicBool::new(false);
    let deadline = Instant::now() + MAX_SEARCH_DURATION;
    let mut results = Vec::new();
    let mut logs = Vec::new();
    let mut cache_update: HashMap<String, PackageCacheEntry> = HashMap::new();
    let cache_stage_start = Instant::now();

    let cache = if settings.use_cache {
        match storage::load_cache(app) {
            Ok(cache) => cache,
            Err(error) => {
                log(app, run_id, &mut logs, &format!("缓存加载失败: {error}"));
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    log(
        app,
        run_id,
        &mut logs,
        &format!(
            "开始多源检索（超时 {} 秒，最大并发 {}）",
            MAX_SEARCH_DURATION.as_secs(),
            settings.search_concurrency
        ),
    );

    let total = packages.len();
    let mut cache = cache;
    let cache_stage_ms = cache_stage_start.elapsed().as_millis() as u64;
    let search_stage_start = Instant::now();
    let mut processed = 0usize;

    while processed < packages.len() {
        while state.is_paused(run_id) && !cancelled.load(Ordering::SeqCst) {
            sleep(SEARCH_STOP_POLL_INTERVAL).await;
        }
        if search_stopped(cancelled, &budget) || timed_out.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() >= deadline {
            timed_out.store(true, Ordering::SeqCst);
            break;
        }

        let batch_size = settings.search_concurrency.min(packages.len() - processed);
        let batch = &packages[processed..processed + batch_size];
        let batch_start = processed;

        let mut batch_tasks = Vec::new();
        for (offset, package) in batch.iter().enumerate() {
            let index = batch_start + offset;
            if state.is_package_cancelled(run_id, &package.name) {
                log(
                    app,
                    run_id,
                    &mut logs,
                    &format!("[{}/{}] {} 已取消", index + 1, total, package.name),
                );
                continue;
            }
            let cache_key = package.name.to_ascii_lowercase();

            if let Some(cached_entry) = cache
                .get(&cache_key)
                .filter(|entry| entry.is_trusted())
                .filter(|entry| {
                    package.version.is_empty()
                        || version_compatible(&entry.version, &package.version)
                })
            {
                log(
                    app,
                    run_id,
                    &mut logs,
                    &format!("[{}/{}] {} (缓存命中)", index + 1, total, package.name),
                );
                results.push(SearchResult {
                    package: package.name.clone(),
                    requested_version: package.version.clone(),
                    latest_version: cached_entry.version.clone(),
                    repository: cached_entry.repository.clone(),
                    real_name: cached_entry.real_name.clone(),
                    source: cached_entry.source.clone(),
                    found: true,
                    message: "缓存命中".to_string(),
                    status: "found".to_string(),
                    stage: "cacheHit".to_string(),
                });
                let _ = app.emit(
                    "search-progress",
                    SearchProgressEvent {
                        run_id,
                        result: results.last().expect("刚刚推送的结果应存在").clone(),
                    },
                );
                sleep(STREAM_RESULT_PAUSE).await;
                continue;
            } else if cache.contains_key(&cache_key) {
                log(
                    app,
                    run_id,
                    &mut logs,
                    &format!("[{}/{}] {} (缓存待验证)", index + 1, total, package.name),
                );
            }

            batch_tasks.push((index, package.clone()));
        }

        if batch_tasks.is_empty() {
            processed += batch_size;
            continue;
        }

        let mut futures: FuturesUnordered<_> = batch_tasks
            .iter()
            .map(|(index, package)| {
                let pkg = package.clone();
                let client_ref = &client;
                let settings_ref = settings;
                let cancelled_ref = cancelled;
                let budget_ref = &budget;
                let timed_out_ref = &timed_out;
                async move {
                    let mut task_logs = Vec::new();
                    let mut task_results = Vec::new();
                    let mut context = SearchContext {
                        log_emitter: Some((app, run_id)),
                        client: client_ref,
                        settings: settings_ref,
                        cancelled: cancelled_ref,
                        budget: budget_ref,
                        deadline,
                        timed_out: timed_out_ref,
                        logs: &mut task_logs,
                        result_limit_reached: false,
                        github_rate_limited: false,
                    };
                    search_one_package(&mut context, &mut task_results, &pkg, *index, total).await;
                    (task_results, task_logs)
                }
            })
            .collect();

        while let Some((task_results_inner, task_logs)) = futures.next().await {
            for msg in &task_logs {
                let _ = append_search_log(&mut logs, msg);
            }

            for result in &task_results_inner {
                let result_key = result.package.to_ascii_lowercase();
                if result.found
                    && matches!(
                        result.source.as_str(),
                        "cran" | "cran-binary" | "bioc" | "biocGit" | "github" | "r-forge"
                    )
                {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .to_string();
                    let existing = cache_update
                        .get(&result_key)
                        .or_else(|| cache.get(&result_key));
                    cache_update.insert(result_key, cache_entry_from_result(result, existing, now));
                }
            }

            for result in task_results_inner {
                if results.len() >= MAX_SEARCH_RESULTS {
                    log(app, run_id, &mut logs, SEARCH_RESULTS_TRUNCATED_MESSAGE);
                    break;
                }
                let sanitized = sanitize_search_result_for_emit(result);
                results.push(sanitized.clone());
                let _ = app.emit(
                    "search-progress",
                    SearchProgressEvent {
                        run_id,
                        result: sanitized,
                    },
                );
                sleep(STREAM_RESULT_PAUSE).await;
            }
        }

        processed += batch_size;
    }

    for (key, entry) in &cache_update {
        cache.insert(key.clone(), entry.clone());
    }
    let search_stage_ms = search_stage_start.elapsed().as_millis() as u64;

    let final_message = if timed_out.load(Ordering::SeqCst) {
        "检索任务已超时停止"
    } else if search_stopped(cancelled, &budget) {
        "检索任务已停止"
    } else {
        "检索任务已完成"
    };
    log(app, run_id, &mut logs, final_message);

    if settings.use_cache {
        if let Err(error) = storage::save_cache(app, &cache) {
            log(app, run_id, &mut logs, &format!("缓存保存失败: {error}"));
        }
    }

    let dependency_stage_start = Instant::now();
    let dependency_graph = if settings.resolve_dependencies && !search_stopped(cancelled, &budget) {
        log(app, run_id, &mut logs, "开始解析 R 包依赖关系图...");
        match crate::dependency::resolve_dependencies(app, &client, &results, settings, cancelled)
            .await
        {
            Ok(graph) => {
                log(
                    app,
                    run_id,
                    &mut logs,
                    &format!(
                        "依赖关系解析完成，共构建 {} 个包节点，{} 条依赖关系",
                        graph.summary.total_nodes, graph.summary.total_edges
                    ),
                );
                Some(graph)
            }
            Err(error) => {
                log(app, run_id, &mut logs, &format!("依赖解析失败: {error}"));
                None
            }
        }
    } else {
        None
    };

    let stage_timings = vec![
        crate::models::SearchStageTiming {
            stage: "缓存与输入".to_string(),
            duration_ms: cache_stage_ms,
        },
        crate::models::SearchStageTiming {
            stage: "多源检索".to_string(),
            duration_ms: search_stage_ms,
        },
        crate::models::SearchStageTiming {
            stage: "依赖解析".to_string(),
            duration_ms: dependency_stage_start.elapsed().as_millis() as u64,
        },
    ];
    Ok(SearchResponse {
        run_id,
        results,
        logs,
        stopped: timed_out.load(Ordering::SeqCst) || search_stopped(cancelled, &budget),
        stage_timings,
        dependency_graph,
    })
}

pub(crate) async fn search_one_package(
    context: &mut SearchContext<'_>,
    results: &mut Vec<SearchResult>,
    package: &PackageInput,
    index: usize,
    total: usize,
) {
    let mut loop_package = package.clone();
    let mut has_retried_casing = false;
    let mut errors = Vec::new();

    let mut attempts = 0;
    loop {
        if context.should_stop() {
            break;
        }

        context.log(&format!(
            "[{}/{}] 检索 {}{}",
            index + 1,
            total,
            loop_package.name,
            if loop_package.version.is_empty() {
                String::new()
            } else {
                format!(" {}", loop_package.version)
            }
        ));

        let had_found_before = has_found_result_for_package(results, &loop_package.name);
        if loop_package.name.contains('/') {
            match search_explicit_github(context, &loop_package).await {
                Ok(Some(result)) => {
                    results.push(result);
                }
                Ok(None) => {}
                Err(error) => {
                    errors.push(format!("GitHub仓库验证失败: {error}"));
                }
            }
        } else {
            match search_cran(context, &loop_package).await {
                Ok(Some(result)) => {
                    results.push(result);
                }
                Ok(None) => {}
                Err(error) => {
                    context.log(&format!(
                        "CRAN 检索失败（{}）: {error}",
                        context.settings.cran_mirror
                    ));
                    errors.push(format!("CRAN 检索失败: {error}"));
                }
            }

            if (context.settings.full_search
                || !has_found_result_for_package(results, &loop_package.name))
                && !context.should_stop()
            {
                match search_bioconductor(context, &loop_package).await {
                    Ok(bioc_results) => {
                        for result in bioc_results {
                            results.push(result);
                        }
                    }
                    Err(error) => {
                        context.log(&format!("Bioconductor 检索失败: {error}"));
                        errors.push(format!("Bioconductor 检索失败: {error}"));
                    }
                }
            }

            if (context.settings.full_search
                || !has_found_result_for_package(results, &loop_package.name))
                && !context.should_stop()
            {
                match search_github(context, &loop_package).await {
                    Ok(github_results) => {
                        let mut casing_diff_name = None;
                        if !has_retried_casing {
                            for res in &github_results {
                                if res.found
                                    && !res.real_name.is_empty()
                                    && res.real_name != loop_package.name
                                    && res.real_name.eq_ignore_ascii_case(&loop_package.name)
                                {
                                    casing_diff_name = Some(res.real_name.clone());
                                    break;
                                }
                            }
                        }

                        if let Some(corrected_name) = casing_diff_name {
                            context.log(&format!(
                                "检测到包名大小写差异，纠正为: {}，重新进行检索...",
                                corrected_name
                            ));
                            results.retain(|r| !r.package.eq_ignore_ascii_case(&loop_package.name));

                            loop_package.name = corrected_name;
                            has_retried_casing = true;
                            continue;
                        }

                        for result in github_results {
                            results.push(result);
                        }
                    }
                    Err(error) => {
                        context.log(&format!("GitHub/R-Universe 检索失败: {error}"));
                        errors.push(format!("GitHub 检索失败: {error}"));
                    }
                }
            }

            if (context.settings.full_search
                || !has_found_result_for_package(results, &loop_package.name))
                && !context.should_stop()
            {
                context.log("尝试 R-Forge 仓库检索...");
                match search_r_forge(context, &loop_package).await {
                    Ok(Some(result)) => {
                        results.push(result);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        context.log(&format!("R-Forge 检索失败: {error}"));
                        errors.push(format!("R-Forge 检索失败: {error}"));
                    }
                }
            }
        }

        if results
            .iter()
            .any(|result| result.found && result.package.eq_ignore_ascii_case(&loop_package.name))
        {
            break;
        }
        if attempts < AUTO_RETRY_LIMIT
            && errors.iter().any(|error| {
                error.contains("超时")
                    || error.contains("限流")
                    || error.contains("网络")
                    || error.contains("请求")
            })
            && !context.should_stop()
        {
            attempts += 1;
            context.log(&format!(
                "{} 检索失败，正在自动重试 ({}/{})",
                loop_package.name, attempts, AUTO_RETRY_LIMIT
            ));
            errors.clear();
            continue;
        }

        if !had_found_before
            && !has_found_result_for_package(results, &loop_package.name)
            && !context.should_stop()
        {
            let (message, status) = if context.timed_out.load(Ordering::SeqCst) {
                ("检索超时，部分来源未查询".to_string(), "timeout")
            } else if context.github_rate_limited {
                (
                    "GitHub API 频率限制，部分来源未查询".to_string(),
                    "rateLimited",
                )
            } else if !errors.is_empty() {
                (errors.join("; "), "error")
            } else {
                ("所有来源均未找到".to_string(), "notFound")
            };
            results.push(SearchResult {
                package: loop_package.name.clone(),
                requested_version: loop_package.version.clone(),
                latest_version: String::new(),
                repository: String::new(),
                real_name: loop_package.name.clone(),
                source: "none".to_string(),
                found: false,
                message,
                status: status.to_string(),
                stage: "final".to_string(),
            });
        }

        break;
    }
}

pub(crate) fn has_found_result_for_package(results: &[SearchResult], package_name: &str) -> bool {
    results.iter().any(|result| {
        result.found
            && (result.package.eq_ignore_ascii_case(package_name)
                || normalize_github_repository(package_name)
                    .as_deref()
                    .is_some_and(|repository| result.package.eq_ignore_ascii_case(repository)))
    })
}
