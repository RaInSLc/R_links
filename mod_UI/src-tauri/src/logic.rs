use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::models::{
    normalize_cran_mirror_url, normalize_https_url, url_has_explicit_port, GenerateOptions,
    HistoryRecord, InputRules, PackageInput, ReverseDependenciesInfo, SearchResult,
    MAX_FIELD_CHARS, MAX_HISTORY_COMMAND_CHARS, MAX_HISTORY_RECORDS, MAX_INPUT_CHARS,
    MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
};

const MAX_GENERATE_METHOD_CHARS: usize = 32;
const MAX_GENERATE_SEARCH_RESULTS: usize = MAX_PACKAGE_LINES * 16;
const MAX_VERSION_CHARS: usize = 64;
const MAX_RESULT_SOURCE_CHARS: usize = 16;
const MAX_RESULT_MESSAGE_CHARS: usize = 512;
const MAX_INSTALL_ARCHIVE_FILE_CHARS: usize = 256;
const MAX_INPUT_LINE_BYTES: usize = 2_048;
const MAX_HISTORY_SCAN_LINES: usize = MAX_HISTORY_RECORDS;
const INSTALL_ARCHIVE_EXTENSIONS: &[&str] = &[".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".zip"];

static INPUT_URL_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_PACKAGE_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_VERSION_RE: OnceLock<Regex> = OnceLock::new();
static QUOTED_VALUE_RE: OnceLock<Regex> = OnceLock::new();
static SOURCE_HINT_RE: OnceLock<Regex> = OnceLock::new();
static HISTORY_VERSION_RE: OnceLock<Regex> = OnceLock::new();
static BASE_HISTORY_RE: OnceLock<[Regex; 4]> = OnceLock::new();
static INSTALL_URL_HISTORY_RE: OnceLock<Regex> = OnceLock::new();
static CRAN_HISTORY_RE: OnceLock<[Regex; 2]> = OnceLock::new();
static REVERSE_DEPS_RE: OnceLock<Regex> = OnceLock::new();

#[cfg(test)]
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

fn is_comment_line(line: &str, rules: &InputRules) -> bool {
    let trimmed = line.trim();
    rules.comment_chars.iter().any(|c| trimmed.starts_with(c))
}

fn strip_r_parens_wrapper(line: &str) -> String {
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

fn split_by_separators(line: &str, rules: &InputRules) -> Vec<String> {
    let mut result = vec![line.to_string()];

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

#[cfg(test)]
pub fn generate_script(
    input: &str,
    options: &GenerateOptions,
    results: &[SearchResult],
) -> Result<String, String> {
    generate_script_with_remote_versions(input, options, results, true)
}

pub fn generate_script_with_rules(
    input: &str,
    options: &GenerateOptions,
    results: &[SearchResult],
    show_remote_version: bool,
    rules: &InputRules,
) -> Result<String, String> {
    let requested_method = normalize_generate_method(&options.method)?;
    validate_search_results_count(results)?;
    let packages = parse_inputs_filtered(input, rules)?;
    generate_script_inner(
        options,
        results,
        show_remote_version,
        requested_method,
        packages,
    )
}

#[cfg(test)]
pub fn generate_script_with_remote_versions(
    input: &str,
    options: &GenerateOptions,
    results: &[SearchResult],
    show_remote_version: bool,
) -> Result<String, String> {
    let requested_method = normalize_generate_method(&options.method)?;
    validate_search_results_count(results)?;
    let packages = parse_inputs(input)?;
    generate_script_inner(
        options,
        results,
        show_remote_version,
        requested_method,
        packages,
    )
}

fn generate_script_inner(
    options: &GenerateOptions,
    results: &[SearchResult],
    show_remote_version: bool,
    requested_method: &str,
    packages: Vec<PackageInput>,
) -> Result<String, String> {
    if packages.is_empty() {
        return Ok("等待输入...".to_string());
    }
    let results = sanitize_search_results(results);

    let mirror = if options.mirror.trim().is_empty() {
        "https://cloud.r-project.org".to_string()
    } else {
        normalize_cran_mirror_url(&options.mirror)?
    };

    let packages_for_verify = if options.append_verify {
        packages.clone()
    } else {
        Vec::new()
    };

    if requested_method == "checkSystem" {
        return generate_check_system_script(&packages);
    }

    let mut output = Vec::new();
    for package in packages {
        let mut is_cran_archive = false;
        let is_archive_url = package.raw.starts_with("https://");
        if is_archive_url && !matches!(requested_method, "auto" | "devtools" | "remotes") {
            return Err(format!(
                "安装归档 URL 仅支持智能路由、devtools 或 remotes，不能使用 {requested_method}"
            ));
        }
        let mut value = if matches!(requested_method, "devtools" | "remotes")
            || (requested_method == "auto" && is_archive_url)
        {
            package.raw.clone()
        } else {
            package.name.clone()
        };
        let mut version = package.version.clone();
        let mut method = if requested_method == "auto" && is_archive_url {
            "remotes".to_string()
        } else {
            requested_method.to_string()
        };

        if let Some(best) = (!is_archive_url)
            .then(|| choose_best_result(&package.name, &results, package.source_hint.as_deref()))
            .flatten()
        {
            is_cran_archive = best.source == "cran" && best.repository == "archive";
            let source_label = source_label(&best.source);
            let remote_version = if show_remote_version {
                format!(": v{}", best.latest_version)
            } else {
                String::new()
            };
            if version.is_empty() {
                let status_text = if is_cran_archive {
                    format!("已下架并归档: v{}", best.latest_version)
                } else {
                    format!("已验证{remote_version}")
                };
                output.push(format!("# [{source_label} {status_text} | 自动同步]"));
                if (show_remote_version || is_cran_archive)
                    && is_clean_version(&best.latest_version)
                    && best.source != "github"
                {
                    version = best.latest_version.clone();
                }
            } else {
                let status_text = if is_cran_archive {
                    format!("已下架并归档: v{}", best.latest_version)
                } else {
                    format!("最新版本{remote_version}")
                };
                output.push(format!("# [{source_label} {status_text} | 保留指定版本]"));
                if best.source == "bioc"
                    && !best.latest_version.is_empty()
                    && best.latest_version != version
                {
                    output.push(format!(
                        "# [提示: Bioconductor 未匹配版本 {version}，将使用 Release]"
                    ));
                }
            }

            if requested_method == "auto" {
                match best.source.as_str() {
                    "biocGit" => {
                        method = "biocGit".to_string();
                        let matched_version = if show_remote_version {
                            best.latest_version.as_str()
                        } else {
                            ""
                        };
                        version = format!("{matched_version}|{}", best.repository);
                    }
                    "bioc" => method = "biocManager".to_string(),
                    "github" => {
                        method = "github".to_string();
                        if !best.repository.is_empty() {
                            value = best.repository.clone();
                        }
                    }
                    _ => method = "remotesVersion".to_string(),
                }
            }
        } else if !is_archive_url
            && results
                .iter()
                .any(|result| result.package.eq_ignore_ascii_case(&package.name))
        {
            output.push("# [提示: CRAN/Bioconductor/GitHub 均未找到]".to_string());
        }

        if requested_method == "auto"
            && !is_archive_url
            && !matches!(method.as_str(), "github" | "biocManager" | "biocGit")
        {
            method = if package.name.contains('/')
                && normalize_github_repository(&package.name).is_some()
            {
                "github".to_string()
            } else if is_clean_version(&version) {
                "remotesVersion".to_string()
            } else {
                "base".to_string()
            };
        }

        let command_mirror = if is_cran_archive {
            "https://cloud.r-project.org".to_string()
        } else {
            mirror.clone()
        };

        output.push(generate_command(
            &value,
            &method,
            &version,
            options.conditional,
            &command_mirror,
            options.install_dependencies,
        )?);
    }

    let mut script = output.join("\r\n") + "\r\n";
    if options.append_verify && !packages_for_verify.is_empty() {
        let verify = generate_verify_script(&packages_for_verify);
        script.push_str(&verify);
    }
    validate_script_size(&script)?;
    Ok(script)
}

fn generate_check_system_script(packages: &[PackageInput]) -> Result<String, String> {
    let names = packages
        .iter()
        .map(|item| format!("\"{}\"", escape_r(&local_package_name(&item.name))))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        "# 1. 定义需要检测的包列表\r\n\
         packages_to_check <- c({names})\r\n\r\n\
         # 2. 逐个检测包是否已安装，并尝试加载捕获报错\r\n\
         check_results <- lapply(packages_to_check, function(p) {{\r\n\
         \x20 installed <- requireNamespace(p, quietly = TRUE)\r\n\
         \x20 version <- if (installed) tryCatch(as.character(packageVersion(p)), error = function(e) NA_character_) else NA_character_\r\n\
         \x20 load_error <- NA_character_\r\n\
         \x20 loaded <- FALSE\r\n\
         \x20 if (installed) {{\r\n\
         \x20   loaded <- tryCatch({{\r\n\
         \x20     suppressPackageStartupMessages(library(p, character.only = TRUE))\r\n\
         \x20     TRUE\r\n\
         \x20   }}, error = function(e) {{\r\n\
         \x20     load_error <<- conditionMessage(e)\r\n\
         \x20     FALSE\r\n\
         \x20   }})\r\n\
         \x20 }}\r\n\
         \x20 data.frame(package = p, installed = installed, loaded = loaded, version = version, error = load_error, stringsAsFactors = FALSE)\r\n\
         }})\r\n\r\n\
         # 3. 汇总输出检测结果\r\n\
         check_results <- do.call(rbind, check_results)\r\n\
         print(check_results, row.names = FALSE)\r\n\r\n\
         failed <- check_results[!check_results$installed | !check_results$loaded, ]\r\n\
         cat(sprintf(\"\\n=== 检测完成: %d/%d 包可正常加载 ===\\n\", sum(check_results$installed & check_results$loaded), nrow(check_results)))\r\n\
         if (nrow(failed) > 0) {{\r\n\
         \x20 cat(\"以下包未安装或加载报错：\\n\")\r\n\
         \x20 print(failed, row.names = FALSE)\r\n\
         }}\r\n"
    );
    validate_script_size(&script)?;
    Ok(script)
}

fn generate_verify_script(packages: &[PackageInput]) -> String {
    let names = packages
        .iter()
        .map(|item| format!("\"{}\"", escape_r(&local_package_name(&item.name))))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\r\n# ===== 安装结果验证 =====\r\n\
         packages <- c({names})\r\n\
         results <- sapply(packages, function(p) {{\r\n\
         \x20 ver <- tryCatch(as.character(packageVersion(p)), error = function(e) NA)\r\n\
         \x20 if (!is.na(ver)) {{\r\n\
         \x20   cat(sprintf(\"[OK] %s (v%s)\\n\", p, ver))\r\n\
         \x20   return(TRUE)\r\n\
         \x20 }} else {{\r\n\
         \x20   cat(sprintf(\"[FAIL] %s\\n\", p))\r\n\
         \x20   return(FALSE)\r\n\
         \x20 }}\r\n\
         }})\r\n\
         cat(sprintf(\"\\n=== 验证完成: %d/%d 包安装成功 ===\\n\", sum(results, na.rm=TRUE), length(results)))\r\n"
    )
}

fn normalize_generate_method(value: &str) -> Result<&'static str, String> {
    if value.len() > MAX_GENERATE_METHOD_CHARS
        || value.chars().any(|character| character.is_control())
    {
        return Err("安装方式无效".to_string());
    }
    match value.trim() {
        "auto" => Ok("auto"),
        "devtools" => Ok("devtools"),
        "remotes" => Ok("remotes"),
        "github" => Ok("github"),
        "base" => Ok("base"),
        "version" => Ok("version"),
        "biocManager" => Ok("biocManager"),
        "checkSystem" => Ok("checkSystem"),
        _ => Err("不支持的安装方式".to_string()),
    }
}

fn validate_search_results_count(results: &[SearchResult]) -> Result<(), String> {
    if results.len() > MAX_GENERATE_SEARCH_RESULTS {
        return Err(format!(
            "检索结果数量过多，最多允许 {MAX_GENERATE_SEARCH_RESULTS} 条"
        ));
    }
    Ok(())
}

fn choose_best_result<'a>(
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
            _ => 4,
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

fn result_identity_matches_package(result: &SearchResult, package: &str) -> bool {
    result.real_name.eq_ignore_ascii_case(package)
        || result_repository_name_matches_package(result, package)
}

fn result_repository_name_matches_package(result: &SearchResult, package: &str) -> bool {
    result
        .repository
        .rsplit('/')
        .next()
        .is_some_and(|repo| repo.eq_ignore_ascii_case(package))
}

fn sanitize_search_results(results: &[SearchResult]) -> Vec<SearchResult> {
    results
        .iter()
        .filter_map(sanitize_search_result)
        .take(MAX_GENERATE_SEARCH_RESULTS)
        .collect()
}

fn sanitize_search_result(result: &SearchResult) -> Option<SearchResult> {
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
    })
}

fn search_result_fields_within_bounds(result: &SearchResult) -> bool {
    result.package.len() <= MAX_FIELD_CHARS
        && result.requested_version.len() <= MAX_VERSION_CHARS
        && result.latest_version.len() <= MAX_VERSION_CHARS
        && result.repository.len() <= MAX_FIELD_CHARS
        && result.real_name.len() <= MAX_FIELD_CHARS
        && result.source.len() <= MAX_RESULT_SOURCE_CHARS
        && result.message.len() <= MAX_RESULT_MESSAGE_CHARS
}

fn is_trusted_found_result(
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
        _ => false,
    }
}

fn clean_result_package(value: &str) -> Option<String> {
    let trimmed = value.trim();
    is_valid_package_name(trimmed).then(|| trimmed.to_string())
}

fn clean_result_version(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }
    is_clean_version(trimmed).then(|| trimmed.to_string())
}

fn clean_result_source(value: &str) -> Option<String> {
    match value.trim() {
        "cran" | "bioc" | "biocGit" | "github" | "none" => Some(value.trim().to_string()),
        _ => None,
    }
}

fn clean_result_repository(source: &str, value: &str) -> Option<String> {
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
        _ => {
            if trimmed.len() <= MAX_FIELD_CHARS && !trimmed.chars().any(char::is_control) {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
    }
}

fn clean_result_text(value: &str) -> String {
    truncate_utf8_bytes(
        &value
            .trim()
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>(),
        MAX_RESULT_MESSAGE_CHARS,
    )
}

fn truncate_utf8_bytes(value: &str, limit: usize) -> String {
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

fn generate_command(
    value: &str,
    method: &str,
    version: &str,
    conditional: bool,
    mirror: &str,
    install_dependencies: bool,
) -> Result<String, String> {
    let dependencies = if install_dependencies {
        "TRUE"
    } else {
        "FALSE"
    };
    let local_value = local_package_name(value);
    let escaped_value = escape_r(&local_value);
    let escaped_mirror = escape_r(mirror);
    let mut package_name = local_package_name(&extract_package_name(value));
    let mut effective_version = version.to_string();

    let raw = match method {
        "devtools" => {
            let url = normalize_install_archive_url(value)?;
            format!(
                "devtools::install_url(\"{}\", dependencies = {dependencies})",
                escape_r(&url)
            )
        }
        "remotes" => {
            let url = normalize_install_archive_url(value)?;
            format!(
                "remotes::install_url(\"{}\", dependencies = {dependencies})",
                escape_r(&url)
            )
        }
        "github" => {
            let Some(repository) = normalize_github_repository(value) else {
                return Err(format!("{value} 不是有效的 GitHub 仓库标识，应为 owner/repo"));
            };
            package_name = repository
                .rsplit('/')
                .next()
                .unwrap_or(&repository)
                .to_string();

            let repo_with_ref = if !effective_version.is_empty() {
                let clean_ver = if effective_version.starts_with('v') {
                    effective_version.clone()
                } else {
                    format!("v{}", effective_version)
                };
                format!("{}@{}", repository, clean_ver)
            } else {
                repository
            };

            effective_version.clear();
            format!(
                "remotes::install_github(\"{}\", upgrade = \"never\", dependencies = {dependencies})",
                escape_r(&repo_with_ref)
            )
        }
        "base" => format!(
            "install.packages(\"{escaped_value}\", repos = \"{escaped_mirror}\", dependencies = {dependencies})"
        ),
        "version" => return Ok(format!("packageVersion(\"{escaped_value}\")")),
        "remotesVersion" => {
            if version.is_empty() {
                return Err(format!("{value} 缺少可用于 install_version 的版本号"));
            }
            if !is_clean_version(version) {
                return Err(format!("{value} 的版本号格式不适合 install_version: {version}"));
            }
            format!(
                "remotes::install_version(\"{escaped_value}\", version = \"{}\", repos = \"{escaped_mirror}\", upgrade = \"never\", dependencies = {dependencies})",
                escape_r(version)
            )
        }
        "biocManager" => format!(
            "BiocManager::install(\"{escaped_value}\", update = FALSE, ask = FALSE, dependencies = {dependencies})"
        ),
        "biocGit" => {
            let (real_version, bioc_version) =
                version.split_once('|').unwrap_or((version, "3.18"));
            if !is_valid_package_name(value) || value.contains('/') {
                return Err(format!("{value} 不是有效的 Bioconductor 包名"));
            }
            if !is_valid_bioc_version(bioc_version) {
                return Err(format!("Bioconductor 版本格式无效: {bioc_version}"));
            }
            effective_version = real_version.to_string();
            let release = format!("RELEASE_{}", bioc_version.replace('.', "_"));
            format!(
                "remotes::install_git(\"https://git.bioconductor.org/packages/{escaped_value}\", ref = \"{release}\", upgrade = \"never\", dependencies = {dependencies})"
            )
        }
        "auto" => format!(
            "install.packages(\"{escaped_value}\", repos = \"{escaped_mirror}\", dependencies = {dependencies})"
        ),
        _ => return Err(format!("不支持的安装方式: {method}")),
    };

    if !conditional || package_name.is_empty() {
        return Ok(raw);
    }

    let version_check = if !effective_version.is_empty() && method != "biocManager" {
        format!(
            " || packageVersion(\"{}\") != \"{}\"",
            escape_r(&package_name),
            escape_r(&effective_version)
        )
    } else {
        String::new()
    };
    let display_version = if effective_version.is_empty() {
        String::new()
    } else {
        format!(" ({})", escape_r(&effective_version))
    };
    Ok(format!(
        "if (!requireNamespace(\"{}\", quietly = TRUE){version_check}) {{\r\n  {raw}\r\n}} else {{\r\n  message(\"{}{display_version} 已存在，跳过安装。\")\r\n}}",
        escape_r(&package_name),
        escape_r(&package_name)
    ))
}

pub fn build_history_records(script: &str) -> Vec<HistoryRecord> {
    if script.len() > MAX_SCRIPT_CHARS {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    script
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .rev()
        .take(MAX_HISTORY_SCAN_LINES)
        .filter_map(|line| {
            supported_history_command(line).and_then(|command| {
                history_metadata_from_command(&command).map(|(package_name, version, tool_name)| {
                    (command, package_name, version, tool_name)
                })
            })
        })
        .filter(|(command, _, _, _)| seen.insert(command.clone()))
        .enumerate()
        .map(
            |(index, (command, package_name, version, tool_name))| HistoryRecord {
                id: format!("{now}-{index}"),
                command,
                package_name,
                version,
                tool_name,
                created_at: now.to_string(),
            },
        )
        .take(MAX_HISTORY_RECORDS)
        .collect()
}

pub fn clean_script(script: &str) -> Result<String, String> {
    validate_script_size(script)?;
    let cleaned = script
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\r\n");
    validate_script_size(&cleaned)?;
    Ok(cleaned)
}

pub fn history_metadata_from_command(command: &str) -> Option<(String, String, String)> {
    let command = supported_history_command(command)?;
    if command.is_empty() {
        return None;
    }

    let version_re = HISTORY_VERSION_RE.get_or_init(|| {
        Regex::new(r#"version\s*=\s*"([^"]+)""#).expect("固定历史版本正则必须有效")
    });
    let version = version_re
        .captures(&command)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
        .unwrap_or_default();
    let tool_name = if command.contains("install_github") {
        "GitHub"
    } else if command.contains("BiocManager") || command.contains("install_git") {
        "Bioconductor"
    } else if command.contains("remotes") {
        "remotes"
    } else if command.contains("devtools") {
        "devtools"
    } else {
        "base R"
    };

    Some((
        extract_package_name(&command),
        version,
        tool_name.to_string(),
    ))
}

pub fn supported_history_command(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty()
        || command.starts_with('#')
        || command.len() > MAX_HISTORY_COMMAND_CHARS
        || command.chars().any(char::is_control)
    {
        return None;
    }
    if !looks_like_supported_history_command(command) {
        return None;
    }

    if BASE_HISTORY_RE
        .get_or_init(|| {
            [
                Regex::new(r#"^packageVersion\("[A-Za-z0-9._-]{1,128}"\)$"#)
                    .expect("固定 packageVersion 历史命令正则必须有效"),
                Regex::new(
                    r#"^BiocManager::install\("[A-Za-z0-9._-]{1,128}", update = FALSE, ask = FALSE, dependencies = (TRUE|FALSE)\)$"#,
                )
                .expect("固定 BiocManager 历史命令正则必须有效"),
                Regex::new(
                    r#"^remotes::install_github\("[A-Za-z0-9._-]{1,100}/[A-Za-z0-9._-]{1,100}", upgrade = "never", dependencies = (TRUE|FALSE)\)$"#,
                )
                .expect("固定 install_github 历史命令正则必须有效"),
                Regex::new(
                    r#"^remotes::install_git\("https://git\.bioconductor\.org/packages/[A-Za-z0-9._-]{1,128}", ref = "RELEASE_[0-9]+_[0-9]+", upgrade = "never", dependencies = (TRUE|FALSE)\)$"#,
                )
                .expect("固定 install_git 历史命令正则必须有效"),
            ]
        })
        .iter()
        .any(|regex| regex.is_match(command))
    {
        return Some(command.to_string());
    }

    if supported_install_url_history_command(command) {
        return Some(command.to_string());
    }

    if supported_cran_history_command(command) {
        return Some(command.to_string());
    }

    let indented = command.trim_start();
    if indented.len() != command.len() {
        return supported_history_command(indented);
    }

    None
}

fn looks_like_supported_history_command(command: &str) -> bool {
    matches!(
        command.as_bytes().first(),
        Some(b'B' | b'd' | b'i' | b'p' | b'r')
    ) && (command.starts_with("BiocManager::install(")
        || command.starts_with("devtools::install_url(")
        || command.starts_with("install.packages(")
        || command.starts_with("packageVersion(")
        || command.starts_with("remotes::install_"))
}

fn supported_install_url_history_command(command: &str) -> bool {
    let regex = INSTALL_URL_HISTORY_RE.get_or_init(|| {
        Regex::new(
            r#"^(remotes|devtools)::install_url\("([^"\r\n]{1,2048})", dependencies = (TRUE|FALSE)\)$"#,
        )
        .expect("固定 install_url 历史命令正则必须有效")
    });
    regex
        .captures(command)
        .and_then(|capture| capture.get(2))
        .is_some_and(|url| normalize_install_archive_url(url.as_str()).is_ok())
}

fn supported_cran_history_command(command: &str) -> bool {
    CRAN_HISTORY_RE
        .get_or_init(|| {
            [
                Regex::new(
                    r#"^install\.packages\("[A-Za-z0-9._-]{1,128}", repos = "([^"\r\n]{1,2048})", dependencies = (TRUE|FALSE)\)$"#,
                )
                .expect("固定 CRAN install.packages 历史命令正则必须有效"),
                Regex::new(
                    r#"^remotes::install_version\("[A-Za-z0-9._-]{1,128}", version = "[0-9][0-9A-Za-z.-]{0,63}", repos = "([^"\r\n]{1,2048})", upgrade = "never", dependencies = (TRUE|FALSE)\)$"#,
                )
                .expect("固定 CRAN install_version 历史命令正则必须有效"),
            ]
        })
        .iter()
        .any(|regex| {
            regex
                .captures(command)
            .and_then(|capture| capture.get(1))
            .is_some_and(|mirror| normalize_cran_mirror_url(mirror.as_str()).is_ok())
        })
}

pub fn infer_bioc_version(major: i32, minor: i32) -> Option<i32> {
    match major {
        1 if minor >= 50 && minor % 2 == 0 => Some((minor - 50) / 2 + 18),
        1 if (34..50).contains(&minor) && minor % 2 == 0 => Some((minor - 34) / 2 + 10),
        2 if minor >= 0 && minor % 2 == 0 => Some(minor / 2 + 21),
        _ => None,
    }
}

fn source_label(source: &str) -> &str {
    match source {
        "cran" => "CRAN",
        "bioc" => "Bioconductor",
        "biocGit" => "Bioconductor 历史版本",
        "github" => "GitHub",
        _ => "未知来源",
    }
}

fn is_clean_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= MAX_VERSION_CHARS
        && version
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, '.' | '-'))
}

fn normalize_install_archive_url(value: &str) -> Result<String, String> {
    let normalized = normalize_https_url(value, "安装 URL")?;
    let parsed = Url::parse(&normalized).map_err(|_| "安装 URL 必须是有效 URL".to_string())?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("安装 URL 不允许包含查询参数或片段".to_string());
    }
    let file_name = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or_default();
    if file_name.is_empty()
        || file_name.len() > MAX_INSTALL_ARCHIVE_FILE_CHARS
        || file_name.chars().any(char::is_control)
    {
        return Err("安装 URL 文件名无效或长度过长".to_string());
    }
    let lower_file_name = file_name.to_ascii_lowercase();
    if !INSTALL_ARCHIVE_EXTENSIONS
        .iter()
        .any(|extension| lower_file_name.ends_with(extension))
    {
        return Err("安装 URL 必须指向 R 包归档文件".to_string());
    }
    Ok(normalized)
}

fn package_name_from_archive_file(file_name: &str) -> Option<String> {
    let lower_file_name = file_name.to_ascii_lowercase();
    INSTALL_ARCHIVE_EXTENSIONS
        .iter()
        .find(|extension| lower_file_name.ends_with(**extension))
        .and_then(|extension| file_name.get(..file_name.len().saturating_sub(extension.len())))
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
}

fn escape_r(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn validate_input_size(input: &str) -> Result<(), String> {
    if input.len() > MAX_INPUT_CHARS {
        return Err(format!("输入内容过长，最多允许 {MAX_INPUT_CHARS} 字节"));
    }
    if input
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
    {
        return Err("输入内容包含非法控制字符".to_string());
    }
    let mut line_count = 0usize;
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if line.len() > MAX_INPUT_LINE_BYTES {
            return Err(format!(
                "单行输入过长，最多允许 {MAX_INPUT_LINE_BYTES} 字节"
            ));
        }
        if trimmed.starts_with('#') {
            continue;
        }
        line_count += 1;
        if line_count > MAX_PACKAGE_LINES {
            return Err(format!("单次最多处理 {MAX_PACKAGE_LINES} 行输入"));
        }
    }
    Ok(())
}

pub fn validate_script_size(script: &str) -> Result<(), String> {
    if script.len() > MAX_SCRIPT_CHARS {
        return Err(format!("脚本内容过长，最多允许 {MAX_SCRIPT_CHARS} 字节"));
    }
    Ok(())
}

pub fn is_valid_package_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 {
        return false;
    }
    if value.contains('/') {
        return is_valid_github_repository(value);
    }
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

fn local_package_name(value: &str) -> String {
    normalize_github_repository(value)
        .and_then(|repository| repository.rsplit('/').next().map(ToString::to_string))
        .unwrap_or_else(|| value.to_string())
}

pub fn is_valid_github_repository(value: &str) -> bool {
    if value.len() > 200 || value.contains('\\') || value.contains("..") {
        return false;
    }
    let parts = value.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() != 2 {
        return false;
    }
    is_valid_github_owner_segment(parts[0]) && is_valid_github_repo_segment(parts[1])
}

fn is_valid_github_owner_segment(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 39
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
    {
        return false;
    }
    value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn is_valid_github_repo_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

pub fn normalize_github_repository(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches(['"', '\'']).trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.contains("://") {
        return github_repository_from_url(trimmed);
    }

    let repository = trimmed.trim_end_matches(".git");
    is_valid_github_repository(repository).then(|| repository.to_string())
}

fn github_repository_from_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value).ok()?;
    if parsed.scheme() != "https" || parsed.host_str()? != "github.com" {
        return None;
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || url_has_explicit_port(value)
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let segments = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() != 2 {
        return None;
    }

    let repository = format!("{}/{}", segments[0], segments[1].trim_end_matches(".git"));
    is_valid_github_repository(&repository).then_some(repository)
}

pub fn is_allowed_browser_search_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https"
        || url.host_str() != Some("www.google.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != "/search"
    {
        return false;
    }
    let pairs = url.query_pairs().collect::<Vec<_>>();
    pairs.len() == 1
        && pairs
            .first()
            .is_some_and(|(key, value)| key == "q" && !value.trim().is_empty())
}

pub fn build_package_page_url(package: &str, source: &str, repository: &str) -> Result<String, String> {
    let package = package.trim();
    if package.is_empty() {
        return Err("包名为空".to_string());
    }
    match source {
        "cran" => {
            if !is_valid_package_name(package) {
                return Err(format!("无效的 CRAN 包名: {package}"));
            }
            Ok(format!("https://cran.r-project.org/package={package}"))
        }
        "bioc" => {
            if !is_valid_package_name(package) {
                return Err(format!("无效的 Bioconductor 包名: {package}"));
            }
            Ok(format!("https://bioconductor.org/packages/{package}"))
        }
        "github" => {
            let repo = repository.trim();
            if !is_valid_github_repository(repo) {
                return Err(format!("无效的 GitHub 仓库地址: {repo}"));
            }
            Ok(format!("https://github.com/{repo}"))
        }
        _ => Err(format!("不支持的来源类型: {source}")),
    }
}

pub fn is_allowed_package_page_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return false;
    }
    let host = url.host_str();
    let path = url.path();
    match host {
        Some("cran.r-project.org") => {
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            segs.len() == 2 && segs[0] == "package" && is_valid_package_name(segs[1])
        }
        Some("bioconductor.org") => {
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            segs.len() == 2 && is_valid_package_name(segs[1])
        }
        Some("github.com") => {
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            segs.len() == 2
                && is_valid_github_repository(&format!("{}/{}", segs[0], segs[1]))
        }
        _ => false,
    }
}

fn is_valid_bioc_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

pub fn parse_reverse_dependencies(html: &str, package: &str) -> Option<ReverseDependenciesInfo> {
    let mut depends = 0usize;
    let mut imports = 0usize;
    let mut suggests = 0usize;
    let mut linking_to = 0usize;
    let mut matched = false;

    let field_re = REVERSE_DEPS_RE.get_or_init(|| {
        Regex::new(r#"<td>\s*Reverse\s+(depends|imports|suggests|linking\s+to)\s*:</td>\s*<td[^>]*>\s*<a[^>]*>(\d+)</a>"#)
            .expect("固定反向依赖正则必须有效")
    });

    for capture in field_re.captures_iter(html) {
        let field = capture.get(1)?.as_str();
        let count: usize = capture.get(2)?.as_str().parse().ok()?;
        match field {
            "depends" => depends = count,
            "imports" => imports = count,
            "suggests" => suggests = count,
            "linking to" => linking_to = count,
            _ => continue,
        }
        matched = true;
    }

    if !matched {
        return None;
    }

    Some(ReverseDependenciesInfo {
        package: package.to_string(),
        depends,
        imports,
        suggests,
        linking_to,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_and_version() {
        let value = parse_input_line("GSVA 1.50.0 说明").expect("应解析包输入");
        assert_eq!(value.name, "GSVA");
        assert_eq!(value.version, "1.50.0");
    }

    #[test]
    fn extracts_github_repository_name() {
        assert_eq!(
            extract_package_name("https://github.com/buenrostrolab/FigR/"),
            "FigR"
        );
        assert_eq!(
            extract_package_name("https://example.org/src/contrib/demo.tgz"),
            "demo"
        );
        assert_eq!(
            extract_package_name("https://example.org/src/contrib/demo.package.zip"),
            "demo.package"
        );
    }

    #[test]
    fn infers_bioconductor_versions() {
        assert_eq!(infer_bioc_version(1, 50), Some(18));
        assert_eq!(infer_bioc_version(1, 34), Some(10));
        assert_eq!(infer_bioc_version(2, 2), Some(22));
    }

    #[test]
    fn generates_conditional_cran_command() {
        let output = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("应生成命令");
        assert!(output.contains("requireNamespace(\"dplyr\""));
        assert!(output.contains("dependencies = TRUE"));
    }

    #[test]
    fn auto_routes_explicit_github_repository_without_search_result() {
        let output = generate_script(
            "owner/demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("显式 GitHub 仓库应生成 GitHub 安装命令");

        assert!(output.contains("requireNamespace(\"demo\""));
        assert!(output.contains("remotes::install_github(\"owner/demo\""));
        assert!(!output.contains("install.packages(\"owner/demo\""));
    }

    #[test]
    fn check_system_uses_local_name_for_explicit_github_repository() {
        let output = generate_script(
            "owner/demo",
            &GenerateOptions {
                method: "checkSystem".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("系统检查应可处理显式 GitHub 仓库");

        assert!(output.contains("\"demo\""));
        assert!(output.contains("requireNamespace(p, quietly = TRUE)"));
        assert!(output.contains("library(p, character.only = TRUE)"));
        assert!(output.contains("未安装或加载报错"));
        assert!(!output.contains("\"owner/demo\""));
    }

    #[test]
    fn local_install_methods_use_local_name_for_explicit_github_repository() {
        for method in ["base", "version", "biocManager"] {
            let output = generate_script(
                "owner/demo",
                &GenerateOptions {
                    method: method.to_string(),
                    conditional: true,
                    install_dependencies: true,
                    mirror: "https://cloud.r-project.org".to_string(),
                    ..Default::default()
                },
                &[],
            )
            .expect("本地安装类方法应可规范化显式仓库名");

            assert!(output.contains("\"demo\""), "{method}");
            assert!(!output.contains("\"owner/demo\""), "{method}");
        }
    }

    #[test]
    fn install_url_condition_still_uses_archive_package_name() {
        let output = generate_script(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("归档 URL 条件安装应保留从文件名提取的包名");

        assert!(output.contains("requireNamespace(\"demo\""));
        assert!(output.contains(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\""
        ));
        assert!(!output.contains("requireNamespace(\"https://"));
    }

    #[test]
    fn auto_routes_archive_urls_per_input_line() {
        let output = generate_script(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz\nother",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "9.9.9".to_string(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("自动模式应按行处理安装归档 URL");

        assert!(output.contains(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\""
        ));
        assert!(output.contains("requireNamespace(\"demo\", quietly = TRUE)"));
        assert!(output.contains("install.packages(\"other\""));
        assert!(!output.contains("install_version(\"demo\""));
    }

    #[test]
    fn rejects_archive_urls_for_incompatible_methods() {
        for method in ["base", "version", "biocManager", "github"] {
            assert!(
                generate_script(
                    "https://example.org/src/contrib/demo_1.0.0.tar.gz",
                    &GenerateOptions {
                        method: method.to_string(),
                        conditional: false,
                        install_dependencies: true,
                        mirror: "https://cloud.r-project.org".to_string(),
                        ..Default::default()
                    },
                    &[],
                )
                .is_err(),
                "{method}"
            );
        }
    }

    #[test]
    fn test_generate_script_for_cran_archive() {
        let options = GenerateOptions {
            method: "auto".to_string(),
            conditional: false,
            install_dependencies: false,
            mirror: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/".to_string(),
            ..Default::default()
        };
        let results = vec![SearchResult {
            package: "oncoPredict".to_string(),
            requested_version: String::new(),
            latest_version: "0.2.0".to_string(),
            repository: "archive".to_string(),
            real_name: "oncoPredict".to_string(),
            source: "cran".to_string(),
            found: true,
            message: "在 Archive 归档区中找到".to_string(),
            status: "found".to_string(),
        }];

        let script = generate_script("oncoPredict", &options, &results).expect("生成脚本成功");

        assert!(script.contains("# [CRAN 已下架并归档: v0.2.0 | 自动同步]"));
        assert!(script.contains("remotes::install_version(\"oncoPredict\", version = \"0.2.0\", repos = \"https://cloud.r-project.org\", upgrade = \"never\", dependencies = FALSE)"));
    }

    #[test]
    fn builds_history_from_supported_conditional_command_body() {
        let script = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("应生成命令");

        let records = build_history_records(&script);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].package_name, "dplyr");
        assert_eq!(records[0].tool_name, "base R");
        assert!(records[0].command.starts_with("install.packages("));
    }

    #[test]
    fn rejects_unsupported_history_commands() {
        assert!(supported_history_command("system(\"calc.exe\")").is_none());
        assert!(supported_history_command("not_a_supported_command()").is_none());
        assert!(supported_history_command("install.packages(pkg)").is_none());
        assert!(supported_history_command(
            "install.packages(\"demo\", repos = \"http://example.com\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org\", dependencies = TRUE)"
        )
        .is_some());
        assert!(supported_history_command(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org?token=secret\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "remotes::install_version(\"demo\", version = \"1.2.3\", repos = \"https://cloud.r-project.org/\", upgrade = \"never\", dependencies = TRUE)"
        )
        .is_some());
        assert!(supported_history_command(
            "remotes::install_version(\"demo\", version = \"1.2.3\", repos = \"https://cloud.r-project.org/#cran\", upgrade = \"never\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\", dependencies = TRUE)"
        )
        .is_some());
        assert!(supported_history_command(
            "remotes::install_url(\"https://example.org:443/src/contrib/demo_1.0.0.tar.gz\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "remotes::install_url(\"https://github.com/owner/demo\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "devtools::install_url(\"https://example.org/demo_1.0.0.tar.gz?token=secret\", dependencies = TRUE)"
        )
        .is_none());
    }

    #[test]
    fn rejects_invalid_github_repository() {
        assert!(!is_valid_github_repository("owner/repo/extra"));
        assert!(!is_valid_github_repository("../repo"));
        assert!(!is_valid_github_repository("owner_name/repo"));
        assert!(!is_valid_github_repository("owner.name/repo"));
        assert!(!is_valid_github_repository("-owner/repo"));
        assert!(!is_valid_github_repository("owner-/repo"));
        assert!(!is_valid_github_repository(&format!(
            "{}/repo",
            "a".repeat(40)
        )));
        assert!(is_valid_github_repository("owner/repo.name"));
        assert!(is_valid_github_repository("owner-name/repo_name"));
        assert!(normalize_github_repository("https://github.com/owner.name/repo").is_none());
        assert!(normalize_github_repository("https://github.com:443/owner/repo").is_none());
    }

    #[test]
    fn rejects_oversized_input() {
        let input = "pkg\n".repeat(MAX_PACKAGE_LINES + 1);
        assert!(parse_inputs(&input).is_err());
    }

    #[test]
    fn ignores_comment_lines_when_counting_package_limit() {
        let input = format!(
            "{}\n{}",
            "# comment\n".repeat(MAX_PACKAGE_LINES + 10),
            "pkg\n".repeat(MAX_PACKAGE_LINES)
        );

        assert_eq!(
            parse_inputs(&input)
                .expect("注释行不应占用包数量限制")
                .len(),
            MAX_PACKAGE_LINES
        );
    }

    #[test]
    fn still_rejects_oversized_comment_lines() {
        let input = format!("# {}", "x".repeat(MAX_INPUT_LINE_BYTES));

        assert!(validate_input_size(&input).is_err());
    }

    #[test]
    fn rejects_oversized_or_controlled_input_before_parse() {
        let multibyte = "注".repeat((MAX_INPUT_CHARS / "注".len()) + 1);
        assert!(multibyte.chars().count() < MAX_INPUT_CHARS);
        assert!(multibyte.len() > MAX_INPUT_CHARS);
        assert!(validate_input_size(&multibyte).is_err());
        assert!(parse_inputs("demo\u{7f}\n").is_err());
        assert!(parse_inputs("demo\t1.2.3").is_ok());
    }

    #[test]
    fn rejects_oversized_input_line_before_parse() {
        let long_line = format!("demo {}", "1".repeat(MAX_INPUT_LINE_BYTES));
        assert!(validate_input_size(&long_line).is_err());

        let multibyte_line = format!(
            "demo {}",
            "注".repeat((MAX_INPUT_LINE_BYTES / "注".len()) + 1)
        );
        assert!(multibyte_line.chars().count() < MAX_INPUT_LINE_BYTES);
        assert!(multibyte_line.len() > MAX_INPUT_LINE_BYTES);
        assert!(parse_inputs(&multibyte_line).is_err());
    }

    #[test]
    fn rejects_oversized_requested_versions() {
        let input = format!("demo {}", "1".repeat(MAX_VERSION_CHARS + 1));

        assert!(parse_input_line(&input).is_none());
        assert!(parse_inputs(&input).is_err());
    }

    #[test]
    fn bounds_result_message_by_utf8_bytes() {
        let message = clean_result_text(&"注".repeat(MAX_RESULT_MESSAGE_CHARS));

        assert!(message.len() <= MAX_RESULT_MESSAGE_CHARS);
        assert!(message.ends_with('注'));
    }

    #[test]
    fn validates_browser_search_url_scope() {
        assert!(is_allowed_browser_search_url(
            "https://www.google.com/search?q=R%20package%20GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "http://www.google.com/search?q=GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://example.com/search?q=GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://www.google.com/search?q=GSVA&source=desktop"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://www.google.com/preferences?q=GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://www.google.com/search?q=GSVA#frag"
        ));
    }

    #[test]
    fn rejects_oversized_history_script() {
        let script = "install.packages(\"demo\")\n".repeat((MAX_SCRIPT_CHARS / 25) + 10);
        assert!(build_history_records(&script).is_empty());
        assert!(validate_script_size(&script).is_err());
    }

    #[test]
    fn rejects_oversized_multibyte_script_by_bytes() {
        let script = "注".repeat((MAX_SCRIPT_CHARS / "注".len()) + 1);
        assert!(script.chars().count() < MAX_SCRIPT_CHARS);
        assert!(script.len() > MAX_SCRIPT_CHARS);
        assert!(validate_script_size(&script).is_err());
        assert!(build_history_records(&script).is_empty());
        assert!(clean_script(&script).is_err());
    }

    #[test]
    fn cleans_script_and_rejects_oversized_cleaned_output() {
        let cleaned = clean_script("# comment\n\ninstall.packages(\"demo\")\n")
            .expect("普通脚本应可清理注释");
        assert_eq!(cleaned, "install.packages(\"demo\")");

        let script = "x\n".repeat((MAX_SCRIPT_CHARS / 3) + 2);
        assert!(validate_script_size(&script).is_ok());
        assert!(clean_script(&script).is_err());
    }

    #[test]
    fn bounds_history_scan_lines() {
        let script = format!(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org/\", dependencies = TRUE)\n{}",
            "not_a_supported_command()\n".repeat(MAX_HISTORY_SCAN_LINES + 1)
        );

        assert!(build_history_records(&script).is_empty());

        let script = format!(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org/\", dependencies = TRUE)\n{}",
            "# ignored\n\n".repeat(MAX_HISTORY_SCAN_LINES + 10)
        );

        assert_eq!(build_history_records(&script).len(), 1);
    }

    #[test]
    fn rejects_oversized_generated_script() {
        let input = (0..MAX_PACKAGE_LINES)
            .map(|index| format!("package{index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let oversized_mirror = format!("https://{}.example.org/CRAN/", "a".repeat(1900));

        let result = generate_script(
            &input,
            &GenerateOptions {
                method: "base".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: oversized_mirror,
                ..Default::default()
            },
            &[],
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_generate_method_without_echoing_value() {
        let method = format!("bad{}\n{}", "x".repeat(128), "system(\"calc.exe\")");
        let error = generate_script(
            "demo",
            &GenerateOptions {
                method: method.clone(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect_err("非法安装方式应被拒绝");

        assert_eq!(error, "安装方式无效");
        assert!(!error.contains(&method));
        assert!(!error.contains("calc.exe"));
    }

    #[test]
    fn rejects_unbounded_generate_search_results() {
        let results = (0..=MAX_GENERATE_SEARCH_RESULTS)
            .map(|index| SearchResult {
                package: format!("demo{index}"),
                requested_version: String::new(),
                latest_version: "1.0.0".to_string(),
                repository: String::new(),
                real_name: format!("demo{index}"),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            })
            .collect::<Vec<_>>();

        let error = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &results,
        )
        .expect_err("超出上限的检索结果应被拒绝");

        assert!(error.contains("检索结果数量过多"));
    }

    #[test]
    fn uses_search_results_across_the_full_accepted_range() {
        let mut results = vec![
            SearchResult {
                package: "other".to_string(),
                requested_version: String::new(),
                latest_version: String::new(),
                repository: String::new(),
                real_name: "other".to_string(),
                source: "none".to_string(),
                found: false,
                message: "未找到".to_string(),
                status: "found".to_string(),
            };
            MAX_GENERATE_SEARCH_RESULTS - 1
        ];
        results.push(SearchResult {
            package: "target".to_string(),
            requested_version: String::new(),
            latest_version: "9.9.9".to_string(),
            repository: String::new(),
            real_name: "target".to_string(),
            source: "cran".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
        });

        let output = generate_script(
            "target",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &results,
        )
        .expect("允许范围末端的合法检索结果应参与脚本生成");

        assert!(output.contains("remotes::install_version(\"target\", version = \"9.9.9\""));
    }

    #[test]
    fn rejects_unsafe_install_url_inputs() {
        assert!(parse_input_line("https://example.org/src/contrib/demo_1.0.0.tar.gz").is_some());
        assert!(parse_input_line("demo https://example.org/pkg_1.0.tar.gz").is_none());
        assert!(parse_input_line("https://user:pass@example.com/pkg_1.0.tar.gz").is_none());
        assert!(parse_input_line("https://example.org:443/pkg_1.0.tar.gz").is_none());
        assert!(parse_input_line("http://example.com/pkg_1.0.tar.gz").is_none());
        assert!(parse_input_line("ftp://example.com/pkg_1.0.tar.gz").is_none());
        assert!(parse_input_line("https://github.com/owner/demo").is_none());
        assert!(parse_input_line("https://example.com/pkg_1.0.tar.gz?token=secret").is_none());
        assert!(parse_input_line("https://example.com/pkg_1.0.tar.gz#section").is_none());
        assert!(parse_input_line("https://example.com/index.html").is_none());

        let output = generate_script(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("合法安装归档 URL 应可生成 install_url 命令");
        assert!(output.contains(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\""
        ));

        assert!(generate_script(
            "https://user:pass@example.com/pkg_1.0.tar.gz",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "https://github.com/owner/demo",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "demo https://example.org/pkg_1.0.tar.gz",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "demo",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "http://cran.example.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "demo",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org?token=secret".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
    }

    #[test]
    fn rejects_untrusted_search_results_for_auto_script() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: "https://example.com/github.com/evil/demo".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("非法检索结果应被忽略并回退基础安装");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains("evil/demo"));
    }

    #[test]
    fn ignores_inconsistent_found_search_results_for_auto_script() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "9.9.9".to_string(),
                    repository: String::new(),
                    real_name: "demo".to_string(),
                    source: "none".to_string(),
                    found: true,
                    message: "伪造成功".to_string(),
                    status: "found".to_string(),
                },
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "8.8.8".to_string(),
                    repository: String::new(),
                    real_name: "demo".to_string(),
                    source: "github".to_string(),
                    found: true,
                    message: "缺少仓库".to_string(),
                    status: "found".to_string(),
                },
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "7.7.7".to_string(),
                    repository: "owner/demo".to_string(),
                    real_name: "demo\nbad".to_string(),
                    source: "github".to_string(),
                    found: true,
                    message: "非法真实包名".to_string(),
                    status: "found".to_string(),
                },
            ],
        )
        .expect("矛盾检索结果应被忽略并回退基础安装");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains("9.9.9"));
        assert!(!output.contains("8.8.8"));
        assert!(!output.contains("7.7.7"));
        assert!(!output.contains("owner/demo"));
    }

    #[test]
    fn ignores_search_results_without_real_name_or_repository_match() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: "owner/not-demo".to_string(),
                real_name: "not-demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("身份不匹配的检索结果应被忽略");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains("owner/not-demo"));
    }

    #[test]
    fn prefers_case_insensitive_real_name_matches() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "1.0.0".to_string(),
                    repository: "https://github.com/other/demo".to_string(),
                    real_name: "not-demo".to_string(),
                    source: "github".to_string(),
                    found: true,
                    message: "验证成功".to_string(),
                    status: "found".to_string(),
                },
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "2.0.0".to_string(),
                    repository: String::new(),
                    real_name: "Demo".to_string(),
                    source: "cran".to_string(),
                    found: true,
                    message: "验证成功".to_string(),
                    status: "found".to_string(),
                },
            ],
        )
        .expect("大小写差异的真实包名应参与优先排序");

        assert!(output.contains("remotes::install_version(\"demo\", version = \"2.0.0\""));
        assert!(!output.contains("install_github"));
    }

    #[test]
    fn accepts_sanitized_search_results_for_auto_script() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: " demo ".to_string(),
                requested_version: String::new(),
                latest_version: " 1.2.3 ".to_string(),
                repository: "https://github.com/owner/demo.git".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("合法检索结果应可参与自动路由");

        assert!(output.contains("remotes::install_github(\"owner/demo\""));
        assert!(!output.contains("https://github.com/owner/demo.git"));
    }

    #[test]
    fn hides_remote_versions_without_losing_source_routing() {
        let options = GenerateOptions {
            method: "auto".to_string(),
            conditional: true,
            install_dependencies: true,
            mirror: "https://cloud.r-project.org".to_string(),
            ..Default::default()
        };
        let cran_output = generate_script_with_remote_versions(
            "demo",
            &options,
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
            false,
        )
        .expect("隐藏远程版本时 CRAN 来源仍应生成脚本");

        assert!(cran_output.contains("install.packages(\"demo\""));
        assert!(!cran_output.contains("install_version"));
        assert!(!cran_output.contains("1.2.3"));

        let github_output = generate_script_with_remote_versions(
            "demo",
            &options,
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "2.0.0".to_string(),
                repository: "owner/demo".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
            false,
        )
        .expect("隐藏远程版本时 GitHub 来源路由仍应保留");

        assert!(github_output.contains("remotes::install_github(\"owner/demo\""));
        assert!(!github_output.contains("2.0.0"));
    }

    #[test]
    fn rejects_invalid_bioc_git_package_names() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: "3.18".to_string(),
                real_name: "demo".to_string(),
                source: "biocGit".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("合法 Bioconductor 历史版本结果应可生成安装命令");
        assert!(output.contains("https://git.bioconductor.org/packages/demo"));

        let error = generate_command(
            "owner/demo",
            "biocGit",
            "1.2.3|3.18",
            false,
            "https://cloud.r-project.org/",
            true,
        )
        .expect_err("仓库式路径不应进入 Bioconductor Git URL");

        assert!(error.contains("不是有效的 Bioconductor 包名"));
    }

    #[test]
    fn rejects_result_versions_with_control_characters() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3\nInjected".to_string(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("非法版本检索结果应被忽略");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_version"));
        assert!(!output.contains("Injected"));
    }

    #[test]
    fn ignores_search_results_with_oversized_versions() {
        let oversized_version = "1".repeat(MAX_VERSION_CHARS + 1);
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: oversized_version.clone(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("超长版本检索结果应被忽略");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_version"));
        assert!(!output.contains(&oversized_version));
    }

    #[test]
    fn ignores_search_results_with_oversized_fields() {
        let huge_repository = format!("https://github.com/owner/{}", "a".repeat(MAX_FIELD_CHARS));
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: huge_repository.clone(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("超大字段结果应被忽略并回退基础安装");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains(&huge_repository));
    }

    #[test]
    fn generate_script_github_with_version_ref() {
        let output = generate_script(
            "demo 1.2.3",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: "1.2.3".to_string(),
                latest_version: "1.2.3".to_string(),
                repository: "owner/demo".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
            }],
        )
        .expect("GitHub 智能路由带版本结果应可生成带有 ref 的 install_github");

        assert!(output.contains("remotes::install_github(\"owner/demo@v1.2.3\""));
    }

    #[test]
    fn parses_comma_separated_quoted_packages() {
        let packages = parse_inputs_filtered(
            "\"ChIPseeker\", \"clusterProfiler\", \"TxDb.Hsapiens.UCSC.hg38.knownGene\"",
            &InputRules::default(),
        )
        .expect("逗号分隔引用包名应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "ChIPseeker");
        assert_eq!(packages[1].name, "clusterProfiler");
        assert_eq!(packages[2].name, "TxDb.Hsapiens.UCSC.hg38.knownGene");
    }

    #[test]
    fn parses_semicolon_separated_packages() {
        let packages =
            parse_inputs_filtered("dplyr; ggplot2; tidyr; shiny", &InputRules::default())
                .expect("分号分隔包名应可解析");
        assert_eq!(packages.len(), 4);
        assert_eq!(packages[0].name, "dplyr");
        assert_eq!(packages[2].name, "tidyr");
    }

    #[test]
    fn parses_r_c_vector_syntax() {
        let packages = parse_inputs_filtered(
            "c(\"Seurat\", \"dplyr\", \"ggplot2\")",
            &InputRules::default(),
        )
        .expect("R c() 向量应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "Seurat");
        assert_eq!(packages[1].name, "dplyr");
        assert_eq!(packages[2].name, "ggplot2");
    }

    #[test]
    fn parses_mixed_separator_lines() {
        let packages =
            parse_inputs_filtered("pkg1; pkg2, pkg3\npkg4, pkg5", &InputRules::default())
                .expect("混合分隔符多行输入应可解析");
        assert_eq!(packages.len(), 5);
        assert_eq!(packages[0].name, "pkg1");
        assert_eq!(packages[3].name, "pkg4");
    }

    #[test]
    fn parses_comma_separated_with_versions() {
        let packages =
            parse_inputs_filtered("GSVA 1.50.0, dplyr 1.0.0, ggplot2", &InputRules::default())
                .expect("逗号分隔带版本应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "GSVA");
        assert_eq!(packages[0].version, "1.50.0");
        assert_eq!(packages[1].name, "dplyr");
        assert_eq!(packages[1].version, "1.0.0");
        assert_eq!(packages[2].name, "ggplot2");
        assert_eq!(packages[2].version, "");
    }

    #[test]
    fn parses_list_variant_syntax() {
        let packages = parse_inputs_filtered("list(\"pkg1\", \"pkg2\")", &InputRules::default())
            .expect("list() 包裹应可解析");
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "pkg1");
        assert_eq!(packages[1].name, "pkg2");
    }

    #[test]
    fn parses_url_lines_bypass_separator_splitting() {
        let packages = parse_inputs_filtered(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz",
            &InputRules::default(),
        )
        .expect("URL 行应保持完整");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "demo");
    }

    #[test]
    fn test_strip_r_parens_wrapper() {
        assert_eq!(
            strip_r_parens_wrapper("c(\"pkg1\", \"pkg2\")"),
            "\"pkg1\", \"pkg2\""
        );
        assert_eq!(strip_r_parens_wrapper("list(\"pkg1\")"), "\"pkg1\"");
        assert_eq!(strip_r_parens_wrapper("plain_line"), "plain_line");
    }

    #[test]
    fn parses_space_separated_with_split_spaces_enabled() {
        let rules = InputRules {
            split_spaces: true,
            separators: Vec::new(),
            ..InputRules::default()
        };
        let packages = parse_inputs_filtered("pkg1 pkg2 pkg3", &rules)
            .expect("空格分隔 (split_spaces=true) 应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "pkg1");
        assert_eq!(packages[2].name, "pkg3");
    }

    #[test]
    fn parses_complex_real_world_input() {
        let input = "c(\"ChIPseeker\", \"clusterProfiler\", \"TxDb.Hsapiens.UCSC.hg38.knownGene\", \"org.Hs.eg.db\", \"enrichplot\")";
        let packages = parse_inputs_filtered(input, &InputRules::default())
            .expect("真实世界 R c() 输入应可解析");
        assert_eq!(packages.len(), 5);
        assert_eq!(packages[0].name, "ChIPseeker");
        assert_eq!(packages[4].name, "enrichplot");
    }

    #[test]
    fn parses_reverse_dependencies_from_cran_html() {
        let html = r#"<table summary="Reverse depends for dplyr">
<tr><td>Reverse depends:</td><td><a href="../.../">250</a></td></tr>
<tr><td>Reverse imports:</td><td><a href="../.../">1856</a></td></tr>
<tr><td>Reverse suggests:</td><td><a href="../.../">125</a></td></tr>
<tr><td>Reverse linking to:</td><td><a href="../.../">42</a></td></tr>
</table>"#;

        let info = parse_reverse_dependencies(html, "dplyr").expect("应可解析反向依赖");
        assert_eq!(info.depends, 250);
        assert_eq!(info.imports, 1856);
        assert_eq!(info.suggests, 125);
        assert_eq!(info.linking_to, 42);
    }

    #[test]
    fn parses_reverse_dependencies_partial_fields() {
        let html =
            r##"<table><tr><td>Reverse imports:</td><td><a href="#">3</a></td></tr></table>"##;
        let info = parse_reverse_dependencies(html, "pkg").expect("部分字段应可解析");
        assert_eq!(info.imports, 3);
        assert_eq!(info.depends, 0);
        assert_eq!(info.suggests, 0);
        assert_eq!(info.linking_to, 0);
    }

    #[test]
    fn reverse_dependencies_returns_none_for_no_match() {
        assert!(parse_reverse_dependencies("no reverse data here", "pkg").is_none());
    }

    #[test]
    fn generates_verify_script_with_correct_package_names() {
        let packages = vec![
            PackageInput {
                raw: "dplyr".to_string(),
                name: "dplyr".to_string(),
                version: String::new(),
                source_hint: None,
            },
            PackageInput {
                raw: "owner/demo".to_string(),
                name: "owner/demo".to_string(),
                version: String::new(),
                source_hint: None,
            },
        ];
        let verify = generate_verify_script(&packages);
        assert!(verify.contains("\"dplyr\""));
        assert!(verify.contains("\"demo\""));
        assert!(!verify.contains("owner/demo"));
        assert!(verify.contains("packageVersion(p)"));
        assert!(verify.contains("[OK]"));
        assert!(verify.contains("[FAIL]"));
        assert!(verify.contains("验证完成"));
    }

    #[test]
    fn append_verify_in_generated_script() {
        let output = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                append_verify: true,
            },
            &[],
        )
        .expect("应生成带验证的脚本");

        assert!(output.contains("install.packages(\"dplyr\""));
        assert!(output.contains("# ===== 安装结果验证 ====="));
        assert!(output.contains("packageVersion(p)"));
    }
}
