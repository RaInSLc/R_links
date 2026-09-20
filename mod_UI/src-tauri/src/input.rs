use crate::models::{InputRules, PackageInput, MAX_PACKAGE_LINES};
use regex::Regex;
use std::sync::OnceLock;

#[path = "input/managed.rs"]
mod managed;

pub(crate) use managed::{normalize_managed_package_line, normalize_markdown_table_line};

static INPUT_URL_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_PACKAGE_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_VERSION_RE: OnceLock<Regex> = OnceLock::new();
static QUOTED_VALUE_RE: OnceLock<Regex> = OnceLock::new();
static SOURCE_HINT_RE: OnceLock<Regex> = OnceLock::new();
static LOCAL_ARCHIVE_RE: OnceLock<Regex> = OnceLock::new();

use crate::logic::*;

/// 本地归档路径（`C:\...` 或 UNC `\\server\...`）的正则，供整行识别复用。
fn local_archive_regex() -> &'static Regex {
    LOCAL_ARCHIVE_RE.get_or_init(|| {
        Regex::new(r"(?i)^(?:[A-Z]:[\\/]|\\\\)[^\r\n]+$").expect("固定本地路径正则必须有效")
    })
}

/// 判断一行是否以 `http://` / `https://` 开头。
///
/// 协议名按大小写不敏感匹配：`Url::parse` 本身会把协议名归一化为小写，
/// 前端 `utils-url.ts` 也用 `/^https?:\/\//i` 判定，因此这里必须同样宽松，
/// 否则 `HTTPS://github.com/o/r` 这类输入会出现"前端算作合法 URL、后端整批拒绝"的割裂。
pub(crate) fn starts_with_http_scheme(value: &str) -> bool {
    fn prefix_eq(bytes: &[u8], prefix: &[u8]) -> bool {
        bytes.len() >= prefix.len() && bytes[..prefix.len()].eq_ignore_ascii_case(prefix)
    }
    let bytes = value.as_bytes();
    prefix_eq(bytes, b"http://") || prefix_eq(bytes, b"https://")
}

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
                    .ok_or_else(|| format!("第 {} 行包管理器输入格式无效", line_idx + 1))?;
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

        if starts_with_http_scheme(trimmed) {
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

        // 本地归档路径整行处理，避免路径中的 `,`/`;` 被分隔符拆断。
        if local_archive_regex().is_match(trimmed) {
            if let Some(pkg) = parse_input_line(trimmed) {
                packages.push(pkg);
            } else {
                return Err(format!("第 {line_num} 行本地归档路径格式无效"));
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

    if raw.contains("://") && !starts_with_http_scheme(raw) {
        return None;
    }

    if starts_with_http_scheme(raw) {
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

    if local_archive_regex().is_match(raw) {
        let path = normalize_local_archive_path(raw).ok()?;
        let name = path
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .and_then(package_name_from_archive_file)
            .map(|stem| strip_archive_version(&stem))
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
    if starts_with_http_scheme(value) {
        if let Some(repository) = normalize_github_repository(value) {
            return repository
                .rsplit('/')
                .next()
                .unwrap_or(&repository)
                .to_string();
        }
        let file = value.rsplit('/').next().unwrap_or(value);
        // 归档文件名统一剥离扩展名与 `_`/`-` 版本号后缀，避免把版本并入包名。
        if let Some(stem) = package_name_from_archive_file(file) {
            let name = strip_archive_version(&stem);
            if is_valid_package_name(&name) {
                return name;
            }
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
