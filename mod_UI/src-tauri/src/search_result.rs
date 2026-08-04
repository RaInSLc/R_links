use super::*;

use crate::search_sanitize::{
    clean_result_package_name, clean_version,
};

pub(crate) fn extract_html_version(html: &str) -> Option<String> {
    let regex = HTML_VERSION_RE.get_or_init(|| {
        Regex::new(r"(?is)<td[^>]*>\s*Version[^<]*</td>\s*<td[^>]*>\s*([^<\s][^<]*)</td>")
            .expect("固定 HTML 版本正则必须有效")
    });
    regex
        .captures(html)
        .and_then(|capture| capture.get(1))
        .and_then(|value| clean_version(value.as_str()))
}

pub(crate) fn extract_description_metadata(description: &str) -> Option<GithubDescription> {
    let mut package_name = None;
    let mut version = None;
    let mut continuation_allowed = false;

    for (line_index, raw_line) in description.lines().enumerate() {
        if line_index >= MAX_DESCRIPTION_LINES || raw_line.len() > MAX_DESCRIPTION_LINE_CHARS {
            return None;
        }
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continuation_allowed = false;
            continue;
        }
        if line.starts_with([' ', '\t']) {
            if !continuation_allowed {
                return None;
            }
            continue;
        }

        let (field, value) = line.split_once(':')?;
        if !is_valid_description_field_name(field) {
            return None;
        }
        let value = value.trim();
        match field {
            "Package" => {
                if package_name.is_some() {
                    return None;
                }
                package_name = Some(clean_result_package_name(value)?);
                continuation_allowed = false;
            }
            "Version" => {
                if version.is_some() {
                    return None;
                }
                version = Some(clean_version(value)?);
                continuation_allowed = false;
            }
            _ => {
                continuation_allowed = true;
            }
        }
    }

    Some(GithubDescription {
        package_name: package_name?,
        version: version?,
    })
}

pub(crate) fn is_valid_description_field_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii()
                && !character.is_ascii_control()
                && !character.is_ascii_whitespace()
                && character != ':'
        })
}

pub(crate) fn github_package_name_matches_request(real_name: &str, requested: &str) -> bool {
    clean_result_package_name(real_name)
        .zip(clean_result_package_name(requested))
        .is_some_and(|(real_name, requested)| real_name.eq_ignore_ascii_case(&requested))
}

pub(crate) fn r_universe_package_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    match value {
        Value::Object(object) if r_universe_object_has_bounded_fields(object) => Some(object),
        Value::Array(values) => values.first().and_then(|item| match item {
            Value::Object(object) if r_universe_object_has_bounded_fields(object) => Some(object),
            _ => None,
        }),
        _ => None,
    }
}

pub(crate) fn r_universe_object_has_bounded_fields(object: &serde_json::Map<String, Value>) -> bool {
    ["Package", "Version", "RemoteUrl"].iter().all(|field| {
        object
            .get(*field)
            .and_then(Value::as_str)
            .is_some_and(|value| {
                value.len() <= MAX_FIELD_CHARS && !value.chars().any(char::is_control)
            })
    })
}

pub(crate) fn clean_github_response_repository_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() > MAX_GITHUB_REPOSITORY_CHARS
        || trimmed.chars().any(|character| character.is_control())
    {
        return None;
    }
    normalize_github_repository(trimmed).map(|_| trimmed.to_string())
}

pub(crate) fn bounded_github_response_repositories(body: GithubSearchResponse) -> Vec<String> {
    body.items
        .into_iter()
        .take(MAX_GITHUB_SEARCH_ITEMS)
        .filter_map(|repository| clean_github_response_repository_name(&repository.full_name))
        .collect()
}
