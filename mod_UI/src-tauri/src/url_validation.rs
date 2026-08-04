use crate::models::{
    url_has_explicit_port, ReverseDependenciesInfo, MAX_FIELD_CHARS, MAX_INPUT_CHARS,
    MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
};
use regex::Regex;
use url::Url;
const MAX_INSTALL_ARCHIVE_FILE_CHARS: usize = 256;
const MAX_INPUT_LINE_BYTES: usize = 2_048;
const INSTALL_ARCHIVE_EXTENSIONS: &[&str] = &[".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".zip"];
static REVERSE_DEPS_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

pub(crate) fn normalize_install_archive_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_FIELD_CHARS
        || trimmed.chars().any(|character| character.is_control())
    {
        return Err("安装 URL 包含非法字符或长度过长".to_string());
    }
    let parsed = Url::parse(trimmed).map_err(|_| "安装 URL 必须是有效 URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("安装 URL 仅支持 http 或 https".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("安装 URL 缺少主机名".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("安装 URL 不允许包含用户名或密码".to_string());
    }
    let normalized = parsed.to_string();
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

pub(crate) fn normalize_local_archive_path(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_FIELD_CHARS
        || trimmed.chars().any(|character| character.is_control())
    {
        return Err("本地归档路径无效".to_string());
    }
    let path = std::path::Path::new(trimmed);
    if !path.is_absolute() {
        return Err("本地归档路径必须是绝对路径".to_string());
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "本地归档文件名无效".to_string())?;
    let lower = file_name.to_ascii_lowercase();
    if !INSTALL_ARCHIVE_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return Err("本地文件必须是 R 包归档格式".to_string());
    }
    Ok(trimmed.to_string())
}

pub(crate) fn package_name_from_archive_file(file_name: &str) -> Option<String> {
    let lower_file_name = file_name.to_ascii_lowercase();
    INSTALL_ARCHIVE_EXTENSIONS
        .iter()
        .find(|extension| lower_file_name.ends_with(**extension))
        .and_then(|extension| file_name.get(..file_name.len().saturating_sub(extension.len())))
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn escape_r(value: &str) -> String {
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

pub(crate) fn local_package_name(value: &str) -> String {
    normalize_github_repository(value)
        .and_then(|repository| repository.rsplit('/').next().map(ToString::to_string))
        .unwrap_or_else(|| value.to_string())
}

pub fn is_valid_github_repository(value: &str) -> bool {
    if value.len() > 200 || value.contains('\\') || value.contains("..") {
        return false;
    }
    let parts = value.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() < 2 {
        return false;
    }
    if !is_valid_github_owner_segment(parts[0]) || !is_valid_github_repo_segment(parts[1]) {
        return false;
    }
    parts[2..]
        .iter()
        .all(|part| is_valid_github_repo_segment(part))
}

pub(crate) fn is_valid_github_owner_segment(value: &str) -> bool {
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

pub(crate) fn is_valid_github_repo_segment(value: &str) -> bool {
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

pub(crate) fn github_repository_from_url(value: &str) -> Option<String> {
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

pub fn build_package_page_url(
    package: &str,
    source: &str,
    repository: &str,
) -> Result<String, String> {
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
        "r-forge" => {
            if !is_valid_package_name(package) {
                return Err(format!("无效的 R-Forge 包名: {package}"));
            }
            Ok(format!("https://r-forge.r-project.org/projects/{package}/"))
        }
        "pip" => {
            if !is_valid_package_name(package) {
                return Err(format!("无效的 PyPI 包名: {package}"));
            }
            Ok(format!("https://pypi.org/project/{package}/"))
        }
        "conda" => {
            let channel = repository.trim();
            if !is_valid_package_name(package) {
                return Err(format!("无效的 Conda 包名: {package}"));
            }
            if channel.is_empty()
                || !channel
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err("无效的 Conda channel".to_string());
            }
            Ok(format!("https://anaconda.org/{channel}/{package}"))
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
            segs.len() == 2 && is_valid_github_repository(&format!("{}/{}", segs[0], segs[1]))
        }
        Some("r-forge.r-project.org") => {
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            segs.len() == 2 && segs[0] == "projects" && is_valid_package_name(segs[1])
        }
        Some("pypi.org") => {
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            segs.len() == 2 && segs[0] == "project" && is_valid_package_name(segs[1])
        }
        Some("anaconda.org") => {
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            segs.len() == 2
                && segs[0]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && is_valid_package_name(segs[1])
        }
        _ => false,
    }
}

pub(crate) fn is_valid_bioc_version(value: &str) -> bool {
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
