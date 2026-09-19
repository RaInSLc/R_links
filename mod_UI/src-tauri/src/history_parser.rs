use crate::models::{
    normalize_cran_mirror_url, HistoryRecord, MAX_HISTORY_COMMAND_CHARS, MAX_HISTORY_RECORDS,
    MAX_SCRIPT_CHARS,
};
use regex::Regex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
const MAX_HISTORY_SCAN_LINES: usize = MAX_HISTORY_RECORDS;
const MAX_VERSION_CHARS: usize = 64;
/// 进程内自增序号，保证同一毫秒内多次构建也不会产生重复 id。
static HISTORY_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static HISTORY_VERSION_RE: OnceLock<Regex> = OnceLock::new();
static BASE_HISTORY_RE: OnceLock<[Regex; 4]> = OnceLock::new();
static INSTALL_URL_HISTORY_RE: OnceLock<Regex> = OnceLock::new();
static CRAN_HISTORY_RE: OnceLock<[Regex; 2]> = OnceLock::new();
use crate::logic::*;

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
                id: format!(
                    "{now}-{}-{index}",
                    HISTORY_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ),
                command,
                package_name,
                version,
                tool_name,
                created_at: now.to_string(),
                input: String::new(),
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: false,
                show_remote_version: false,
                verify_install: false,
                cran_mirror: String::new(),
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
    } else if command.contains("R-Forge.R-project.org") {
        "R-Forge"
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

pub(crate) fn looks_like_supported_history_command(command: &str) -> bool {
    matches!(
        command.as_bytes().first(),
        Some(b'B' | b'd' | b'i' | b'p' | b'r')
    ) && (command.starts_with("BiocManager::install(")
        || command.starts_with("devtools::install_url(")
        || command.starts_with("install.packages(")
        || command.starts_with("packageVersion(")
        || command.starts_with("remotes::install_"))
}

pub(crate) fn supported_install_url_history_command(command: &str) -> bool {
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

pub(crate) fn supported_cran_history_command(command: &str) -> bool {
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
                .is_some_and(|mirror| {
                    normalize_cran_mirror_url(mirror.as_str()).is_ok()
                        || mirror.as_str() == "http://R-Forge.R-project.org"
                })
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

pub(crate) fn source_label(source: &str) -> &str {
    match source {
        "cran" => "CRAN",
        "cran-binary" => "R 二进制镜像",
        "bioc" => "Bioconductor",
        "biocGit" => "Bioconductor 历史版本",
        "github" => "GitHub",
        "r-forge" => "R-Forge",
        _ => "未知来源",
    }
}

pub(crate) fn is_clean_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= MAX_VERSION_CHARS
        && version
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, '.' | '-'))
}
