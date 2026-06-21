use crate::logic::{is_valid_package_name, normalize_github_repository};
use crate::models::SearchResult;

pub(crate) const MAX_RESULT_MESSAGE_CHARS: usize = 256;
pub(crate) const MAX_SEARCH_LOG_CHARS: usize = 512;
pub(crate) const SEARCH_LOG_EMPTY_MESSAGE: &str = "日志内容为空或已被清理";

pub(crate) fn clean_version(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        .then(|| trimmed.to_string())
}

pub(crate) fn clean_result_package_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    is_valid_package_name(trimmed).then(|| trimmed.to_string())
}

pub(crate) fn clean_result_real_name(source: &str, value: &str, fallback: &str) -> Option<String> {
    match clean_result_package_name(value) {
        Some(real_name) => Some(real_name),
        None if source == "github" => None,
        None => Some(fallback.to_string()),
    }
}

pub(crate) fn clean_result_repository(source: &str, value: &str) -> Option<String> {
    let trimmed = value.trim();
    match source {
        "github" => {
            if trimmed.is_empty() {
                Some(String::new())
            } else {
                normalize_github_repository(trimmed)
            }
        }
        "biocGit" => {
            if trimmed.is_empty() || is_valid_bioc_version(trimmed) {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
        _ => trimmed.is_empty().then(String::new),
    }
}

pub(crate) fn clean_result_source(value: &str) -> String {
    match value.trim() {
        "cran" | "bioc" | "biocGit" | "github" | "none" => value.trim().to_string(),
        _ => "none".to_string(),
    }
}

pub(crate) fn sanitize_result_message(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_RESULT_MESSAGE_CHARS)
        .collect()
}

pub(crate) fn sanitize_log_message(value: &str) -> String {
    let message = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_SEARCH_LOG_CHARS)
        .collect::<String>();
    if message.is_empty() {
        SEARCH_LOG_EMPTY_MESSAGE.to_string()
    } else {
        message
    }
}

pub(crate) fn sanitize_search_result_for_emit(mut result: SearchResult) -> SearchResult {
    let fallback_package = clean_result_package_name(&result.package).unwrap_or_default();
    result.package = fallback_package.clone();
    result.requested_version = clean_version(&result.requested_version).unwrap_or_default();
    result.latest_version = clean_version(&result.latest_version).unwrap_or_default();
    result.source = clean_result_source(&result.source);
    result.repository =
        clean_result_repository(&result.source, &result.repository).unwrap_or_default();
    result.real_name = clean_result_package_name(&result.real_name).unwrap_or(fallback_package);
    result.message = sanitize_result_message(&result.message);
    if result.status.is_empty() {
        result.status = if result.found {
            "found".to_string()
        } else {
            "notFound".to_string()
        };
    }
    result.status = sanitize_log_message(&result.status);
    if result.found && !is_trusted_emit_result(&result) {
        result.found = false;
        result.latest_version.clear();
        result.repository.clear();
        result.source = "none".to_string();
        result.message = "结果字段无效，已忽略".to_string();
    }
    result
}

fn is_trusted_emit_result(result: &SearchResult) -> bool {
    if result.package.is_empty() || result.real_name.is_empty() || result.latest_version.is_empty()
    {
        return false;
    }

    match result.source.as_str() {
        "cran" | "bioc" => result.repository.is_empty(),
        "biocGit" => !result.repository.is_empty(),
        "github" => github_emit_identity_matches(result),
        _ => false,
    }
}

fn github_emit_identity_matches(result: &SearchResult) -> bool {
    if result.repository.is_empty() {
        return false;
    }
    if normalize_github_repository(&result.package)
        .as_deref()
        .is_some_and(|repository| repository.eq_ignore_ascii_case(&result.repository))
    {
        return true;
    }
    result.real_name.eq_ignore_ascii_case(&result.package)
        || result
            .repository
            .rsplit('/')
            .next()
            .is_some_and(|repo| repo.eq_ignore_ascii_case(&result.real_name))
}

pub(crate) fn is_valid_bioc_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

#[cfg(test)]
fn append_bounded_search_result(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_search_result_for_emit_strips_control_chars_from_message() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "dplyr".to_string(),
            requested_version: String::new(),
            latest_version: "1.1.0".to_string(),
            repository: String::new(),
            real_name: "dplyr".to_string(),
            source: "cran".to_string(),
            found: true,
            message: format!("ok\n{}", "x".repeat(MAX_RESULT_MESSAGE_CHARS + 20)),
            status: "found".to_string(),
        });
        assert!(!result.message.contains('\n'));
        assert!(result.message.len() <= MAX_RESULT_MESSAGE_CHARS);
    }

    #[test]
    fn sanitize_search_result_for_emit_marks_invalid_cran_as_not_found() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "dplyr".to_string(),
            requested_version: String::new(),
            latest_version: String::new(),
            repository: String::new(),
            real_name: "dplyr".to_string(),
            source: "cran".to_string(),
            found: true,
            message: "ok".to_string(),
            status: "found".to_string(),
        });
        assert!(!result.found);
        assert_eq!(result.source, "none");
    }

    #[test]
    fn sanitize_search_result_for_emit_rejects_untrusted_github_identity() {
        let result = sanitize_search_result_for_emit(SearchResult {
            package: "not-a-match".to_string(),
            requested_version: String::new(),
            latest_version: "1.0.0".to_string(),
            repository: "owner/repo".to_string(),
            real_name: "something-else".to_string(),
            source: "github".to_string(),
            found: true,
            message: "ok".to_string(),
            status: "found".to_string(),
        });
        assert!(!result.found);
    }

    #[test]
    fn append_bounded_search_result_respects_limit() {
        let mut results = Vec::new();
        for _ in 0..3 {
            assert!(append_bounded_search_result(
                &mut results,
                SearchResult::default(),
                3
            ));
        }
        assert!(!append_bounded_search_result(
            &mut results,
            SearchResult::default(),
            3
        ));
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn append_bounded_search_result_returns_false_at_limit() {
        let mut results = vec![
            SearchResult::default(),
            SearchResult::default(),
            SearchResult::default(),
        ];
        assert!(!append_bounded_search_result(
            &mut results,
            SearchResult::default(),
            3
        ));
    }

    #[test]
    fn sanitize_log_message_handles_empty() {
        assert_eq!(sanitize_log_message(""), SEARCH_LOG_EMPTY_MESSAGE);
        assert_eq!(sanitize_log_message("   "), SEARCH_LOG_EMPTY_MESSAGE);
    }

    #[test]
    fn sanitize_log_message_strips_control_chars() {
        let result = sanitize_log_message("hello\tworld\n");
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn is_valid_bioc_version_accepts_numeric_dots() {
        assert!(is_valid_bioc_version("3.18"));
        assert!(is_valid_bioc_version("3.99"));
        assert!(!is_valid_bioc_version("3"));
        assert!(!is_valid_bioc_version("3.a"));
        assert!(!is_valid_bioc_version(""));
    }

    #[test]
    fn clean_version_rejects_empty_and_overlong() {
        assert!(clean_version("").is_none());
        assert!(clean_version("   ").is_none());
        assert!(clean_version(&"a".repeat(65)).is_none());
        assert!(clean_version("1.2-3_test").is_some());
        assert!(clean_version("invalid!").is_none());
    }

    #[test]
    fn clean_result_source_normalizes() {
        assert_eq!(clean_result_source("cran"), "cran");
        assert_eq!(clean_result_source(" CRAN "), "none");
        assert_eq!(clean_result_source("unknown"), "none");
        assert_eq!(clean_result_source("biocGit"), "biocGit");
    }

    #[test]
    fn clean_result_repository_validates_github() {
        assert!(clean_result_repository("github", "owner/repo").is_some());
        assert!(clean_result_repository("github", "invalid").is_none());
        assert_eq!(clean_result_repository("github", "").unwrap(), "");
    }
}
