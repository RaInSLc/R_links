use super::dependency_parse::parse_description;
use crate::logic::{is_valid_package_name, normalize_github_repository};
use crate::search_urls::validate_search_request_url_with_mirror;

const BIOC_DEPENDENCY_CATEGORIES: &[&str] =
    &["bioc", "data/annotation", "data/experiment", "workflows"];

/// 构造依赖元数据候选 URL。与检索路径使用同一套 URL 校验，避免字符串拼接绕过防御。
pub(crate) fn build_dependency_urls(
    package: &str,
    source: &str,
    repository: &str,
    mirror: &str,
) -> Vec<(String, bool)> {
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
                urls.push((
                    format!("https://bioconductor.org/packages/{bioc_version}/{category}/src/contrib/PACKAGES"),
                    true,
                ));
            }
        }
    } else if source.eq_ignore_ascii_case("github") {
        let candidate = if !repository.trim().is_empty() {
            repository.trim()
        } else if package.contains('/') {
            package
        } else {
            ""
        };
        if let Some(github_repo) = normalize_github_repository(candidate) {
            urls.push((
                format!("https://raw.githubusercontent.com/{github_repo}/master/DESCRIPTION"),
                false,
            ));
            urls.push((
                format!("https://raw.githubusercontent.com/{github_repo}/main/DESCRIPTION"),
                false,
            ));
        }
        if is_valid_package_name(package) {
            urls.push((
                format!("https://raw.githubusercontent.com/cran/{package}/master/DESCRIPTION"),
                false,
            ));
        }
    } else {
        urls.push((
            format!("{}/web/packages/{}/DESCRIPTION", mirror_clean, package),
            false,
        ));
    }

    urls.retain(|(url, _)| validate_search_request_url_with_mirror(url, Some(mirror)).is_ok());
    urls
}

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
    let urls = build_dependency_urls(package, source, repository, mirror);

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
        let Some(entry_package) = meta.get("package") else {
            continue;
        };
        if !entry_package.eq_ignore_ascii_case(package) {
            continue;
        }
        if !version.is_empty()
            && meta
                .get("version")
                .is_some_and(|entry_version| entry_version != version)
        {
            continue;
        }
        return Some(entry.to_string());
    }
    None
}
