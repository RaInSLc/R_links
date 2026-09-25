use crate::{
    logic,
    models::{InputRules, SearchResult},
    storage,
};
use regex::Regex;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
static CACHE_FEEDBACK_LOCK: Mutex<()> = Mutex::new(());
static HISTORY_EXTRACT_RE: OnceLock<Regex> = OnceLock::new();
pub(crate) fn build_offline_results(
    app: &AppHandle,
    input: &str,
    rules: &InputRules,
) -> Vec<SearchResult> {
    let Ok(packages) = logic::parse_inputs_filtered(input, rules) else {
        return Vec::new();
    };
    let cache = storage::load_cache(app).unwrap_or_default();
    let cache_index = crate::search::index_cache(&cache);
    let history = storage::load_history(app).unwrap_or_default();
    packages
        .into_iter()
        .filter_map(|pkg| {
            if let Some(entry) = cache_index
                .get(&pkg.name)
                .into_iter()
                .flatten()
                .copied()
                .filter(|entry| {
                    entry.is_trusted()
                        && (pkg.version.is_empty()
                            || crate::search::version_compatible(&entry.version, &pkg.version))
                })
                .filter(|entry| {
                    if entry.source == "github" {
                        logic::normalize_github_repository(&pkg.name)
                            .is_some_and(|repository| entry.repository == repository)
                    } else {
                        matches!(
                            entry.source.as_str(),
                            "cran" | "bioc" | "biocGit" | "r-forge"
                        ) && entry.package_name == pkg.name
                            && !pkg.name.contains('/')
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
                })
            {
                return Some(SearchResult {
                    package: pkg.name,
                    requested_version: pkg.version,
                    latest_version: entry.version.clone(),
                    repository: entry.repository.clone(),
                    real_name: entry.real_name.clone(),
                    source: entry.source.clone(),
                    found: true,
                    message: "离线缓存命中".to_string(),
                    status: "found".to_string(),
                    stage: "cacheHit".to_string(),
                });
            }
            history
                .iter()
                .find(|r| {
                    r.package_name == pkg.name
                        && (pkg.version.is_empty()
                            || crate::search::version_compatible(&r.version, &pkg.version))
                })
                .map(|record| {
                    let source = match record.tool_name.as_str() {
                        "Bioconductor" => "bioc",
                        "GitHub" => "github",
                        "R-Forge" => "r-forge",
                        _ => "cran",
                    };
                    let repository = if source == "github" {
                        HISTORY_EXTRACT_RE
                            .get_or_init(|| {
                                Regex::new(r#"(?:install_github|install_url)\("([^\"]+)"#)
                                    .expect("固定 install_github/install_url 历史命令正则必须有效")
                            })
                            .captures(&record.command)
                            .and_then(|c| c.get(1))
                            .map(|m| m.as_str())
                            .filter(|v| v.contains('/') && !v.starts_with("http"))
                            .unwrap_or("")
                            .to_string()
                    } else if source == "r-forge" {
                        "http://R-Forge.R-project.org".to_string()
                    } else {
                        String::new()
                    };
                    SearchResult {
                        package: pkg.name,
                        requested_version: pkg.version,
                        latest_version: record.version.clone(),
                        repository,
                        real_name: record.package_name.clone(),
                        source: source.to_string(),
                        found: true,
                        message: "历史记录命中".to_string(),
                        status: "found".to_string(),
                        stage: "final".to_string(),
                    }
                })
        })
        .collect()
}
#[tauri::command]
pub(crate) fn load_cached_results(app: AppHandle, input: String) -> Vec<SearchResult> {
    let rules = storage::load_input_rules(&app);
    build_offline_results(&app, &input, &rules)
}
#[tauri::command]
pub(crate) fn clear_package_cache(app: AppHandle) -> Result<(), String> {
    storage::clear_cache(&app)
}
#[tauri::command]
pub(crate) fn clear_invalidated_cache(app: AppHandle) -> Result<usize, String> {
    storage::clear_invalidated_cache(&app)
}
#[tauri::command]
pub(crate) fn export_package_cache(app: AppHandle) -> Result<String, String> {
    storage::export_cache(&app)
}
#[tauri::command]
pub(crate) fn import_package_cache(app: AppHandle, content: String) -> Result<usize, String> {
    storage::import_cache(&app, &content)
}
#[tauri::command]
pub(crate) fn load_package_cache(
    app: AppHandle,
) -> Result<Vec<crate::models::PackageCacheEntry>, String> {
    let mut entries: Vec<_> = storage::load_cache(&app)?.into_values().collect();
    entries.sort_by(|a, b| {
        b.cached_at.cmp(&a.cached_at).then_with(|| {
            a.package_name
                .to_ascii_lowercase()
                .cmp(&b.package_name.to_ascii_lowercase())
        })
    });
    Ok(entries)
}
#[tauri::command]
pub(crate) fn delete_package_cache_entry(
    app: AppHandle,
    package: String,
    source: String,
    version: String,
    repository: String,
    real_name: String,
) -> Result<(), String> {
    storage::delete_cache_entry(&app, &package, &source, &version, &repository, &real_name)
}
fn matches(
    e: &crate::models::PackageCacheEntry,
    package: &str,
    source: &str,
    version: &str,
    repository: &str,
    real_name: &str,
) -> bool {
    storage::cache_entry_matches(e, package, source, version, repository, real_name)
}
#[tauri::command]
pub(crate) fn rate_cache_result(
    app: AppHandle,
    package: String,
    source: String,
    version: String,
    repository: String,
    real_name: String,
    vote: String,
) -> Result<String, String> {
    let _guard = CACHE_FEEDBACK_LOCK
        .lock()
        .map_err(|_| "缓存反馈锁已损坏".to_string())?;
    let _transaction = storage::lock_cache()?;
    if package.trim().is_empty() || !matches!(vote.as_str(), "up" | "down") {
        return Err("缓存反馈参数无效".to_string());
    }
    let mut cache = storage::load_cache(&app)?;
    let key = storage::package_cache_key(&source, &real_name, &repository);
    let Some(entry) = cache.get_mut(&key) else {
        return Err("没有找到可反馈的缓存记录".to_string());
    };
    if !matches(entry, &package, &source, &version, &repository, &real_name) {
        return Err("当前结果与缓存记录不一致，已跳过反馈".to_string());
    }
    let message = if vote == "up" {
        entry.up_votes = entry.up_votes.saturating_add(1);
        entry.invalidated = entry.down_votes > entry.up_votes;
        if entry.is_trusted() {
            "缓存反馈已记录，当前缓存仍可信"
        } else {
            "缓存反馈已记录，仍需继续验证"
        }
    } else {
        entry.down_votes = entry.down_votes.saturating_add(1);
        entry.invalidated = entry.down_votes > entry.up_votes;
        if entry.invalidated {
            "缓存已标记失效，后续将重新联网验证"
        } else {
            "缓存反馈已记录，当前缓存仍可信"
        }
    };
    storage::save_cache(&app, &cache)?;
    Ok(message.to_string())
}

#[cfg(test)]
mod cache_match_tests {
    use super::matches;
    use crate::models::PackageCacheEntry;

    fn entry(
        package_name: &str,
        source: &str,
        version: &str,
        repository: &str,
        real_name: &str,
    ) -> PackageCacheEntry {
        PackageCacheEntry {
            package_name: package_name.to_string(),
            source: source.to_string(),
            version: version.to_string(),
            repository: repository.to_string(),
            real_name: real_name.to_string(),
            cached_at: "2026-01-01T00:00:00Z".to_string(),
            verified_count: 0,
            up_votes: 0,
            down_votes: 0,
            invalidated: false,
        }
    }

    #[test]
    fn rejects_package_name_mismatch() {
        let item = entry("dplyr", "cran", "1.1.4", "", "dplyr");
        assert!(matches(&item, "dplyr", "cran", "1.1.4", "", "dplyr"));
        // 回归：历史上这里把 entry.package_name 当成期望包名传下去，
        // 调用方传入的包名从未参与比较，任何包名都能通过这一段校验。
        assert!(!matches(&item, "ggplot2", "cran", "1.1.4", "", "dplyr"));
    }

    #[test]
    fn still_compares_source_version_repository_and_real_name() {
        let item = entry("dplyr", "cran", "1.1.4", "", "dplyr");
        assert!(!matches(&item, "dplyr", "bioc", "1.1.4", "", "dplyr"));
        assert!(!matches(&item, "dplyr", "cran", "1.1.5", "", "dplyr"));
        assert!(!matches(
            &item, "dplyr", "cran", "1.1.4", "archive", "dplyr"
        ));
        assert!(!matches(&item, "dplyr", "cran", "1.1.4", "", "other"));
    }
}
