use super::*;

pub async fn search_binary_packages(
    app: &AppHandle,
    run_id: u64,
    cancelled: &AtomicBool,
    state: &crate::SearchState,
    input: &str,
    settings: &Settings,
    mirror: &str,
) -> Result<SearchResponse, String> {
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
        return Err("请输入至少一个有效的 R 二进制包名".to_string());
    }
    let mirror = crate::models::normalize_cran_mirror_url(mirror)?;
    let mut results = Vec::new();
    let mut logs = Vec::new();
    log(app, run_id, &mut logs, &format!("开始 R 二进制包命令生成，镜像: {mirror}"));

    for (index, package) in packages.iter().enumerate() {
        if state.is_paused(run_id) {
            sleep(SEARCH_STOP_POLL_INTERVAL).await;
        }
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        log(app, run_id, &mut logs, &format!("[{}/{}] 已按输入生成 R 二进制安装命令 {}", index + 1, packages.len(), package.name));
        let result = SearchResult {
            package: package.name.clone(),
            requested_version: package.version.clone(),
            latest_version: if package.version.is_empty() { "unknown".to_string() } else { package.version.clone() },
            repository: mirror.clone(),
            real_name: package.name.clone(),
            source: "cran-binary".to_string(),
            found: true,
            message: "已生成安装命令，未执行网络检索".to_string(),
            status: "found".to_string(),
            stage: "final".to_string(),
        };
        let _ = app.emit("search-progress", SearchProgressEvent { run_id, result: result.clone() });
        results.push(result);
    }
    log(app, run_id, &mut logs, "R 二进制安装命令生成完成（未执行网络检索）");
    Ok(SearchResponse { run_id, results, logs, stopped: cancelled.load(Ordering::SeqCst), stage_timings: Vec::new(), dependency_graph: None })
}
