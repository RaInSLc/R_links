use super::*;

pub(crate) async fn search_bioconductor(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
) -> Result<Vec<SearchResult>, String> {
    for category in BIOC_CATEGORIES {
        if context.should_stop() {
            return Ok(Vec::new());
        }
        let release_url = format!(
            "https://bioconductor.org/packages/release/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        match get_text(context, &release_url).await {
            Ok(Some(html)) => {
                if let Some(release_version) = extract_html_version(&html) {
                    if !package.version.is_empty()
                        && !version_compatible(&release_version, &package.version)
                    {
                        if let Some(history) = find_bioc_history(context, package, category).await?
                        {
                            return Ok(vec![history]);
                        }
                    }
                    context.log(&format!("Bioconductor Release 命中版本 {release_version}"));
                    return Ok(vec![found_result(
                        package,
                        &release_version,
                        "",
                        &package.name,
                        "bioc",
                    )]);
                }
            }
            Ok(None) => {}
            Err(error) => {
                return Err(error);
            }
        }
    }
    Ok(Vec::new())
}

pub(crate) async fn find_bioc_history(
    context: &mut SearchContext<'_>,
    package: &PackageInput,
    category: &str,
) -> Result<Option<SearchResult>, String> {
    // 用拥有的字符串保存候选版本，便于把推断出的版本直接插到队首。
    let mut versions = BIOC_VERSIONS
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<String>>();
    let parts = package
        .version
        .split('.')
        .filter_map(|value| value.parse::<i32>().ok())
        .collect::<Vec<_>>();
    if parts.len() >= 2 {
        if let Some(inferred) = infer_bioc_version(parts[0], parts[1]) {
            let inferred = format!("3.{inferred}");
            if let Some(position) = versions.iter().position(|value| value == &inferred) {
                versions.remove(position);
                versions.insert(0, inferred);
            }
        }
    }

    for bioc_version in versions {
        if context.should_stop() {
            return Ok(None);
        }
        let url = format!(
            "https://bioconductor.org/packages/{bioc_version}/{category}/html/{}.html",
            urlencoding::encode(&package.name)
        );
        match get_text(context, &url).await {
            Ok(Some(html)) => {
                if let Some(version) = extract_html_version(&html) {
                    if version_compatible(&version, &package.version) {
                        context.log(&format!("Bioconductor {bioc_version} 匹配版本 {version}"));
                        return Ok(Some(found_result(
                            package,
                            &version,
                            &bioc_version,
                            &package.name,
                            "biocGit",
                        )));
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                // 单个 Bioc 版本请求失败不应中断整个历史版本遍历，否则包会被误判为未找到。
                context.log(&format!(
                    "Bioconductor {bioc_version} 请求失败，继续尝试其它版本: {error}"
                ));
            }
        }
    }
    Ok(None)
}
