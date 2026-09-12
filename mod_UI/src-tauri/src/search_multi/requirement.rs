use super::CondaResponse;

pub(super) fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub(super) fn split_requirement(line: &str) -> (String, String) {
    let line = line.trim().trim_matches(['"', '\'']);
    if let Some(index) = line.find(['=', '>', '<', '!', '~']) {
        return (
            line[..index].trim().to_string(),
            line[index..].split_whitespace().collect(),
        );
    }
    (line.to_string(), String::new())
}

pub(super) fn inputs(input: &str) -> Vec<(String, String)> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
        .map(split_requirement)
        .filter(|(name, _)| valid_name(name))
        .collect()
}

pub(super) fn conda_version(payload: &CondaResponse, requested: &str) -> Option<String> {
    let mut versions = payload.versions.clone().unwrap_or_default();
    if let Some(latest) = payload
        .latest_version
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        versions.push(latest.to_string());
    }
    versions.sort_by(|left, right| {
        let left_parts = left.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
        let right_parts = right
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0));
        left_parts.cmp(right_parts)
    });
    versions.dedup();
    versions.reverse();

    versions
        .into_iter()
        .find(|version| requirement_matches(version, requested))
}

pub(super) fn requirement_matches(version: &str, requested: &str) -> bool {
    if requested.is_empty() {
        return true;
    }
    let numeric = |value: &str| {
        value
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
    };
    let Ok(actual) = numeric(version) else {
        return false;
    };
    requested.split(',').all(|constraint| {
        let constraint = constraint.trim();
        let operator = ["==", "!=", ">=", "<=", "~=", ">", "<", "="]
            .into_iter()
            .find(|operator| constraint.starts_with(operator))
            .unwrap_or("=");
        let target = constraint.strip_prefix(operator).unwrap_or(constraint);
        let Ok(mut expected) = numeric(target) else {
            return false;
        };
        let original = expected.clone();
        let mut actual = actual.clone();
        let length = actual.len().max(expected.len());
        actual.resize(length, 0);
        expected.resize(length, 0);
        match operator {
            "==" => actual == expected,
            "!=" => actual != expected,
            ">=" => actual >= expected,
            "<=" => actual <= expected,
            ">" => actual > expected,
            "<" => actual < expected,
            "~=" => {
                actual >= expected
                    && original.len() >= 2
                    && actual[..original.len() - 1] == original[..original.len() - 1]
            }
            _ => actual.starts_with(&original),
        }
    })
}
