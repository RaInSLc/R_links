use super::dependency_parse::parse_description;

const BIOC_DEPENDENCY_CATEGORIES: &[&str] =
    &["bioc", "data/annotation", "data/experiment", "workflows"];

/// 发送请求获取包的 DESCRIPTION 文本
#[allow(clippy::too_many_arguments)]
pub(crate) async fn fetch_description(
    client: &reqwest::Client,
    package: &str,
    source: &str,
    version: &str,
    repository: &str,
    mirror: &str,
    cancelled: &std::sync::atomic::AtomicBool,
    budget: &crate::search::RequestBudget,
    deadline: std::time::Instant,
) -> Result<String, String> {
    let mut urls: Vec<(String, bool)> = Vec::new();
    let mirror_clean = mirror.trim_end_matches('/');

    if source.eq_ignore_ascii_case("cran") || source.eq_ignore_ascii_case("none") {
        urls.push((
            format!("{}/web/packages/{}/DESCRIPTION", mirror_clean, package),
            false,
        ));
    } else if source.eq_ignore_ascii_case("bioc") || source.eq_ignore_ascii_case("biocGit") {
        let bioc_versions =
            if source.eq_ignore_ascii_case("biocGit") && !repository.trim().is_empty() {
                vec![
                    repository.trim().trim_matches('/').to_string(),
                    "release".to_string(),
                ]
            } else {
                vec!["release".to_string()]
            };
        for bioc_version in bioc_versions {
            for category in BIOC_DEPENDENCY_CATEGORIES {
                urls.push((format!("https://bioconductor.org/packages/{bioc_version}/{category}/src/contrib/PACKAGES"), true));
            }
        }
    } else if source.eq_ignore_ascii_case("github") {
        let github_repo = if !repository.trim().is_empty() {
            repository.trim()
        } else if package.contains('/') {
            package
        } else {
            ""
        };
        if !github_repo.is_empty() {
            urls.push((
                format!(
                    "https://raw.githubusercontent.com/{}/master/DESCRIPTION",
                    github_repo
                ),
                false,
            ));
            urls.push((
                format!(
                    "https://raw.githubusercontent.com/{}/main/DESCRIPTION",
                    github_repo
                ),
                false,
            ));
        }
        urls.push((
            format!(
                "https://raw.githubusercontent.com/cran/{}/master/DESCRIPTION",
                package
            ),
            false,
        ));
    } else {
        urls.push((
            format!("{}/web/packages/{}/DESCRIPTION", mirror_clean, package),
            false,
        ));
    }

    for (url, is_packages_index) in urls {
        budget.try_acquire()?;
        match crate::search::await_or_stop(client.get(&url).send(), cancelled, budget, deadline)
            .await?
        {
            Ok(mut resp) if resp.status().is_success() => {
                const MAX_DESCRIPTION_BYTES: usize = 8 * 1024 * 1024;
                if resp
                    .content_length()
                    .is_some_and(|length| length > MAX_DESCRIPTION_BYTES as u64)
                {
                    continue;
                }
                let mut body = Vec::new();
                while let Some(chunk) =
                    crate::search::await_or_stop(resp.chunk(), cancelled, budget, deadline)
                        .await?
                        .map_err(|error| error.to_string())?
                {
                    if body.len().saturating_add(chunk.len()) > MAX_DESCRIPTION_BYTES {
                        return Err("依赖元数据超过读取限制".to_string());
                    }
                    body.extend_from_slice(&chunk);
                }
                if let Ok(text) = String::from_utf8(body) {
                    if is_packages_index {
                        if let Some(entry) = extract_packages_index_entry(&text, package, version) {
                            return Ok(entry);
                        }
                    } else if !text.trim().is_empty() {
                        return Ok(text);
                    }
                }
            }
            _ => {}
        }
    }
    Err(format!("无法获取包 {} 的 DESCRIPTION 元数据", package))
}

pub(crate) fn extract_packages_index_entry(
    text: &str,
    package: &str,
    version: &str,
) -> Option<String> {
    for entry in text.split("\n\n") {
        let meta = parse_description(entry);
        let Some(entry_package) = meta.get("Package") else {
            continue;
        };
        if !entry_package.eq_ignore_ascii_case(package) {
            continue;
        }
        if !version.is_empty()
            && meta
                .get("Version")
                .is_some_and(|entry_version| entry_version != version)
        {
            continue;
        }
        return Some(entry.to_string());
    }
    None
}
