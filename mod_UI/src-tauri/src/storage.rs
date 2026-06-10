use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use crate::logic;
use crate::models::{
    HistoryRecord, Settings, MAX_FIELD_CHARS, MAX_HISTORY_COMMAND_CHARS, MAX_HISTORY_RECORDS,
    MAX_TOKEN_CHARS,
};
use crate::secrets;
use serde::{Deserialize, Serialize};

const MAX_PROTECTED_TOKEN_CHARS: usize = MAX_TOKEN_CHARS * 16;
const MAX_HISTORY_SAVE_RECORDS: usize = MAX_HISTORY_RECORDS * 4;
const MAX_HISTORY_SAVE_BYTES: usize = 1024 * 1024;
const MAX_HISTORY_LOAD_SCAN_RECORDS: usize = MAX_HISTORY_RECORDS * 20;
const MAX_HISTORY_ID_CHARS: usize = 64;
const MAX_HISTORY_VERSION_CHARS: usize = 64;
const MAX_HISTORY_TIMESTAMP_CHARS: usize = 32;
const MAX_SETTINGS_FILE_BYTES: u64 = 64 * 1024;
const MAX_HISTORY_FILE_BYTES: u64 = MAX_HISTORY_SAVE_BYTES as u64;
const MAX_CORRUPT_BACKUPS_PER_FILE: usize = 5;
const MAX_TEMP_FILE_CREATE_ATTEMPTS: usize = 8;
const OVERSIZED_BACKUP_NOTICE: &str = "原文件超过安全读取上限，内容未复制到备份。";
static STORAGE_WRITE_LOCK: Mutex<()> = Mutex::new(());
static STORAGE_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    proxy: String,
    cran_mirror: String,
    full_search: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    github_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    github_token_protected: String,
}

impl StoredSettings {
    fn into_settings(self) -> Result<Settings, String> {
        if self.github_token_protected.len() > MAX_PROTECTED_TOKEN_CHARS {
            return Err("加密 Token 长度超过限制".to_string());
        }
        let github_token = if self.github_token_protected.trim().is_empty() {
            self.github_token
        } else {
            secrets::unprotect_string(&self.github_token_protected)?
        };
        Settings {
            proxy: self.proxy,
            github_token,
            cran_mirror: self.cran_mirror,
            full_search: self.full_search,
        }
        .normalized()
    }

    fn from_settings(settings: &Settings) -> Result<Self, String> {
        let settings = settings.normalized()?;
        Ok(Self {
            proxy: settings.proxy,
            github_token: String::new(),
            github_token_protected: secrets::protect_string(&settings.github_token)?,
            cran_mirror: settings.cran_mirror,
            full_search: settings.full_search,
        })
    }
}

fn data_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join(name))
}

pub fn load_settings(app: &AppHandle) -> Result<Settings, String> {
    load_settings_with_recovery(app, true)
}

pub fn load_existing_settings(app: &AppHandle) -> Result<Option<Settings>, String> {
    let path = data_file(app, "settings.json")?;
    if !path_entry_exists(&path)? {
        return Ok(None);
    }
    load_settings_with_recovery(app, false).map(Some)
}

fn load_settings_with_recovery(
    app: &AppHandle,
    fallback_to_default: bool,
) -> Result<Settings, String> {
    let path = data_file(app, "settings.json")?;
    if !path_entry_exists(&path)? {
        return Ok(Settings::default());
    }
    let Some(content) =
        read_storage_file_with_recovery(app, "settings.json", MAX_SETTINGS_FILE_BYTES, "设置文件")?
    else {
        return if fallback_to_default {
            Ok(Settings::default())
        } else {
            Err("设置文件超过安全读取上限，已备份；请重新确认设置后再保存".to_string())
        };
    };
    match serde_json::from_str::<StoredSettings>(&content).and_then(|settings| {
        settings.into_settings().map_err(|error| {
            serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
    }) {
        Ok(settings) => Ok(settings),
        Err(_) => {
            backup_corrupt_settings_file(app, &content)?;
            if fallback_to_default {
                Ok(Settings::default())
            } else {
                Err("设置文件损坏，已备份；请重新确认设置后再保存".to_string())
            }
        }
    }
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = data_file(app, "settings.json")?;
    let settings = StoredSettings::from_settings(settings)?;
    let content = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    atomic_write(&path, &content)
}

pub fn load_history(app: &AppHandle) -> Result<Vec<HistoryRecord>, String> {
    let path = data_file(app, "history.json")?;
    if !path_entry_exists(&path)? {
        return Ok(Vec::new());
    }
    let Some(content) =
        read_storage_file_with_recovery(app, "history.json", MAX_HISTORY_FILE_BYTES, "历史文件")?
    else {
        return Ok(Vec::new());
    };
    match serde_json::from_str::<Vec<HistoryRecord>>(&content) {
        Ok(history) => Ok(sanitize_history(&history)),
        Err(_) => {
            backup_corrupt_file(app, "history.json", &content)?;
            Ok(Vec::new())
        }
    }
}

pub fn save_history(
    app: &AppHandle,
    history: &[HistoryRecord],
) -> Result<Vec<HistoryRecord>, String> {
    let path = data_file(app, "history.json")?;
    save_history_to_path(&path, history)
}

fn save_history_to_path(
    path: &Path,
    history: &[HistoryRecord],
) -> Result<Vec<HistoryRecord>, String> {
    validate_history_save_payload(history)?;
    let limited = sanitize_history(history);
    let content = serde_json::to_string_pretty(&limited).map_err(|error| error.to_string())?;
    atomic_write(path, &content)?;
    Ok(limited)
}

fn validate_history_save_payload(history: &[HistoryRecord]) -> Result<(), String> {
    if history.len() > MAX_HISTORY_SAVE_RECORDS {
        return Err(format!(
            "历史记录数量过多，最多允许 {MAX_HISTORY_SAVE_RECORDS} 条"
        ));
    }

    let mut total_bytes = 0usize;
    for record in history {
        total_bytes = total_bytes
            .saturating_add(validate_history_save_field(
                "历史记录 ID",
                &record.id,
                MAX_HISTORY_ID_CHARS,
            )?)
            .saturating_add(validate_history_save_field(
                "历史记录命令",
                &record.command,
                MAX_HISTORY_COMMAND_CHARS,
            )?)
            .saturating_add(validate_history_save_field(
                "历史记录包名",
                &record.package_name,
                MAX_FIELD_CHARS,
            )?)
            .saturating_add(validate_history_save_field(
                "历史记录版本",
                &record.version,
                MAX_HISTORY_VERSION_CHARS,
            )?)
            .saturating_add(validate_history_save_field(
                "历史记录工具名",
                &record.tool_name,
                MAX_FIELD_CHARS,
            )?)
            .saturating_add(validate_history_save_field(
                "历史记录时间戳",
                &record.created_at,
                MAX_HISTORY_TIMESTAMP_CHARS,
            )?);
        if total_bytes > MAX_HISTORY_SAVE_BYTES {
            return Err(format!(
                "历史记录总大小过大，最多允许 {MAX_HISTORY_SAVE_BYTES} 字节"
            ));
        }
    }
    Ok(())
}

fn validate_history_save_field(
    label: &str,
    value: &str,
    max_bytes: usize,
) -> Result<usize, String> {
    let length = value.len();
    if length > max_bytes {
        return Err(format!("{label}长度过长，最多允许 {max_bytes} 字节"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{label}包含非法控制字符"));
    }
    Ok(length)
}

fn sanitize_history(history: &[HistoryRecord]) -> Vec<HistoryRecord> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    history
        .iter()
        .take(MAX_HISTORY_LOAD_SCAN_RECORDS)
        .enumerate()
        .filter_map(|(index, record)| sanitize_history_record(record, index, now))
        .take(MAX_HISTORY_RECORDS)
        .collect()
}

fn sanitize_history_record(
    record: &HistoryRecord,
    index: usize,
    now: u128,
) -> Option<HistoryRecord> {
    let command = logic::supported_history_command(&record.command)?;
    let (package_name, version, tool_name) = logic::history_metadata_from_command(&command)?;
    let id = if is_clean_history_id(&record.id) {
        record.id.clone()
    } else {
        format!("{now}-{index}")
    };
    let created_at = if is_clean_timestamp(&record.created_at) {
        record.created_at.clone()
    } else {
        now.to_string()
    };

    Some(HistoryRecord {
        id,
        command,
        package_name,
        version,
        tool_name,
        created_at,
    })
}

fn is_clean_history_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HISTORY_ID_CHARS
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn is_clean_timestamp(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HISTORY_TIMESTAMP_CHARS
        && value.chars().all(|character| character.is_ascii_digit())
}

fn read_limited_to_string(
    path: &Path,
    max_bytes: u64,
    file_label: &str,
) -> Result<Option<String>, String> {
    let file = open_storage_file(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!("{file_label}不是普通文件"));
    }
    if metadata.len() > max_bytes {
        return Ok(None);
    }

    let mut reader = file.take(max_bytes + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Ok(None);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{file_label}不是有效 UTF-8"))
}

fn open_storage_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn path_entry_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn storage_target_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err("存储目标不是普通文件".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn read_storage_file_with_recovery(
    app: &AppHandle,
    name: &str,
    max_bytes: u64,
    file_label: &str,
) -> Result<Option<String>, String> {
    let path = data_file(app, name)?;
    let directory = path.parent().ok_or_else(|| "存储目录无效".to_string())?;
    read_storage_path_with_recovery(directory, &path, name, max_bytes, file_label)
}

fn read_storage_path_with_recovery(
    directory: &Path,
    path: &Path,
    name: &str,
    max_bytes: u64,
    file_label: &str,
) -> Result<Option<String>, String> {
    match read_limited_to_string(path, max_bytes, file_label) {
        Ok(Some(content)) => Ok(Some(content)),
        Ok(None) => {
            backup_corrupt_storage_path(directory, name, OVERSIZED_BACKUP_NOTICE)?;
            Ok(None)
        }
        Err(error) => {
            backup_corrupt_storage_path(directory, name, &error)?;
            Ok(None)
        }
    }
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let _guard = STORAGE_WRITE_LOCK
        .lock()
        .map_err(|_| "存储写入锁已损坏".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "存储文件名无效".to_string())?;
    storage_target_exists(path)?;

    let tmp = create_synced_temp_file(path, file_name, content)?;
    if let Err(error) = replace_storage_file(path, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

fn create_synced_temp_file(path: &Path, file_name: &str, content: &str) -> Result<PathBuf, String> {
    for _ in 0..MAX_TEMP_FILE_CREATE_ATTEMPTS {
        let tmp = path.with_file_name(format!("{file_name}.{}.tmp", unique_file_suffix()));
        match write_synced_new_file(&tmp, content) {
            Ok(()) => return Ok(tmp),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("无法创建唯一的存储临时文件".to_string())
}

fn write_synced_new_file(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(error) = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn replace_storage_file(path: &Path, tmp: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };

    fn wide_path(path: &Path) -> Result<Vec<u16>, String> {
        let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if wide.contains(&0) {
            return Err("存储路径包含非法空字符".to_string());
        }
        wide.push(0);
        Ok(wide)
    }

    let target = wide_path(path)?;
    let replacement = wide_path(tmp)?;
    let replaced = if storage_target_exists(path)? {
        // ReplaceFileW 在一个文件系统操作中替换现有目标，避免正式文件短暂缺失。
        unsafe {
            ReplaceFileW(
                target.as_ptr(),
                replacement.as_ptr(),
                null(),
                REPLACEFILE_WRITE_THROUGH,
                null(),
                null(),
            )
        }
    } else {
        unsafe {
            MoveFileExW(
                replacement.as_ptr(),
                target.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_storage_file(path: &Path, tmp: &Path) -> Result<(), String> {
    storage_target_exists(path)?;
    fs::rename(tmp, path).map_err(|error| error.to_string())
}

fn backup_corrupt_settings_file(app: &AppHandle, content: &str) -> Result<(), String> {
    let redacted = redact_settings_backup_content(content);
    backup_corrupt_file(app, "settings.json", &redacted)
}

fn backup_corrupt_storage_path(directory: &Path, name: &str, content: &str) -> Result<(), String> {
    if name == "settings.json" {
        backup_corrupt_path(directory, name, &redact_settings_backup_content(content))
    } else {
        backup_corrupt_path(directory, name, content)
    }
}

fn backup_corrupt_file(app: &AppHandle, name: &str, content: &str) -> Result<(), String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    backup_corrupt_path(&directory, name, content)
}

fn backup_corrupt_path(directory: &Path, name: &str, content: &str) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let backup = directory.join(format!("{name}.corrupt.{}.bak", unique_file_suffix()));
    atomic_write(&backup, content)?;
    prune_corrupt_backups(directory, name);
    Ok(())
}

fn prune_corrupt_backups(directory: &Path, name: &str) {
    let prefix = format!("{name}.corrupt.");
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut backups = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if !file_name.starts_with(&prefix) || !file_name.ends_with(".bak") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((
                metadata.modified().unwrap_or(UNIX_EPOCH),
                file_name,
                entry.path(),
            ))
        })
        .collect::<Vec<_>>();

    if backups.len() <= MAX_CORRUPT_BACKUPS_PER_FILE {
        return;
    }
    backups.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let delete_count = backups.len() - MAX_CORRUPT_BACKUPS_PER_FILE;
    for (_, _, path) in backups.into_iter().take(delete_count) {
        let _ = fs::remove_file(path);
    }
}

fn unique_file_suffix() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = STORAGE_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{stamp}.{counter}")
}

fn redact_settings_backup_content(content: &str) -> String {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content) {
        redact_settings_value(&mut value);
        return serde_json::to_string_pretty(&value)
            .unwrap_or_else(|_| OVERSIZED_BACKUP_NOTICE.to_string());
    }

    ["githubTokenProtected", "githubToken", "proxy"]
        .into_iter()
        .fold(content.to_string(), redact_json_string_field)
}

fn redact_settings_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "githubToken" || key == "githubTokenProtected" || key == "proxy" {
                    *child = serde_json::Value::String("[redacted]".to_string());
                } else {
                    redact_settings_value(child);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_settings_value(item);
            }
        }
        _ => {}
    }
}

fn redact_json_string_field(content: String, field: &str) -> String {
    let key = format!("\"{field}\"");
    let mut output = String::with_capacity(content.len());
    let mut cursor = 0;

    while let Some(relative_start) = content[cursor..].find(&key) {
        let start = cursor + relative_start;
        output.push_str(&content[cursor..start]);
        output.push_str(&key);

        let after_key = start + key.len();
        let Some((value_start, value_end)) = find_json_string_value_span(&content, after_key)
        else {
            cursor = after_key;
            continue;
        };

        output.push_str(&content[after_key..value_start]);
        output.push_str("\"[redacted]\"");
        cursor = value_end;
    }

    output.push_str(&content[cursor..]);
    output
}

fn find_json_string_value_span(content: &str, after_key: usize) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut index = after_key;
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'"') {
        return None;
    }

    let value_start = index;
    index += 1;
    let mut escaped = false;
    while let Some(byte) = bytes.get(index) {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            return Some((value_start, index + 1));
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_settings_do_not_serialize_plain_token() {
        let settings = Settings {
            github_token: "secret-token".to_string(),
            ..Settings::default()
        };
        let stored = StoredSettings::from_settings(&settings).expect("应能保护 Token");
        let content = serde_json::to_string(&stored).expect("应能序列化设置");
        assert!(!content.contains("secret-token"));
        assert!(!content.contains("githubToken\":\""));
        assert!(content.contains("githubTokenProtected"));
        assert_eq!(stored.into_settings().unwrap().github_token, "secret-token");
    }

    #[test]
    fn stored_settings_read_legacy_plain_token() {
        let content = r#"{
            "proxy": "",
            "githubToken": "legacy-token",
            "cranMirror": "https://cloud.r-project.org",
            "fullSearch": false
        }"#;
        let settings = serde_json::from_str::<StoredSettings>(content)
            .expect("旧设置应可解析")
            .into_settings()
            .expect("旧 Token 应可迁移读取");
        assert_eq!(settings.github_token, "legacy-token");
    }

    #[test]
    fn stored_settings_reject_invalid_protected_token() {
        let content = r#"{
            "proxy": "",
            "githubTokenProtected": "dpapi:not-valid-base64",
            "cranMirror": "https://cloud.r-project.org",
            "fullSearch": false
        }"#;
        assert!(serde_json::from_str::<StoredSettings>(content)
            .expect("格式本身应可解析")
            .into_settings()
            .is_err());
    }

    #[test]
    fn redacts_sensitive_settings_backup_fields() {
        let content = r#"{
            "proxy": "http://user:pass@127.0.0.1:7890",
            "githubToken": "legacy-secret",
            "githubTokenProtected": "dpapi:encrypted-secret",
            "cranMirror": "https://cloud.r-project.org",
            "fullSearch": false
        }"#;

        let redacted = redact_settings_backup_content(content);

        assert!(!redacted.contains("legacy-secret"));
        assert!(!redacted.contains("dpapi:encrypted-secret"));
        assert!(!redacted.contains("user:pass"));
        assert!(redacted.contains("\"githubToken\": \"[redacted]\""));
        assert!(redacted.contains("\"githubTokenProtected\": \"[redacted]\""));
        assert!(redacted.contains("\"proxy\": \"[redacted]\""));
    }

    #[test]
    fn redacts_sensitive_fields_from_malformed_settings_backup() {
        let content = r#"{"proxy":"http://user:pass@127.0.0.1:7890","githubToken":"legacy-secret","githubTokenProtected":"dpapi:encrypted-secret","broken":true"#;

        let redacted = redact_settings_backup_content(content);

        assert!(!redacted.contains("legacy-secret"));
        assert!(!redacted.contains("dpapi:encrypted-secret"));
        assert!(!redacted.contains("user:pass"));
        assert!(redacted.contains("\"githubToken\":\"[redacted]\""));
        assert!(redacted.contains("\"githubTokenProtected\":\"[redacted]\""));
        assert!(redacted.contains("\"proxy\":\"[redacted]\""));
    }

    #[test]
    fn unique_file_suffix_changes_between_calls() {
        assert_ne!(unique_file_suffix(), unique_file_suffix());
    }

    #[test]
    fn atomic_write_does_not_use_shared_tmp_name() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-storage-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("settings.json");

        atomic_write(&path, "first").expect("首次写入应成功");
        atomic_write(&path, "second").expect("第二次写入应成功");

        assert_eq!(fs::read_to_string(&path).expect("应能读取文件"), "second");
        assert!(!path.with_extension("tmp").exists());
        let leftovers = fs::read_dir(&directory)
            .expect("应能列出目录")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn synced_temp_write_refuses_to_overwrite_existing_file() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-exclusive-temp-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("settings.json.fixed.tmp");
        fs::write(&path, "existing").expect("应能预置临时文件");

        let error =
            write_synced_new_file(&path, "replacement").expect_err("独占临时写入不得覆盖已有文件");

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read_to_string(&path).expect("应能读取预置临时文件"),
            "existing"
        );
        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn atomic_replace_failure_keeps_existing_target() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-replace-fail-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("settings.json");
        let missing_tmp = directory.join("missing.tmp");
        fs::write(&path, "existing").expect("应能预置正式文件");

        assert!(replace_storage_file(&path, &missing_tmp).is_err());
        assert_eq!(
            fs::read_to_string(&path).expect("替换失败后正式文件应仍可读取"),
            "existing"
        );
        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn atomic_write_cleans_tmp_when_initial_rename_fails() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-storage-fail-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("missing").join("settings.json");

        assert!(atomic_write(&path, "content").is_err());
        let leftovers = fs::read_dir(&directory)
            .expect("应能列出临时目录")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn atomic_write_rejects_non_file_target_without_tmp() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-storage-non-file-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("settings.json");
        fs::create_dir(&path).expect("应能创建同名目录");

        assert!(atomic_write(&path, "content").is_err());
        assert!(path.is_dir());
        let leftovers = fs::read_dir(&directory)
            .expect("应能列出临时目录")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn read_limited_to_string_rejects_invalid_utf8() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-invalid-utf8-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("settings.json");
        fs::write(&path, [b'{', 0xff, b'}']).expect("应能写入非法 UTF-8 内容");

        let error = read_limited_to_string(&path, MAX_SETTINGS_FILE_BYTES, "设置文件")
            .expect_err("非法 UTF-8 应被拒绝");

        assert_eq!(error, "设置文件不是有效 UTF-8");

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[cfg(windows)]
    #[test]
    fn read_limited_to_string_rejects_symbolic_links() {
        use std::os::windows::fs::symlink_file;

        let directory =
            std::env::temp_dir().join(format!("mod-ui-read-link-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let target = directory.join("target.json");
        let link = directory.join("settings.json");
        fs::write(&target, "{}").expect("应能写入链接目标");
        symlink_file(&target, &link).expect("应能创建文件符号链接");

        assert!(read_limited_to_string(&link, MAX_SETTINGS_FILE_BYTES, "设置文件").is_err());

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[cfg(windows)]
    #[test]
    fn atomic_write_rejects_dangling_symbolic_links() {
        use std::os::windows::fs::symlink_file;

        let directory =
            std::env::temp_dir().join(format!("mod-ui-dangling-link-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let missing_target = directory.join("missing.json");
        let link = directory.join("settings.json");
        symlink_file(&missing_target, &link).expect("应能创建断开的文件符号链接");

        assert!(!link.exists());
        assert!(path_entry_exists(&link).expect("断链目录项应可识别"));
        assert!(atomic_write(&link, "replacement").is_err());
        assert!(fs::symlink_metadata(&link)
            .expect("断链目录项应保持存在")
            .file_type()
            .is_symlink());
        let leftovers = fs::read_dir(&directory)
            .expect("应能列出临时目录")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);

        fs::remove_file(&link).expect("应能移除断链");
        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn storage_recovery_backs_up_invalid_utf8_settings() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-invalid-settings-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("settings.json");
        fs::write(&path, [b'{', 0xff, b'}']).expect("应能写入非法 UTF-8 设置");

        let content = read_storage_path_with_recovery(
            &directory,
            &path,
            "settings.json",
            MAX_SETTINGS_FILE_BYTES,
            "设置文件",
        )
        .expect("非法 UTF-8 设置应进入恢复路径");

        assert!(content.is_none());
        let backup = fs::read_dir(&directory)
            .expect("应能列出备份目录")
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.corrupt.")
            })
            .expect("应写入设置损坏备份");
        let backup_content = fs::read_to_string(backup.path()).expect("应能读取设置备份");

        assert_eq!(backup_content, "设置文件不是有效 UTF-8");

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn storage_recovery_backs_up_invalid_utf8_history() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-invalid-history-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("history.json");
        fs::write(&path, [b'[', 0xff, b']']).expect("应能写入非法 UTF-8 历史");

        let content = read_storage_path_with_recovery(
            &directory,
            &path,
            "history.json",
            MAX_HISTORY_FILE_BYTES,
            "历史文件",
        )
        .expect("非法 UTF-8 历史应进入恢复路径");

        assert!(content.is_none());
        let backup = fs::read_dir(&directory)
            .expect("应能列出备份目录")
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("history.json.corrupt.")
            })
            .expect("应写入历史损坏备份");

        assert_eq!(
            fs::read_to_string(backup.path()).expect("应能读取历史备份"),
            "历史文件不是有效 UTF-8"
        );

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn atomic_write_replaces_existing_corrupt_backup_without_tmp() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-corrupt-write-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("settings.json.corrupt.demo.bak");
        fs::write(&path, "old").expect("应能写入旧备份");

        atomic_write(&path, "new").expect("应能覆盖旧备份");

        assert_eq!(fs::read_to_string(&path).expect("应能读取备份"), "new");
        let leftovers = fs::read_dir(&directory)
            .expect("应能列出临时目录")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn prune_corrupt_backups_keeps_recent_limit_per_file() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-corrupt-backups-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");

        for index in 0..(MAX_CORRUPT_BACKUPS_PER_FILE + 2) {
            fs::write(
                directory.join(format!("settings.json.corrupt.{index:02}.bak")),
                "bad-settings",
            )
            .expect("应能写入腐坏备份");
        }
        fs::write(directory.join("history.json.corrupt.00.bak"), "bad-history")
            .expect("应能写入其他备份");
        fs::write(
            directory.join("settings.json.corrupt.keep.txt"),
            "not-a-backup",
        )
        .expect("应能写入非备份文件");
        fs::create_dir(directory.join("settings.json.corrupt.directory.bak"))
            .expect("应能创建同名目录");

        prune_corrupt_backups(&directory, "settings.json");

        assert!(!directory.join("settings.json.corrupt.00.bak").exists());
        assert!(!directory.join("settings.json.corrupt.01.bak").exists());
        for index in 2..(MAX_CORRUPT_BACKUPS_PER_FILE + 2) {
            assert!(directory
                .join(format!("settings.json.corrupt.{index:02}.bak"))
                .exists());
        }
        assert!(directory.join("history.json.corrupt.00.bak").exists());
        assert!(directory.join("settings.json.corrupt.keep.txt").exists());
        assert!(directory
            .join("settings.json.corrupt.directory.bak")
            .is_dir());

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn sanitize_history_rejects_unsupported_commands() {
        let records = vec![HistoryRecord {
            id: "1".to_string(),
            command: "system(\"calc.exe\")".to_string(),
            package_name: "demo".to_string(),
            version: String::new(),
            tool_name: "base R".to_string(),
            created_at: "1".to_string(),
        }];
        let history = sanitize_history(&records);

        assert!(history.is_empty());
    }

    #[test]
    fn sanitize_history_recomputes_frontend_metadata() {
        let records = vec![HistoryRecord {
            id: "history-1".to_string(),
            command:
                "remotes::install_github(\"owner/demo\", upgrade = \"never\", dependencies = TRUE)"
                    .to_string(),
            package_name: "forged".to_string(),
            version: "9.9.9".to_string(),
            tool_name: "forged".to_string(),
            created_at: "123456".to_string(),
        }];
        let history = sanitize_history(&records);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].package_name, "demo");
        assert_eq!(history[0].version, "");
        assert_eq!(history[0].tool_name, "GitHub");
        assert_eq!(history[0].created_at, "123456");
    }

    #[test]
    fn sanitize_history_bounds_invalid_record_scan_window() {
        let mut records = vec![
            HistoryRecord {
                id: "bad".to_string(),
                command: "system(\"calc.exe\")".to_string(),
                package_name: "demo".to_string(),
                version: String::new(),
                tool_name: "base R".to_string(),
                created_at: "1".to_string(),
            };
            MAX_HISTORY_LOAD_SCAN_RECORDS
        ];
        records.push(HistoryRecord {
            id: "history-valid".to_string(),
            command:
                "remotes::install_github(\"owner/demo\", upgrade = \"never\", dependencies = TRUE)"
                    .to_string(),
            package_name: "demo".to_string(),
            version: String::new(),
            tool_name: "GitHub".to_string(),
            created_at: "1".to_string(),
        });

        let history = sanitize_history(&records);

        assert!(history.is_empty());
    }

    #[test]
    fn save_history_returns_sanitized_written_records() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-history-save-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");
        let path = directory.join("history.json");
        let history = vec![HistoryRecord {
            id: "bad id".to_string(),
            command:
                "remotes::install_github(\"owner/demo\", upgrade = \"never\", dependencies = TRUE)"
                    .to_string(),
            package_name: "forged".to_string(),
            version: "9.9.9".to_string(),
            tool_name: "forged".to_string(),
            created_at: "bad-time".to_string(),
        }];

        let saved = save_history_to_path(&path, &history).expect("历史应可保存");
        let written = serde_json::from_str::<Vec<HistoryRecord>>(
            &fs::read_to_string(&path).expect("应能读取历史文件"),
        )
        .expect("写入历史应可解析");

        assert_eq!(written, saved);
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].package_name, "demo");
        assert_ne!(saved[0].id, "bad id");
        assert_ne!(saved[0].created_at, "bad-time");

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn rejects_unbounded_history_save_payload() {
        let record = HistoryRecord {
            id: "history-1".to_string(),
            command:
                "remotes::install_github(\"owner/demo\", upgrade = \"never\", dependencies = TRUE)"
                    .to_string(),
            package_name: "demo".to_string(),
            version: String::new(),
            tool_name: "GitHub".to_string(),
            created_at: "1".to_string(),
        };
        let bounded = vec![record.clone(); MAX_HISTORY_SAVE_RECORDS];
        let unbounded = vec![record; MAX_HISTORY_SAVE_RECORDS + 1];

        assert!(validate_history_save_payload(&bounded).is_ok());
        assert!(validate_history_save_payload(&unbounded).is_err());
    }

    #[test]
    fn rejects_oversized_history_save_fields() {
        let history = vec![HistoryRecord {
            id: "history-1".to_string(),
            command: "x".repeat(MAX_HISTORY_COMMAND_CHARS + 1),
            package_name: "demo".to_string(),
            version: String::new(),
            tool_name: "GitHub".to_string(),
            created_at: "1".to_string(),
        }];

        let error =
            validate_history_save_payload(&history).expect_err("超长历史命令应在清洗前被拒绝");

        assert!(error.contains("历史记录命令长度过长"));
    }

    #[test]
    fn rejects_history_save_fields_with_control_characters() {
        let history = vec![HistoryRecord {
            id: "history-1".to_string(),
            command:
                "remotes::install_github(\"owner/demo\", upgrade = \"never\", dependencies = TRUE)"
                    .to_string(),
            package_name: "demo\nbad".to_string(),
            version: String::new(),
            tool_name: "GitHub".to_string(),
            created_at: "1".to_string(),
        }];

        let error =
            validate_history_save_payload(&history).expect_err("控制字符字段应在清洗前被拒绝");

        assert!(error.contains("历史记录包名包含非法控制字符"));
    }

    #[test]
    fn rejects_history_save_payload_total_bytes() {
        let history = (0..MAX_HISTORY_SAVE_RECORDS)
            .map(|index| HistoryRecord {
                id: format!("history-{index}"),
                command: format!(
                    "install.packages(\"demo{index}\", repos = \"https://cloud.r-project.org/\", dependencies = TRUE)"
                ),
                package_name: "p".repeat(MAX_FIELD_CHARS),
                version: "1.0.0".to_string(),
                tool_name: "t".repeat(MAX_FIELD_CHARS),
                created_at: "1".to_string(),
            })
            .collect::<Vec<_>>();

        let error = validate_history_save_payload(&history)
            .expect_err("总大小过大的历史保存 payload 应被拒绝");

        assert!(error.contains("历史记录总大小过大"));
    }
}
