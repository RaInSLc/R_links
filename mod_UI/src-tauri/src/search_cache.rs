use super::*;
use super::log;

pub(crate) fn cache_entry_matches_result(entry: &PackageCacheEntry, result: &SearchResult) -> bool {
    entry.source == result.source
        && entry.version == result.latest_version
        && entry.repository == result.repository
        && entry.real_name.eq_ignore_ascii_case(&result.real_name)
}

pub fn cache_entry_from_result(
    result: &SearchResult,
    existing: Option<&PackageCacheEntry>,
    cached_at: String,
) -> PackageCacheEntry {
    let mut verified_count = 1;
    let mut up_votes = 0;
    let mut down_votes = 0;
    let mut invalidated = false;

    if let Some(entry) = existing {
        if cache_entry_matches_result(entry, result) {
            verified_count = entry.verified_count.saturating_add(1).max(1);
            up_votes = entry.up_votes;
            down_votes = entry.down_votes;
            invalidated = entry.invalidated && entry.down_votes > entry.up_votes;
        }
    }

    PackageCacheEntry {
        package_name: result.real_name.clone(),
        source: result.source.clone(),
        version: result.latest_version.clone(),
        repository: result.repository.clone(),
        real_name: result.real_name.clone(),
        cached_at,
        verified_count,
        up_votes,
        down_votes,
        invalidated,
    }
}

pub fn build_client(settings: &Settings) -> Result<Client, String> {
    let mut builder = Client::builder()
        .user_agent("RLinkModUI/0.1")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());
    if !settings.proxy.trim().is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(settings.proxy.trim())
                .map_err(|_| "网络代理配置无效".to_string())?,
        );
    }
    builder.build().map_err(|error| error.to_string())
}
pub(crate) fn version_compatible(found: &str, requested: &str) -> bool {
    found == requested
        || (requested.matches('.').count() == 1
            && found
                .strip_prefix(requested)
                .is_some_and(|suffix| suffix.starts_with('.')))
}

pub(crate) fn found_result(
    package: &PackageInput,
    version: &str,
    repository: &str,
    real_name: &str,
    source: &str,
) -> SearchResult {
    let package_name =
        clean_result_package_name(&package.name).unwrap_or_else(|| package.name.clone());
    let latest_version = clean_version(version).unwrap_or_default();
    let source = clean_result_source(source);
    let repository = clean_result_repository(&source, repository).unwrap_or_default();
    let Some(real_name) = clean_result_real_name(&source, real_name, &package_name) else {
        return SearchResult {
            package: package_name.clone(),
            requested_version: package.version.clone(),
            latest_version: String::new(),
            repository: String::new(),
            real_name: package_name,
            source: "none".to_string(),
            found: false,
            message: "结果真实包名无效，已忽略".to_string(),
            status: "notFound".to_string(),
            stage: "final".to_string(),
        };
    };
    SearchResult {
        package: package_name,
        requested_version: package.version.clone(),
        latest_version,
        repository,
        real_name,
        source,
        found: true,
        message: "验证成功".to_string(),
        status: "found".to_string(),
        stage: "final".to_string(),
    }
}

#[cfg(test)]
pub(crate) fn append_bounded_search_result(
    results: &mut Vec<SearchResult>,
    result: SearchResult,
    limit: usize,
) -> bool {
    if results.len() >= limit {
        return false;
    }
    results.push(result);
    true
}
