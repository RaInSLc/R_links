use std::collections::HashMap;
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
    HistoryRecord, InputRules, PackageCacheEntry, Settings, INPUT_RULES_FILE_NAME, MAX_FIELD_CHARS,
    MAX_HISTORY_COMMAND_CHARS, MAX_HISTORY_RECORDS, MAX_TOKEN_CHARS,
};
use crate::secrets;
use serde::{Deserialize, Serialize};

const MAX_PROTECTED_TOKEN_CHARS: usize = MAX_TOKEN_CHARS * 16;
const MAX_HISTORY_SAVE_RECORDS: usize = MAX_HISTORY_RECORDS * 4;
const MAX_HISTORY_SAVE_BYTES: usize = 10 * 1024 * 1024;
const MAX_HISTORY_LOAD_SCAN_RECORDS: usize = MAX_HISTORY_RECORDS * 20;
const MAX_HISTORY_ID_CHARS: usize = 64;
const MAX_HISTORY_VERSION_CHARS: usize = 64;
const MAX_HISTORY_TIMESTAMP_CHARS: usize = 32;
const MAX_SETTINGS_FILE_BYTES: u64 = 64 * 1024;
const MAX_HISTORY_FILE_BYTES: u64 = MAX_HISTORY_SAVE_BYTES as u64;
const MAX_CORRUPT_BACKUPS_PER_FILE: usize = 5;
const MAX_CORRUPT_BACKUP_SCAN_ENTRIES: usize = 512;
const MAX_TEMP_FILE_CREATE_ATTEMPTS: usize = 8;
const OVERSIZED_BACKUP_NOTICE: &str = "原文件超过安全读取上限，内容未复制到备份。";
const MALFORMED_SETTINGS_BACKUP_NOTICE: &str = "设置文件格式损坏，原始内容未写入备份。";
static STORAGE_WRITE_LOCK: Mutex<()> = Mutex::new(());
static STORAGE_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_max_cache_entries() -> usize {
    1000
}

fn default_max_dependency_depth() -> usize {
    2
}

fn default_max_dependency_nodes() -> usize {
    100
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    proxy: String,
    cran_mirror: String,
    full_search: bool,
    #[serde(default = "default_true")]
    conditional: bool,
    #[serde(default = "default_true")]
    install_dependencies: bool,
    #[serde(default = "default_true")]
    show_remote_version: bool,
    #[serde(default = "default_true")]
    use_cache: bool,
    #[serde(default = "default_max_cache_entries")]
    max_cache_entries: usize,
    #[serde(default = "default_true")]
    use_filter: bool,
    #[serde(default = "default_true")]
    resolve_dependencies: bool,
    #[serde(default = "default_max_dependency_depth")]
    max_dependency_depth: usize,
    #[serde(default = "default_false")]
    include_light_dependencies: bool,
    #[serde(default = "default_max_dependency_nodes")]
    max_dependency_nodes: usize,
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
            conditional: self.conditional,
            install_dependencies: self.install_dependencies,
            show_remote_version: self.show_remote_version,
            use_cache: self.use_cache,
            max_cache_entries: self.max_cache_entries,
            use_filter: self.use_filter,
            resolve_dependencies: self.resolve_dependencies,
            max_dependency_depth: self.max_dependency_depth,
            include_light_dependencies: self.include_light_dependencies,
            max_dependency_nodes: self.max_dependency_nodes,
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
            conditional: settings.conditional,
            install_dependencies: settings.install_dependencies,
            show_remote_version: settings.show_remote_version,
            use_cache: settings.use_cache,
            max_cache_entries: settings.max_cache_entries,
            use_filter: settings.use_filter,
            resolve_dependencies: settings.resolve_dependencies,
            max_dependency_depth: settings.max_dependency_depth,
            include_light_dependencies: settings.include_light_dependencies,
            max_dependency_nodes: settings.max_dependency_nodes,
        })
    }
}

pub fn data_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let directory = ensure_data_directory(app)?;
    Ok(directory.join(name))
}

pub fn ensure_data_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    ensure_storage_directory(&directory)?;
    Ok(directory)
}

fn ensure_storage_directory(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let metadata = fs::symlink_metadata(directory).map_err(|error| error.to_string())?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata_is_windows_reparse_point(&metadata)
    {
        return Err("应用数据目录不是普通目录".to_string());
    }
    Ok(())
}

pub fn load_input_rules(app: &AppHandle) -> InputRules {
    let path = match data_file(app, INPUT_RULES_FILE_NAME) {
        Ok(p) => p,
        Err(_) => return InputRules::default(),
    };
    if !path_entry_exists(&path).unwrap_or(false) {
        return InputRules::default();
    }
    let Some(content) =
        read_storage_file_with_recovery(app, INPUT_RULES_FILE_NAME, 64 * 1024, "输入规则文件")
            .unwrap_or(None)
    else {
        return InputRules::default();
    };
    match serde_json::from_str::<InputRules>(&content) {
        Ok(rules) => rules.normalized(),
        Err(_) => {
            let _ = backup_corrupt_file(app, INPUT_RULES_FILE_NAME, &content);
            InputRules::default()
        }
    }
}

pub fn save_default_input_rules(app: &AppHandle) {
    let path = match data_file(app, INPUT_RULES_FILE_NAME) {
        Ok(p) => p,
        Err(_) => return,
    };
    if path.exists() {
        return;
    }
    let rules = InputRules::default();
    if let Ok(content) = serde_json::to_string_pretty(&rules) {
        let _ = atomic_write(&path, &content);
    }
}

pub fn save_input_rules(app: &AppHandle, rules: &InputRules) -> Result<(), String> {
    let path = data_file(app, INPUT_RULES_FILE_NAME)?;
    let normalized = rules.normalized();
    let content = serde_json::to_string_pretty(&normalized).map_err(|error| error.to_string())?;
    atomic_write(&path, &content)
}

#[cfg(windows)]
fn metadata_is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_windows_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
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
    backup_corrupt_path(directory, name, content)
}

fn backup_corrupt_file(app: &AppHandle, name: &str, content: &str) -> Result<(), String> {
    let directory = ensure_data_directory(app)?;
    backup_corrupt_path(&directory, name, content)
}

fn backup_corrupt_path(directory: &Path, name: &str, content: &str) -> Result<(), String> {
    ensure_storage_directory(directory)?;
    let saturated = corrupt_backup_directory_saturated(directory)?;
    let backup = if saturated {
        directory.join(format!("{name}.corrupt.overflow.bak"))
    } else {
        directory.join(format!("{name}.corrupt.{}.bak", unique_file_suffix()))
    };
    atomic_write(&backup, content)?;
    if !saturated {
        prune_corrupt_backups(directory, name);
    }
    Ok(())
}

fn corrupt_backup_directory_saturated(directory: &Path) -> Result<bool, String> {
    let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;
    let mut count = 0usize;
    for entry in entries.take(MAX_CORRUPT_BACKUP_SCAN_ENTRIES) {
        entry.map_err(|error| error.to_string())?;
        count += 1;
        if count >= MAX_CORRUPT_BACKUP_SCAN_ENTRIES {
            return Ok(true);
        }
    }
    Ok(false)
}

fn prune_corrupt_backups(directory: &Path, name: &str) -> usize {
    let prefix = format!("{name}.corrupt.");
    let Ok(entries) = fs::read_dir(directory) else {
        return 0;
    };
    let mut scanned = 0usize;
    let mut recent = Vec::with_capacity(MAX_CORRUPT_BACKUPS_PER_FILE);
    for entry in entries.take(MAX_CORRUPT_BACKUP_SCAN_ENTRIES) {
        scanned += 1;
        let Ok(entry) = entry else {
            continue;
        };
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.starts_with(&prefix) || !file_name.ends_with(".bak") {
            continue;
        }
        if !entry
            .file_type()
            .ok()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        let candidate = (modified, file_name, entry.path());
        if recent.len() < MAX_CORRUPT_BACKUPS_PER_FILE {
            recent.push(candidate);
            continue;
        }

        let Some((oldest_index, oldest)) =
            recent.iter().enumerate().min_by(|(_, left), (_, right)| {
                left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1))
            })
        else {
            continue;
        };
        if candidate
            .0
            .cmp(&oldest.0)
            .then_with(|| candidate.1.cmp(&oldest.1))
            .is_gt()
        {
            let old_path = std::mem::replace(&mut recent[oldest_index], candidate).2;
            let _ = fs::remove_file(old_path);
        } else {
            let _ = fs::remove_file(candidate.2);
        }
    }
    scanned
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
            .unwrap_or_else(|_| MALFORMED_SETTINGS_BACKUP_NOTICE.to_string());
    }
    MALFORMED_SETTINGS_BACKUP_NOTICE.to_string()
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

const CACHE_FILE_NAME: &str = "pkg_cache.json";

pub fn load_cache(app: &AppHandle) -> Result<HashMap<String, PackageCacheEntry>, String> {
    let path = data_file(app, CACHE_FILE_NAME)?;
    if !path_entry_exists(&path)? {
        return Ok(HashMap::new());
    }
    let Some(content) =
        read_storage_file_with_recovery(app, CACHE_FILE_NAME, 1024 * 1024, "包缓存文件")?
    else {
        return Ok(HashMap::new());
    };
    match serde_json::from_str::<Vec<PackageCacheEntry>>(&content) {
        Ok(entries) => {
            let mut cache = HashMap::new();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let limit = load_existing_settings(app)
                .ok()
                .flatten()
                .map(|s| s.max_cache_entries)
                .unwrap_or(1000);
            for entry in entries.into_iter().take(limit) {
                let key = entry.package_name.to_ascii_lowercase();
                if !key.is_empty() && !entry.source.is_empty() {
                    // 如果是已在 CRAN 下架的包 oncoPredict 且缓存为普通 CRAN 包，强制跳过以触发重新检索
                    if key == "oncopredict" && entry.source == "cran" && entry.repository.is_empty()
                    {
                        continue;
                    }
                    cache.insert(key, entry);
                }
            }
            // 自动清理超过 7 天的旧缓存
            cache.retain(|_, entry| {
                entry
                    .cached_at
                    .parse::<u64>()
                    .map(|ts| now.saturating_sub(ts) < 7 * 24 * 3600)
                    .unwrap_or(false)
            });
            Ok(cache)
        }
        Err(_) => {
            backup_corrupt_file(app, CACHE_FILE_NAME, &content)?;
            Ok(HashMap::new())
        }
    }
}

pub fn save_cache(
    app: &AppHandle,
    cache: &HashMap<String, PackageCacheEntry>,
) -> Result<(), String> {
    let limit = load_existing_settings(app)
        .ok()
        .flatten()
        .map(|s| s.max_cache_entries)
        .unwrap_or(1000);
    let path = data_file(app, CACHE_FILE_NAME)?;
    let entries = sorted_cache_entries(cache, limit);
    let content = serde_json::to_string_pretty(&entries).map_err(|error| error.to_string())?;
    atomic_write(&path, &content)
}

fn sorted_cache_entries(
    cache: &HashMap<String, PackageCacheEntry>,
    limit: usize,
) -> Vec<&PackageCacheEntry> {
    let mut entries: Vec<(u64, String, &PackageCacheEntry)> = cache
        .values()
        .map(|entry| {
            (
                entry.cached_at.parse::<u64>().unwrap_or_default(),
                entry.package_name.to_ascii_lowercase(),
                entry,
            )
        })
        .collect();
    entries.sort_by(|(left_time, left_name, _), (right_time, right_name, _)| {
        right_time
            .cmp(left_time)
            .then_with(|| left_name.cmp(right_name))
    });
    entries.truncate(limit);
    entries
        .into_iter()
        .map(|(_, _, entry)| entry)
        .collect::<Vec<_>>()
}

pub fn clear_cache(app: &AppHandle) -> Result<(), String> {
    let path = data_file(app, CACHE_FILE_NAME)?;
    atomic_write(&path, "[]")?;
    let dep_path = data_file(app, DEP_CACHE_FILE_NAME)?;
    atomic_write(&dep_path, "{}")
}

const DEP_CACHE_FILE_NAME: &str = "dep_cache.json";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyCacheEntry {
    pub heavy_deps: Vec<String>,
    pub light_deps: Vec<String>,
    pub version: String,
}

pub fn load_dependency_cache(
    app: &AppHandle,
) -> Result<HashMap<String, DependencyCacheEntry>, String> {
    let path = data_file(app, DEP_CACHE_FILE_NAME)?;
    if !path_entry_exists(&path)? {
        return Ok(HashMap::new());
    }
    let Some(content) =
        read_storage_file_with_recovery(app, DEP_CACHE_FILE_NAME, 5 * 1024 * 1024, "依赖缓存文件")?
    else {
        return Ok(HashMap::new());
    };
    match serde_json::from_str::<HashMap<String, DependencyCacheEntry>>(&content) {
        Ok(cache) => Ok(cache),
        Err(_) => {
            backup_corrupt_file(app, DEP_CACHE_FILE_NAME, &content)?;
            Ok(HashMap::new())
        }
    }
}

pub fn save_dependency_cache(
    app: &AppHandle,
    cache: &HashMap<String, DependencyCacheEntry>,
) -> Result<(), String> {
    let path = data_file(app, DEP_CACHE_FILE_NAME)?;
    let content = serde_json::to_string_pretty(cache).map_err(|error| error.to_string())?;
    atomic_write(&path, &content)
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

        assert_eq!(redacted, MALFORMED_SETTINGS_BACKUP_NOTICE);
    }

    #[test]
    fn drops_malformed_nested_sensitive_settings_content() {
        let content =
            r#"{"githubToken":{"nested":"legacy-secret"},"proxy":["user:pass"],"broken":"#;

        let redacted = redact_settings_backup_content(content);

        assert_eq!(redacted, MALFORMED_SETTINGS_BACKUP_NOTICE);
        assert!(!redacted.contains("legacy-secret"));
        assert!(!redacted.contains("user:pass"));
    }

    #[test]
    fn unique_file_suffix_changes_between_calls() {
        assert_ne!(unique_file_suffix(), unique_file_suffix());
    }

    #[test]
    fn sorted_cache_entries_keeps_newest_then_package_name() {
        fn entry(package_name: &str, cached_at: &str) -> PackageCacheEntry {
            PackageCacheEntry {
                package_name: package_name.to_string(),
                source: "cran".to_string(),
                version: "1.0.0".to_string(),
                repository: String::new(),
                real_name: package_name.to_string(),
                cached_at: cached_at.to_string(),
                verified_count: 3,
                up_votes: 0,
                down_votes: 0,
                invalidated: false,
            }
        }

        let mut cache = HashMap::new();
        cache.insert("zeta".to_string(), entry("zeta", "200"));
        cache.insert("alpha".to_string(), entry("alpha", "200"));
        cache.insert("newest".to_string(), entry("newest", "300"));
        cache.insert("oldest".to_string(), entry("oldest", "100"));

        let sorted = sorted_cache_entries(&cache, 3)
            .into_iter()
            .map(|entry| entry.package_name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(sorted, vec!["newest", "alpha", "zeta"]);
    }

    #[test]
    fn ensure_storage_directory_creates_and_accepts_plain_directory() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-data-dir-{}", unique_file_suffix()));

        ensure_storage_directory(&directory).expect("普通数据目录应可创建");
        assert!(directory.is_dir());
        ensure_storage_directory(&directory).expect("已有普通数据目录应可复用");

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[cfg(windows)]
    #[test]
    fn ensure_storage_directory_rejects_directory_links() {
        use std::os::windows::fs::symlink_dir;

        let root = std::env::temp_dir().join(format!("mod-ui-data-link-{}", unique_file_suffix()));
        let target = root.join("target");
        let link = root.join("data");
        fs::create_dir_all(&target).expect("应能创建链接目标目录");
        symlink_dir(&target, &link).expect("应能创建目录符号链接");

        let metadata = fs::symlink_metadata(&link).expect("应能读取目录链接元数据");
        assert!(metadata_is_windows_reparse_point(&metadata));
        assert!(ensure_storage_directory(&link).is_err());
        assert!(fs::read_dir(&target)
            .expect("应能读取链接目标目录")
            .next()
            .is_none());

        fs::remove_dir(&link).expect("应能移除目录链接");
        fs::remove_dir_all(root).expect("应能清理临时目录");
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
    fn prune_corrupt_backups_bounds_directory_scan() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-backup-scan-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");

        for index in 0..(MAX_CORRUPT_BACKUP_SCAN_ENTRIES + 10) {
            fs::write(directory.join(format!("unrelated-{index:04}.txt")), "data")
                .expect("应能写入目录填充文件");
        }

        assert_eq!(
            prune_corrupt_backups(&directory, "settings.json"),
            MAX_CORRUPT_BACKUP_SCAN_ENTRIES
        );

        fs::remove_dir_all(directory).expect("应能清理临时目录");
    }

    #[test]
    fn saturated_backup_directory_reuses_overflow_file() {
        let directory =
            std::env::temp_dir().join(format!("mod-ui-backup-overflow-{}", unique_file_suffix()));
        fs::create_dir_all(&directory).expect("应能创建临时目录");

        for index in 0..MAX_CORRUPT_BACKUP_SCAN_ENTRIES {
            fs::write(directory.join(format!("unrelated-{index:04}.txt")), "data")
                .expect("应能写入目录填充文件");
        }

        backup_corrupt_path(&directory, "settings.json", "first")
            .expect("饱和目录应可写入固定备份");
        backup_corrupt_path(&directory, "settings.json", "second").expect("饱和目录应复用固定备份");

        let backups = fs::read_dir(&directory)
            .expect("应能列出临时目录")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.corrupt.")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            backups[0].file_name().to_string_lossy(),
            "settings.json.corrupt.overflow.bak"
        );
        assert_eq!(
            fs::read_to_string(backups[0].path()).expect("应能读取固定备份"),
            "second"
        );

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
