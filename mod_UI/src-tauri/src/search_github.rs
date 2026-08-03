use super::*;

pub(crate) async fn search_explicit_github(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Option<SearchResult>, String> {
    context.log("验证指定 GitHub 仓库");
    let Some(repository) = normalize_github_repository(&package.name) else {
        context.log("GitHub 仓库格式无效，已跳过");
        return Ok(None);
    };
    let description = match github_description(context, &repository).await {
        Ok(Some(desc)) => desc,
        Ok(None) => {
            let package_name = repository
                .rsplit('/')
                .next()
                .unwrap_or(&repository)
                .to_string();
            GithubDescription {
                package_name,
                version: "unknown".to_string(),
            }
        }
        Err(error) => {
            return Err(error);
        }
    };
    Ok(Some(found_result(
        package,
        &description.version,
        &repository,
        &description.package_name,
        "github",
    )))
}

pub(crate) async fn search_github(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let universe_url = format!(
        "https://r-universe.dev/api/search?q=package:{}&limit=1",
        urlencoding::encode(&package.name)
    );
    let universe_res = get_json(context, &universe_url).await;
    if let Ok(Some(value)) = universe_res {
        if let Some(object) = r_universe_package_object(&value) {
            if let Some(real_name) = object.get("Package").and_then(Value::as_str) {
                if !github_package_name_matches_request(real_name, &package.name) {
                    // 忽略不可信的 r-universe 命中，继续尝试 GitHub API 检索。
                } else {
                    let version = object
                        .get("Version")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let repository = object
                        .get("RemoteUrl")
                        .and_then(Value::as_str)
                        .and_then(normalize_github_repository);
                    if let Some(repository) = repository {
                        seen.insert(repository.to_ascii_lowercase());
                        if let Some(version) = clean_version(version) {
                            results.push(found_result(
                                package,
                                &version,
                                repository.as_str(),
                                real_name,
                                "github",
                            ));
                        }
                    }
                }
            }
        }
    } else if let Err(error) = universe_res {
        context.log(&format!("R-Universe 检索异常: {error}"));
    }

    if !results.is_empty() && !context.settings.full_search {
        return Ok(results);
    }

    let url = format!(
        "https://api.github.com/search/repositories?q={}+language:R&sort=stars&per_page=10",
        urlencoding::encode(&package.name)
    );
    let request = authorized_get(context.client, &url, context.settings)?;
    if !context.acquire_request_budget() {
        return Ok(results);
    }
    let response = send_request(context, request).await?;
    if response.status() == StatusCode::FORBIDDEN {
        context.log("GitHub API 已触发频率限制（rateLimited）");
        context.github_rate_limited = true;
        return Ok(results);
    }
    if !response.status().is_success() {
        let err_msg = format!("GitHub API 返回 HTTP {}", response.status().as_u16());
        context.log(&err_msg);
        return Err(err_msg);
    }
    let text = read_limited_text(
        response,
        MAX_JSON_RESPONSE_BYTES,
        context.cancelled,
        context.budget,
        context.deadline,
    )
    .await?;
    let body = serde_json::from_str::<GithubSearchResponse>(&text)
        .map_err(|e| format!("GitHub 响应解析失败: {e}"))?;

    for full_name in bounded_github_response_repositories(body) {
        if context.is_stopped() {
            break;
        }
        let repo_name = full_name.rsplit('/').next().unwrap_or_default();
        let lower_repo = repo_name.to_ascii_lowercase();
        let lower_package = package.name.to_ascii_lowercase();
        if !lower_repo.contains(&lower_package) || seen.contains(&full_name.to_ascii_lowercase()) {
            continue;
        }
        if let Some(repository_name) = normalize_github_repository(&full_name) {
            match github_description(context, &repository_name).await {
                Ok(Some(description)) => {
                    if !github_package_name_matches_request(
                        &description.package_name,
                        &package.name,
                    ) {
                        continue;
                    }
                    seen.insert(repository_name.to_ascii_lowercase());
                    results.push(found_result(
                        package,
                        &description.version,
                        &repository_name,
                        &description.package_name,
                        "github",
                    ));
                }
                Ok(None) => {
                    if lower_repo == lower_package {
                        // 兜底逻辑：如果拿不到 DESCRIPTION（比如 mono-repo），但仓库名精确匹配请求包名，则信任该结果
                        seen.insert(repository_name.to_ascii_lowercase());
                        results.push(found_result(
                            package,
                            "unknown",
                            &repository_name,
                            repo_name,
                            "github",
                        ));
                    }
                }
                Err(error) => {
                    context.log(&format!("获取 GitHub DESCRIPTION 异常: {error}"));
                    if lower_repo == lower_package {
                        // 精确仓库名已由 GitHub API 返回并通过仓库名校验；DESCRIPTION
                        // 暂时不可用时仍保留该仓库，避免网络抖动造成真实包被判定为未找到。
                        seen.insert(repository_name.to_ascii_lowercase());
                        results.push(found_result(
                            package,
                            "unknown",
                            &repository_name,
                            repo_name,
                            "github",
                        ));
                    }
                }
            }
        }
    }
    Ok(results)
}

pub(crate) async fn github_description(
    context: &mut SearchContext<'_>,
    repository: &str,
) -> Result<Option<GithubDescription>, String> {
    let mut last_error = None;
    for branch in ["HEAD", "master", "main", "devel"] {
        if context.is_stopped() {
            return Ok(None);
        }
        let url = {
            let parts: Vec<&str> = repository.split('/').collect();
            if parts.len() > 2 {
                let owner = parts[0];
                let repo = parts[1];
                let subdir = parts[2..].join("/");
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{subdir}/DESCRIPTION")
            } else {
                format!("https://raw.githubusercontent.com/{repository}/{branch}/DESCRIPTION")
            }
        };
        let request = match authorized_get(context.client, &url, context.settings) {
            Ok(request) => request,
            Err(error) => {
                context.log(&error);
                return Err(error);
            }
        };
        if !context.acquire_request_budget() {
            return Ok(None);
        }
        match send_request(context, request).await {
            Ok(response) => {
                if response.status() == StatusCode::NOT_FOUND {
                    continue;
                }
                if !response.status().is_success() {
                    let err = format!("HTTP 错误: {}", response.status());
                    context.log(&err);
                    last_error = Some(err);
                    continue;
                }
                let description_text = match read_limited_text(
                    response,
                    MAX_DESCRIPTION_BYTES,
                    context.cancelled,
                    context.budget,
                    context.deadline,
                )
                .await
                {
                    Ok(text) => text,
                    Err(e) => {
                        context.log(&e);
                        last_error = Some(e);
                        continue;
                    }
                };
                if let Some(description) = extract_description_metadata(&description_text) {
                    return Ok(Some(description));
                }
            }
            Err(error) => {
                if !context.is_stopped() {
                    context.log(&format!("GitHub DESCRIPTION 请求失败: {error}"));
                }
                last_error = Some(error);
            }
        }
    }
    if let Some(err) = last_error {
        Err(err)
    } else {
        Ok(None)
    }
}
