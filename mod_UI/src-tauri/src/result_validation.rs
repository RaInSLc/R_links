use crate::models::{SearchResult, MAX_FIELD_CHARS};
const MAX_GENERATE_SEARCH_RESULTS: usize = 8_000;
const MAX_VERSION_CHARS: usize = 64;
const MAX_RESULT_SOURCE_CHARS: usize = 16;
const MAX_RESULT_MESSAGE_CHARS: usize = 512;
use crate::logic::*;

pub(crate) fn validate_search_results_count(results: &[SearchResult]) -> Result<(), String> {
    if results.len() > MAX_GENERATE_SEARCH_RESULTS {
        return Err(format!(
            "检索结果数量过多，最多允许 {MAX_GENERATE_SEARCH_RESULTS} 条"
        ));
    }
    Ok(())
}

pub(crate) fn choose_best_result<'a>(
    package: &str,
    results: &'a [SearchResult],
    source_hint: Option<&str>,
) -> Option<&'a SearchResult> {
    let mut candidates = results
        .iter()
        .filter(|result| result.found && result.package.eq_ignore_ascii_case(package))
        .filter(|result| result_identity_matches_package(result, package))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|result| {
        let hint_match = if let Some(hint) = source_hint {
            if result.source.eq_ignore_ascii_case(hint)
                || (hint == "bioc" && result.source == "biocGit")
            {
                0
            } else {
                1
            }
        } else {
            1
        };
        let strict_name = if result.real_name.eq_ignore_ascii_case(package) {
            0
        } else {
            1
        };
        let exact_repo = result_repository_name_matches_package(result, package);
        let source = match result.source.as_str() {
            "biocGit" => 0,
            "cran" => 1,
            "bioc" => 2,
            "github" => 3,
            "r-forge" => 4,
            _ => 5,
        };
        (
            hint_match,
            strict_name,
            source,
            if exact_repo { 0 } else { 1 },
        )
    });
    candidates.into_iter().next()
}

pub(crate) struct ArchiveGithubDecision<'a> {
    pub(crate) archive: &'a SearchResult,
    pub(crate) github: &'a SearchResult,
    pub(crate) archive_major: usize,
    pub(crate) github_major: usize,
    pub(crate) major_gap: usize,
    pub(crate) use_github: bool,
}

impl ArchiveGithubDecision<'_> {
    pub(crate) fn comment(&self) -> String {
        let diff = self.github_major.saturating_sub(self.archive_major);
        if self.use_github {
            format!(
                "# [Archive/GitHub 决策: GitHub v{} 主版本比 Archive v{} 高 {diff}，达到阈值 {}，使用 GitHub]",
                self.github.latest_version, self.archive.latest_version, self.major_gap
            )
        } else {
            format!(
                "# [Archive/GitHub 决策: Archive v{}，GitHub v{}，主版本差 {diff} 未达到阈值 {}，保留 Archive]",
                self.archive.latest_version, self.github.latest_version, self.major_gap
            )
        }
    }
}

pub(crate) fn archive_github_decision<'a>(
    package: &str,
    archive: &'a SearchResult,
    results: &'a [SearchResult],
    major_gap: usize,
) -> Option<ArchiveGithubDecision<'a>> {
    if archive.source != "cran" || !is_cran_archive_result(archive) {
        return None;
    }
    let archive_major = major_version(&archive.latest_version)?;
    results
        .iter()
        .filter(|result| result.found && result.source == "github")
        .filter(|result| result.package.eq_ignore_ascii_case(package))
        .filter(|result| result_identity_matches_package(result, package))
        .filter_map(|result| {
            let github_major = major_version(&result.latest_version)?;
            Some((github_major, result))
        })
        .max_by_key(|(github_major, _)| *github_major)
        .map(|(github_major, github)| ArchiveGithubDecision {
            archive,
            github,
            archive_major,
            github_major,
            major_gap,
            use_github: github_major >= archive_major.saturating_add(major_gap),
        })
}

pub(crate) fn major_version(version: &str) -> Option<usize> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .find(|segment| !segment.is_empty())
        .and_then(|segment| segment.parse::<usize>().ok())
}

pub(crate) fn is_cran_archive_result(result: &SearchResult) -> bool {
    result.repository == "archive"
        || result
            .repository
            .starts_with("https://cran.r-project.org/src/contrib/Archive/")
}

pub(crate) fn result_identity_matches_package(result: &SearchResult, package: &str) -> bool {
    result.real_name.eq_ignore_ascii_case(package)
        || result_repository_name_matches_package(result, package)
}

pub(crate) fn result_repository_name_matches_package(result: &SearchResult, package: &str) -> bool {
    result
        .repository
        .rsplit('/')
        .next()
        .is_some_and(|repo| repo.eq_ignore_ascii_case(package))
}

pub(crate) fn sanitize_search_results(results: &[SearchResult]) -> Vec<SearchResult> {
    results
        .iter()
        .filter_map(sanitize_search_result)
        .take(MAX_GENERATE_SEARCH_RESULTS)
        .collect()
}

pub(crate) fn sanitize_search_result(result: &SearchResult) -> Option<SearchResult> {
    if !search_result_fields_within_bounds(result) {
        return None;
    }
    let package = clean_result_package(&result.package)?;
    let requested_version = clean_result_version(&result.requested_version).unwrap_or_default();
    let latest_version = clean_result_version(&result.latest_version).unwrap_or_default();
    let source = clean_result_source(&result.source)?;
    let repository = clean_result_repository(&source, &result.repository)?;
    let clean_real_name = clean_result_package(&result.real_name);
    let real_name_is_valid = clean_real_name.is_some();
    let real_name = clean_real_name.unwrap_or_else(|| package.clone());
    let message = clean_result_text(&result.message);

    if result.found
        && !is_trusted_found_result(&source, &latest_version, &repository, real_name_is_valid)
    {
        return None;
    }

    Some(SearchResult {
        package,
        requested_version,
        latest_version,
        repository,
        real_name,
        source,
        found: result.found,
        message,
        status: if result.found {
            "found".to_string()
        } else {
            "notFound".to_string()
        },
        stage: if result.stage.is_empty() {
            "final".to_string()
        } else {
            result.stage.clone()
        },
    })
}

pub(crate) fn search_result_fields_within_bounds(result: &SearchResult) -> bool {
    result.package.len() <= MAX_FIELD_CHARS
        && result.requested_version.len() <= MAX_VERSION_CHARS
        && result.latest_version.len() <= MAX_VERSION_CHARS
        && result.repository.len() <= MAX_FIELD_CHARS
        && result.real_name.len() <= MAX_FIELD_CHARS
        && result.source.len() <= MAX_RESULT_SOURCE_CHARS
        && result.message.len() <= MAX_RESULT_MESSAGE_CHARS
}

pub(crate) fn is_trusted_found_result(
    source: &str,
    latest_version: &str,
    repository: &str,
    real_name_is_valid: bool,
) -> bool {
    if latest_version.is_empty() {
        return false;
    }

    match source {
        "cran" | "bioc" => true,
        "biocGit" => !repository.is_empty(),
        "github" => !repository.is_empty() && real_name_is_valid,
        "r-forge" => repository == "http://R-Forge.R-project.org",
        _ => false,
    }
}

pub(crate) fn clean_result_package(value: &str) -> Option<String> {
    let trimmed = value.trim();
    is_valid_package_name(trimmed).then(|| trimmed.to_string())
}

pub(crate) fn clean_result_version(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }
    is_clean_version(trimmed).then(|| trimmed.to_string())
}

pub(crate) fn clean_result_source(value: &str) -> Option<String> {
    match value.trim() {
        "cran" | "cran-binary" | "bioc" | "biocGit" | "github" | "r-forge" | "none" => {
            Some(value.trim().to_string())
        }
        _ => None,
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
        "r-forge" => {
            if trimmed.is_empty() || trimmed == "http://R-Forge.R-project.org" {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
        "cran" => {
            if trimmed.is_empty()
                || trimmed == "archive"
                || (trimmed.starts_with("https://cran.r-project.org/src/contrib/Archive/")
                    && trimmed.ends_with(".tar.gz"))
            {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
        "cran-binary" => {
            if trimmed.starts_with("https://packagemanager.posit.co/") && trimmed.ends_with('/') {
                Some(trimmed.to_string())
            } else { None }
        }
        _ => {
            if trimmed.len() <= MAX_FIELD_CHARS && !trimmed.chars().any(char::is_control) {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
    }
}

pub(crate) fn clean_result_text(value: &str) -> String {
    truncate_utf8_bytes(
        &value
            .trim()
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>(),
        MAX_RESULT_MESSAGE_CHARS,
    )
}

pub(crate) fn truncate_utf8_bytes(value: &str, limit: usize) -> String {
    let mut bytes = 0usize;
    let mut output = String::new();
    for character in value.chars() {
        let next_bytes = character.len_utf8();
        if bytes + next_bytes > limit {
            break;
        }
        bytes += next_bytes;
        output.push(character);
    }
    output
}
