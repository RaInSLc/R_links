use super::*;

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
