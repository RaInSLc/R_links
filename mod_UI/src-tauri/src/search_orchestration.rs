use super::*;
use super::{append_search_log, log};

#[path = "search_package_pipeline.rs"]
mod search_package_pipeline;

#[allow(unused_imports)]
pub(crate) use search_package_pipeline::{has_found_result_for_package, search_one_package};

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

    let cache_revision = storage::cache_revision();
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
    let cache_index = index_cache(&cache);

    while processed < packages.len() {
        while state.is_paused(run_id)
            && !cancelled.load(Ordering::SeqCst)
            && Instant::now() < deadline
        {
            sleep(SEARCH_STOP_POLL_INTERVAL).await;
        }
        if search_stopped(cancelled, &budget) || timed_out.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() >= deadline {
            timed_out.store(true, Ordering::SeqCst);
            break;
        }

        let batch_size = packages.len() - processed;
        let batch = &packages[processed..processed + batch_size];
        let batch_start = processed;

        let mut batch_tasks = Vec::new();
        let cached_start = results.len();
        for (offset, package) in batch.iter().enumerate() {
            if cancelled.load(Ordering::SeqCst) || Instant::now() >= deadline {
                break;
            }
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
            let cached_entry = cache_index
                .get(&package.name)
                .into_iter()
                .flatten()
                .copied()
                .filter(|entry| {
                    entry.is_trusted()
                        && (package.version.is_empty()
                            || version_compatible(&entry.version, &package.version))
                })
                .filter(|entry| {
                    if entry.source == "github" {
                        crate::logic::normalize_github_repository(&package.name)
                            .is_some_and(|repository| entry.repository == repository)
                    } else {
                        matches!(
                            entry.source.as_str(),
                            "cran" | "bioc" | "biocGit" | "r-forge"
                        ) && entry.package_name == package.name
                            && !package.name.contains('/')
                    }
                })
                .min_by_key(|entry| {
                    (
                        match entry.source.as_str() {
                            "cran" => 0,
                            "bioc" => 1,
                            "biocGit" => 2,
                            "github" => 3,
                            _ => 4,
                        },
                        &entry.repository,
                    )
                });

            if let Some(cached_entry) =
                cached_entry
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
                tokio::task::yield_now().await;
                continue;
            } else if cached_entry.is_some() {
                log(
                    app,
                    run_id,
                    &mut logs,
                    &format!("[{}/{}] {} (缓存待验证)", index + 1, total, package.name),
                );
            }

            batch_tasks.push((index, package.clone()));
        }

        emit_result_batch(app, run_id, &results[cached_start..]);
        if batch_tasks.is_empty() {
            processed += batch_size;
            continue;
        }

        let mut futures = futures_util::stream::iter(batch_tasks)
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
                    while state.is_paused(run_id)
                        && !cancelled_ref.load(Ordering::SeqCst)
                        && Instant::now() < deadline
                    {
                        sleep(SEARCH_STOP_POLL_INTERVAL).await;
                    }
                    if state.is_package_cancelled(run_id, &pkg.name)
                        || cancelled_ref.load(Ordering::SeqCst)
                        || Instant::now() >= deadline
                    {
                        return (task_results, task_logs);
                    }
                    let mut context = SearchContext {
                        log_emitter: None,
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
                    search_one_package(&mut context, &mut task_results, &pkg, index, total).await;
                    if state.is_package_cancelled(run_id, &pkg.name) {
                        task_results.clear();
                    }
                    (task_results, task_logs)
                }
            })
            .buffer_unordered(settings.search_concurrency.max(1));

        while let Some((task_results_inner, task_logs)) = futures.next().await {
            emit_log_batch(app, run_id, &task_logs);
            for msg in &task_logs {
                let _ = append_search_log(&mut logs, msg);
            }

            for result in &task_results_inner {
                let result_key = storage::package_cache_key(
                    &result.source,
                    &result.real_name,
                    &result.repository,
                );
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

            let batch_start = results.len();
            for result in task_results_inner {
                if results.len() >= MAX_SEARCH_RESULTS {
                    log(app, run_id, &mut logs, SEARCH_RESULTS_TRUNCATED_MESSAGE);
                    break;
                }
                let sanitized = sanitize_search_result_for_emit(result);
                results.push(sanitized.clone());
                tokio::task::yield_now().await;
            }
            emit_result_batch(app, run_id, &results[batch_start..]);
        }

        processed += batch_size;
    }

    drop(cache_index);
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
        if let Err(error) = storage::save_search_cache(app, &cache, cache_revision) {
            log(app, run_id, &mut logs, &format!("缓存保存失败: {error}"));
        }
    }

    let dependency_stage_start = Instant::now();
    let dependency_graph = if settings.resolve_dependencies
        && !search_stopped(cancelled, &budget)
        && !timed_out.load(Ordering::SeqCst)
        && Instant::now() < deadline
    {
        log(app, run_id, &mut logs, "开始解析 R 包依赖关系图...");
        match await_or_stop(
            crate::dependency::resolve_dependencies(
                app, &client, &results, settings, cancelled, &budget, deadline,
            ),
            cancelled,
            &budget,
            deadline,
        )
        .await
        .and_then(|result| result)
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
        stopped: timed_out.load(Ordering::SeqCst)
            || search_stopped(cancelled, &budget)
            || Instant::now() >= deadline,
        stage_timings,
        dependency_graph,
    })
}
