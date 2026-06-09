use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::models::{
    normalize_cran_mirror_url, normalize_https_url, url_has_explicit_port, GenerateOptions,
    HistoryRecord, PackageInput, SearchResult, MAX_FIELD_CHARS, MAX_HISTORY_COMMAND_CHARS,
    MAX_HISTORY_RECORDS, MAX_INPUT_CHARS, MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
};

const MAX_GENERATE_METHOD_CHARS: usize = 32;
const MAX_GENERATE_SEARCH_RESULTS: usize = MAX_PACKAGE_LINES * 16;
const MAX_VERSION_CHARS: usize = 64;
const MAX_RESULT_SOURCE_CHARS: usize = 16;
const MAX_RESULT_MESSAGE_CHARS: usize = 512;
const MAX_INSTALL_ARCHIVE_FILE_CHARS: usize = 256;
const MAX_HISTORY_SCAN_LINES: usize = MAX_HISTORY_RECORDS * 20;
const INSTALL_ARCHIVE_EXTENSIONS: &[&str] = &[".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".zip"];

static INPUT_URL_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_PACKAGE_RE: OnceLock<Regex> = OnceLock::new();
static INPUT_VERSION_RE: OnceLock<Regex> = OnceLock::new();
static QUOTED_VALUE_RE: OnceLock<Regex> = OnceLock::new();
static HISTORY_VERSION_RE: OnceLock<Regex> = OnceLock::new();
static BASE_HISTORY_RE: OnceLock<[Regex; 4]> = OnceLock::new();
static INSTALL_URL_HISTORY_RE: OnceLock<Regex> = OnceLock::new();
static CRAN_HISTORY_RE: OnceLock<[Regex; 2]> = OnceLock::new();

pub fn parse_inputs(input: &str) -> Result<Vec<PackageInput>, String> {
    validate_input_size(input)?;

    let mut packages = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let package = parse_input_line(trimmed)
            .ok_or_else(|| format!("第 {} 行输入格式无效或包含不允许的字符", index + 1))?;
        packages.push(package);
        if packages.len() > MAX_PACKAGE_LINES {
            return Err(format!("单次最多处理 {MAX_PACKAGE_LINES} 行输入"));
        }
    }
    Ok(packages)
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
    let version_re = INPUT_VERSION_RE
        .get_or_init(|| Regex::new(r"([0-9]+[0-9A-Za-z.\-]*)").expect("固定版本正则必须有效"));
    let version = version_re
        .captures(remaining)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
        .unwrap_or_default();
    if !version.is_empty() && !is_clean_version(&version) {
        return None;
    }

    Some(PackageInput {
        raw: raw.to_string(),
        name,
        version,
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

pub fn generate_script(
    input: &str,
    options: &GenerateOptions,
    results: &[SearchResult],
) -> Result<String, String> {
    let requested_method = normalize_generate_method(&options.method)?;
    validate_search_results_count(results)?;
    let packages = parse_inputs(input)?;
    if packages.is_empty() {
        return Ok("等待输入...".to_string());
    }
    let results = sanitize_search_results(results);

    let mirror = if options.mirror.trim().is_empty() {
        "https://cloud.r-project.org".to_string()
    } else {
        normalize_cran_mirror_url(&options.mirror)?
    };

    if requested_method == "checkSystem" {
        let names = packages
            .iter()
            .map(|item| format!("\"{}\"", escape_r(&local_package_name(&item.name))))
            .collect::<Vec<_>>()
            .join(", ");
        return Ok(format!(
            "# 1. 定义需要检查的包列表\r\npackages_to_check <- c(\r\n  {names}\r\n)\r\n\r\n# 2. 获取系统中已安装的包\r\ninstalled_pkgs <- installed.packages()[, \"Package\"]\r\n\r\n# 3. 计算缺失包\r\nmissing_pkgs <- packages_to_check[!(packages_to_check %in% installed_pkgs)]\r\n\r\n# 4. 输出结果\r\nif (length(missing_pkgs) == 0) {{\r\n  cat(\"所有包均已安装。\\n\")\r\n}} else {{\r\n  cat(\"以下包尚未安装：\\n\")\r\n  print(missing_pkgs)\r\n}}\r\n"
        ));
    }

    let mut output = Vec::new();
    for package in packages {
        let mut value = if matches!(requested_method, "devtools" | "remotes") {
            package.raw.clone()
        } else {
            package.name.clone()
        };
        let mut version = package.version.clone();
        let mut method = requested_method.to_string();

        if let Some(best) = choose_best_result(&package.name, &results) {
            let source_label = source_label(&best.source);
            if version.is_empty() {
                output.push(format!(
                    "# [{source_label} 已验证: v{} | 自动同步]",
                    best.latest_version
                ));
                if is_clean_version(&best.latest_version) {
                    version = best.latest_version.clone();
                }
            } else {
                output.push(format!(
                    "# [{source_label} 最新版本: v{} | 保留指定版本]",
                    best.latest_version
                ));
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
                        version = format!("{}|{}", best.latest_version, best.repository);
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
        } else if results
            .iter()
            .any(|result| result.package.eq_ignore_ascii_case(&package.name))
        {
            output.push("# [提示: CRAN/Bioconductor/GitHub 均未找到]".to_string());
        }

        if requested_method == "auto"
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

        output.push(generate_command(
            &value,
            &method,
            &version,
            options.conditional,
            &mirror,
            options.install_dependencies,
        )?);
    }

    let script = output.join("\r\n") + "\r\n";
    validate_script_size(&script)?;
    Ok(script)
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

fn choose_best_result<'a>(package: &str, results: &'a [SearchResult]) -> Option<&'a SearchResult> {
    let mut candidates = results
        .iter()
        .filter(|result| result.found && result.package.eq_ignore_ascii_case(package))
        .filter(|result| result_identity_matches_package(result, package))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|result| {
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
        (strict_name, source, if exact_repo { 0 } else { 1 })
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
        .take(MAX_PACKAGE_LINES * 4)
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
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect()
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
            effective_version.clear();
            format!(
                "remotes::install_github(\"{}\", upgrade = \"never\", dependencies = {dependencies})",
                escape_r(&repository)
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
        return Err(format!("输入内容过长，最多允许 {MAX_INPUT_CHARS} 个字符"));
    }
    let line_count = input.lines().filter(|line| !line.trim().is_empty()).count();
    if line_count > MAX_PACKAGE_LINES {
        return Err(format!("单次最多处理 {MAX_PACKAGE_LINES} 行输入"));
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

fn is_valid_bioc_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
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
            },
            &[],
        )
        .expect("系统检查应可处理显式 GitHub 仓库");

        assert!(output.contains("\"demo\""));
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
    fn builds_history_from_supported_conditional_command_body() {
        let script = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
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
    fn rejects_oversized_requested_versions() {
        let input = format!("demo {}", "1".repeat(MAX_VERSION_CHARS + 1));

        assert!(parse_input_line(&input).is_none());
        assert!(parse_inputs(&input).is_err());
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
            "{}\ninstall.packages(\"demo\", repos = \"https://cloud.r-project.org/\", dependencies = TRUE)",
            "not_a_supported_command()\n".repeat(MAX_HISTORY_SCAN_LINES + 1)
        );

        assert!(build_history_records(&script).is_empty());

        let script = format!(
            "{}\ninstall.packages(\"demo\", repos = \"https://cloud.r-project.org/\", dependencies = TRUE)",
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
            })
            .collect::<Vec<_>>();

        let error = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
            },
            &results,
        )
        .expect_err("超出上限的检索结果应被拒绝");

        assert!(error.contains("检索结果数量过多"));
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
            }],
        )
        .expect("合法检索结果应可参与自动路由");

        assert!(output.contains("remotes::install_github(\"owner/demo\""));
        assert!(!output.contains("https://github.com/owner/demo.git"));
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
            }],
        )
        .expect("超大字段结果应被忽略并回退基础安装");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains(&huge_repository));
    }
}
