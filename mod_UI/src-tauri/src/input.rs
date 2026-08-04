use crate::models::{InputRules, PackageInput, MAX_PACKAGE_LINES};
use regex::Regex;
use std::sync::OnceLock;

static INPUT_URL_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_PACKAGE_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_VERSION_RE: OnceLock<Regex> = OnceLock::new();
static QUOTED_VALUE_RE: OnceLock<Regex> = OnceLock::new();
static SOURCE_HINT_RE: OnceLock<Regex> = OnceLock::new();
static LOCAL_ARCHIVE_RE: OnceLock<Regex> = OnceLock::new();

use crate::logic::*;

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn parse_inputs(input: &str) -> Result<Vec<PackageInput>, String> {
    parse_inputs_filtered(input, &InputRules::default())
}

pub fn parse_inputs_filtered(input: &str, rules: &InputRules) -> Result<Vec<PackageInput>, String> {
    validate_input_size(input)?;

    let mut packages = Vec::new();
    let exclude_regexes: Vec<regex::Regex> = rules
        .exclude_regex
        .iter()
        .filter_map(|pattern| regex::Regex::new(pattern).ok())
        .collect();

    for (line_idx, line) in input.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_comment_line(trimmed, rules) {
            continue;
        }
        if let Some(managed) = normalize_managed_package_line(trimmed) {
            for item in managed
                .lines()
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                let pkg = parse_input_line(item)
                    .ok_or_else(|| format!("第 {line_idx} 行包管理器输入格式无效"))?;
                packages.push(pkg);
                if packages.len() > MAX_PACKAGE_LINES {
                    return Err(format!("单次最多处理 {MAX_PACKAGE_LINES} 行输入"));
                }
            }
            continue;
        }
        if let Some(markdown_line) = normalize_markdown_table_line(trimmed) {
            if markdown_line.is_empty() {
                continue;
            }
            if let Some(pkg) = parse_input_line(&markdown_line) {
                packages.push(pkg);
                if packages.len() > MAX_PACKAGE_LINES {
                    return Err(format!("单次最多处理 {MAX_PACKAGE_LINES} 行输入"));
                }
                continue;
            }
            return Err(format!("第 {} 行 Markdown 表格输入格式无效", line_idx + 1));
        }
        if exclude_regexes.iter().any(|re| re.is_match(trimmed)) {
            continue;
        }
        let line_num = line_idx + 1;

        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            if let Some(pkg) = parse_input_line(trimmed) {
                packages.push(pkg);
            } else {
                return Err(format!("第 {line_num} 行 URL 输入格式无效"));
            }
            if packages.len() > MAX_PACKAGE_LINES {
                return Err(format!("单次最多处理 {MAX_PACKAGE_LINES} 行输入"));
            }
            continue;
        }

        let preprocessed = if rules.strip_c_parens {
            strip_r_parens_wrapper(trimmed)
        } else {
            trimmed.to_string()
        };

        let segments = split_by_separators(&preprocessed, rules);
        for (seg_idx, segment) in segments.iter().enumerate() {
            let cleaned = if rules.strip_quotes {
                segment.trim_matches(['"', '\'']).trim().to_string()
            } else {
                segment.trim().to_string()
            };
            if cleaned.is_empty() {
                continue;
            }
            if exclude_regexes.iter().any(|re| re.is_match(&cleaned)) {
                continue;
            }
            let pkg_opt = parse_input_line(&cleaned);
            if let Some(ref pkg) = pkg_opt {
                let pkg_name_lower = pkg.name.to_ascii_lowercase();
                let is_builtin_blacklisted = matches!(
                    pkg_name_lower.as_str(),
                    "if" | "else"
                        | "for"
                        | "while"
                        | "function"
                        | "in"
                        | "repeat"
                        | "next"
                        | "break"
                        | "true"
                        | "false"
                        | "nil"
                        | "null"
                        | "library"
                        | "require"
                        | "install"
                        | "packages"
                        | "repos"
                        | "dependencies"
                        | "version"
                        | "upgrade"
                        | "never"
                        | "quietly"
                        | "c"
                        | "list"
                        | "packageversion"
                );
                let is_user_blacklisted = rules
                    .exclude_keywords
                    .iter()
                    .any(|kw| kw.eq_ignore_ascii_case(&pkg.name));

                if is_builtin_blacklisted || is_user_blacklisted {
                    continue;
                }
            }
            let pkg = pkg_opt
                .ok_or_else(|| format!("第 {line_num} 行第 {} 段输入格式无效", seg_idx + 1))?;
            packages.push(pkg);
            if packages.len() > MAX_PACKAGE_LINES {
                return Err(format!("单次最多处理 {MAX_PACKAGE_LINES} 行输入"));
            }
        }
    }
    Ok(packages)
}

pub(crate) fn normalize_managed_package_line(line: &str) -> Option<String> {
    let normalized = line.replace(['，', '、', '；'], ",");
    let call_re = Regex::new(r"(?i)\b(?:install\.packages|BiocManager::install|pacman::p_load|renv::install|pak::pkg_install)\s*\((.*)\)").ok()?;
    let shell = Regex::new(r#"(?i)\b(?:R|Rscript)\s+-e\s+[\"'](.+)[\"']"#).ok()?;
    let body = if let Some(captures) = call_re.captures(&normalized) {
        captures.get(1)?.as_str().to_string()
    } else if let Some(captures) = shell.captures(&normalized) {
        return normalize_managed_package_line(captures.get(1)?.as_str());
    } else {
        return None;
    };
    let quoted = Regex::new(r#"[\"']([^\"']+)[\"']"#).ok()?;
    let quoted_values = quoted
        .captures_iter(&body)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .collect::<Vec<_>>();
    if !quoted_values.is_empty() {
        return Some(quoted_values.join("\n"));
    }
    Some(
        body.trim()
            .trim_start_matches(|c: char| {
                c == 'c' || c == 'C' || c == 'l' || c == 'i' || c == 's' || c == 't' || c == '('
            })
            .trim_end_matches(')')
            .split([',', ';'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub(crate) fn normalize_markdown_table_line(line: &str) -> Option<String> {
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let cells: Vec<String> = line
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect();
    if cells.is_empty() {
        return Some(String::new());
    }
    if cells.iter().all(|cell| is_markdown_separator_cell(cell)) {
        return Some(String::new());
    }
    let package = cells.first().map(String::as_str).unwrap_or_default().trim();
    if package.is_empty() || matches!(package, "包名" | "package" | "Package" | "PACKAGE") {
        return Some(String::new());
    }
    let source_hint = cells
        .get(1)
        .map(String::as_str)
        .unwrap_or_default()
        .replace(['（', '）', '(', ')'], " ");
    Some(format!("{package} {source_hint}").trim().to_string())
}

pub(crate) fn is_markdown_separator_cell(cell: &str) -> bool {
    let trimmed = cell.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|character| matches!(character, '-' | ':' | ' '))
        && trimmed.contains('-')
}

pub(crate) fn is_comment_line(line: &str, rules: &InputRules) -> bool {
    let trimmed = line.trim();
    rules.comment_chars.iter().any(|c| trimmed.starts_with(c))
}

pub(crate) fn strip_r_parens_wrapper(line: &str) -> String {
    let trimmed = line.trim();
    for prefix in &[
        "c(",
        "list(",
        "library(",
        "require(",
        "requireNamespace(",
        "install.packages(",
        "devtools::install_github(",
        "remotes::install_github(",
        "remotes::install_version(",
        "BiocManager::install(",
    ] {
        if let Some(inner) = trimmed.strip_prefix(prefix) {
            if let Some(end) = inner.rfind(')').map(|pos| &inner[..pos]) {
                return end.to_string();
            }
        }
    }
    trimmed.to_string()
}

pub(crate) fn split_by_separators(line: &str, rules: &InputRules) -> Vec<String> {
    let normalized = line.replace(['，', '、', '；'], ",");
    let mut result = vec![normalized];

    for sep in &rules.separators {
        let mut next = Vec::new();
        for part in &result {
            for sub in part.split(sep.as_str()) {
                let trimmed = sub.trim();
                if !trimmed.is_empty() {
                    next.push(trimmed.to_string());
                }
            }
        }
        result = next;
    }

    if rules.split_spaces {
        let mut next = Vec::new();
        for part in &result {
            for sub in part.split_whitespace() {
                let trimmed = sub.trim();
                if !trimmed.is_empty() {
                    next.push(trimmed.to_string());
                }
            }
        }
        result = next;
    }

    if result.is_empty() {
        vec![line.to_string()]
    } else {
        result
    }
}

pub fn parse_input_line(line: &str) -> Option<PackageInput> {
    let raw = line.trim();
    if raw.is_empty() || raw.starts_with('#') {
        return None;
    }

    if raw.contains("://") && !raw.starts_with("http://") && !raw.starts_with("https://") {
        return None;
    }

    if raw.starts_with("http://") || raw.starts_with("https://") {
        if let Some(repository) = normalize_github_repository(raw) {
            return Some(PackageInput {
                raw: raw.to_string(),
                name: repository,
                version: String::new(),
                source_hint: Some("github".to_string()),
            });
        }
        if normalize_install_archive_url(raw).is_err() {
            return None;
        }
        let name = extract_package_name(raw);
        if !is_valid_package_name(&name) {
            return None;
        }
        return Some(PackageInput {
            raw: raw.to_string(),
            name,
            version: String::new(),
            source_hint: None,
        });
    }

    let local_archive_re = LOCAL_ARCHIVE_RE.get_or_init(|| {
        Regex::new(r"(?i)^(?:[A-Z]:[\\/]|\\\\)[^\r\n]+$").expect("固定本地路径正则必须有效")
    });
    if local_archive_re.is_match(raw) {
        let path = normalize_local_archive_path(raw).ok()?;
        let name = path
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .and_then(|file| {
                let stem = package_name_from_archive_file(file)?;
                stem.rsplit_once('_')
                    .filter(|(_, version)| {
                        version.chars().next().is_some_and(|c| c.is_ascii_digit())
                    })
                    .map(|(name, _)| name.to_string())
                    .or(Some(stem))
            })
            .filter(|name| is_valid_package_name(name))?;
        return Some(PackageInput {
            raw: path,
            name,
            version: String::new(),
            source_hint: Some("local".to_string()),
        });
    }

    if raw.contains('\\') || raw.starts_with('/') {
        return None;
    }

    if raw.contains("http://") || raw.contains("https://") {
        return None;
    }

    let url_re =
        INPUT_URL_RE.get_or_init(|| Regex::new(r"https?://\S+").expect("固定 URL 正则必须有效"));
    let clean = url_re.replace_all(raw, " ");
    let package_re = INPUT_PACKAGE_RE.get_or_init(|| {
        Regex::new(r"^\s*([A-Za-z0-9][A-Za-z0-9._\-/]*)").expect("固定包名正则必须有效")
    });
    let captures = package_re.captures(&clean)?;
    let name = captures
        .get(1)?
        .as_str()
        .trim_matches(['"', '\''])
        .to_string();
    if !is_valid_package_name(&name) {
        return None;
    }
    let remaining = &clean[captures.get(0)?.end()..];
    let version_re = INPUT_VERSION_RE.get_or_init(|| {
        Regex::new(r"^\s*(?:v|V)?([0-9]+[0-9A-Za-z.\-]*)").expect("固定版本正则必须有效")
    });
    let version = version_re
        .captures(remaining)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
        .unwrap_or_default();
    if !version.is_empty() && !is_clean_version(&version) {
        return None;
    }

    let hint_re = SOURCE_HINT_RE.get_or_init(|| {
        Regex::new(r"(?i)\b(cran|bioconductor|bioc|github)\b").expect("源提示正则必须有效")
    });
    let source_hint = hint_re
        .captures(remaining)
        .and_then(|capture| capture.get(1))
        .map(|value| {
            let lower = value.as_str().to_ascii_lowercase();
            if lower == "bioconductor" {
                "bioc".to_string()
            } else {
                lower
            }
        });

    Some(PackageInput {
        raw: raw.to_string(),
        name,
        version,
        source_hint,
    })
}

pub fn extract_package_name(input: &str) -> String {
    let value = input.trim().trim_matches(['"', '\'']);
    if value.starts_with("http://") || value.starts_with("https://") {
        if let Some(repository) = normalize_github_repository(value) {
            return repository
                .rsplit('/')
                .next()
                .unwrap_or(&repository)
                .to_string();
        }
        let file = value.rsplit('/').next().unwrap_or(value);
        if let Some((name, _)) = file.split_once('_') {
            return name.to_string();
        }
        if let Some(name) = package_name_from_archive_file(file) {
            return name;
        }
        return file
            .trim_end_matches(".html")
            .split('.')
            .next()
            .unwrap_or(file)
            .to_string();
    }

    let quote_re =
        QUOTED_VALUE_RE.get_or_init(|| Regex::new(r#""([^"]+)""#).expect("固定引号正则必须有效"));
    if let Some(value) = quote_re
        .captures(value)
        .and_then(|capture| capture.get(1))
        .map(|match_| match_.as_str())
    {
        if value.starts_with("http") {
            return extract_package_name(value);
        }
        let without_ref = value.split('@').next().unwrap_or(value);
        return without_ref
            .rsplit('/')
            .next()
            .unwrap_or(without_ref)
            .to_string();
    }

    value.rsplit('/').next().unwrap_or(value).to_string()
}
