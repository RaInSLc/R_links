use super::*;

pub(crate) fn parse_version(v: &str) -> Vec<i32> {
    v.split(['.', '-', '_'])
        .filter_map(|s| s.parse::<i32>().ok())
        .collect()
}

pub(crate) fn compare_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
    parse_version(v1).cmp(&parse_version(v2))
}

pub(crate) fn extract_archive_versions(html: &str, package_name: &str) -> Vec<String> {
    let pattern = format!(
        r#"{}_([0-9A-Za-z.-]+)\.tar\.gz"#,
        regex::escape(package_name)
    );
    let Ok(re) = Regex::new(&pattern) else {
        return Vec::new();
    };
    let mut versions = Vec::new();
    for cap in re.captures_iter(html) {
        if let Some(version) = cap.get(1) {
            versions.push(version.as_str().to_string());
        }
    }
    versions
}

pub(crate) fn cran_archive_tarball_url(package_name: &str, version: &str) -> String {
    format!(
        "https://cran.r-project.org/src/contrib/Archive/{}/{}_{}.tar.gz",
        urlencoding::encode(package_name),
        urlencoding::encode(package_name),
        urlencoding::encode(version)
    )
}

pub(crate) async fn search_cran(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Option<SearchResult>, String> {
    let url = format!(
        "{}/web/packages/{}/index.html",
        context.settings.cran_mirror.trim_end_matches('/'),
        urlencoding::encode(&package.name)
    );
    let html = get_text(context, &url).await?;
    let version = html.as_deref().and_then(extract_html_version);

    if let Some(version) = version {
        context.log(&format!("CRAN 命中版本 {version}"));
        return Ok(Some(found_result(
            package,
            &version,
            "",
            &package.name,
            "cran",
        )));
    }

    // 如果主页请求失败（404），或者主页中无法提取出版本号（例如包已被移出 CRAN 官方主页并归档）
    // 尝试从 CRAN Archive 归档区寻找包的历史版本
    let archive_url = format!(
        "{}/src/contrib/Archive/{}/",
        context.settings.cran_mirror.trim_end_matches('/'),
        urlencoding::encode(&package.name)
    );
    context.log(&format!(
        "CRAN 主页未找到包 {} 的有效版本，尝试检索 Archive 归档...",
        package.name
    ));
    match get_text(context, &archive_url).await? {
        Some(archive_html) => {
            let versions = extract_archive_versions(&archive_html, &package.name);
            if versions.is_empty() {
                return Ok(None);
            }
            let mut latest_version = versions[0].clone();
            for v in &versions {
                if compare_versions(v, &latest_version) == std::cmp::Ordering::Greater {
                    latest_version = v.clone();
                }
            }

            let target_version = if !package.version.is_empty() {
                // 单次遍历完成判定与取值；不使用 unwrap，避免依赖
                // 「判定谓词与查找谓词完全一致」这一隐含前提。
                match versions
                    .iter()
                    .find(|v| version_compatible(v, &package.version))
                {
                    Some(matched) => matched.clone(),
                    None => return Ok(None),
                }
            } else {
                latest_version
            };

            context.log(&format!("CRAN Archive 命中归档版本 {target_version}"));
            let archive_url = cran_archive_tarball_url(&package.name, &target_version);
            Ok(Some(found_result(
                package,
                &target_version,
                &archive_url,
                &package.name,
                "cran",
            )))
        }
        None => Ok(None),
    }
}

pub(crate) async fn search_r_forge(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Option<SearchResult>, String> {
    let text = get_text(context, R_FORGE_PACKAGES_URL).await?;
    if let Some(content) = text {
        if let Some(version) = find_package_in_r_forge_dcf(&content, &package.name) {
            context.log(&format!("R-Forge 命中版本 {version}"));
            return Ok(Some(found_result(
                package,
                &version,
                R_FORGE_REPOS_URL,
                &package.name,
                "r-forge",
            )));
        }
    }
    Ok(None)
}

pub(crate) fn find_package_in_r_forge_dcf(text: &str, package_name: &str) -> Option<String> {
    let target = format!("Package: {package_name}");
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().eq_ignore_ascii_case(&target) {
            for subsequent in lines.get(i + 1..).unwrap_or(&[]) {
                let trimmed = subsequent.trim();
                if trimmed.is_empty() {
                    break;
                }
                if let Some(version) = trimmed.strip_prefix("Version:") {
                    let version = version.trim();
                    if !version.is_empty() {
                        return Some(version.to_string());
                    }
                }
            }
        }
    }
    None
}
