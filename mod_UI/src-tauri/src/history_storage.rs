use super::atomic_storage::{
    atomic_write, backup_corrupt_file, path_entry_exists, read_storage_file_with_recovery,
    MAX_HISTORY_FILE_BYTES,
};
use super::settings_storage::data_file;
use crate::{
    logic,
    models::{HistoryRecord, MAX_FIELD_CHARS, MAX_HISTORY_COMMAND_CHARS, MAX_HISTORY_RECORDS},
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
const MAX_RECORDS: usize = MAX_HISTORY_RECORDS * 4;
const MAX_SCAN: usize = MAX_HISTORY_RECORDS * 20;
pub(crate) fn load_history(app: &AppHandle) -> Result<Vec<HistoryRecord>, String> {
    let p = data_file(app, "history.json")?;
    if !path_entry_exists(&p)? {
        return Ok(Vec::new());
    }
    let Some(c) =
        read_storage_file_with_recovery(app, "history.json", MAX_HISTORY_FILE_BYTES, "历史文件")?
    else {
        return Ok(Vec::new());
    };
    match serde_json::from_str::<Vec<HistoryRecord>>(&c) {
        Ok(v) => Ok(sanitize_history(&v)),
        Err(_) => {
            backup_corrupt_file(app, "history.json", &c)?;
            Ok(Vec::new())
        }
    }
}
pub(crate) fn save_history(
    app: &AppHandle,
    h: &[HistoryRecord],
) -> Result<Vec<HistoryRecord>, String> {
    validate(h)?;
    let v = sanitize_history(h);
    atomic_write(
        &data_file(app, "history.json")?,
        &serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?,
    )?;
    Ok(v)
}
#[cfg(test)]
pub(crate) fn test_sanitize_history(h: &[HistoryRecord]) -> Vec<HistoryRecord> {
    sanitize_history(h)
}
#[cfg(test)]
pub(crate) fn test_validate_history(h: &[HistoryRecord]) -> Result<(), String> {
    validate(h)
}
fn validate(h: &[HistoryRecord]) -> Result<(), String> {
    if h.len() > MAX_RECORDS {
        return Err(format!("历史记录数量过多，最多允许 {MAX_RECORDS} 条"));
    }
    let mut total = 0;
    for r in h {
        for (label, v, max) in [
            ("历史记录 ID", &r.id, 64),
            ("历史记录命令", &r.command, MAX_HISTORY_COMMAND_CHARS),
            ("历史记录包名", &r.package_name, MAX_FIELD_CHARS),
            ("历史记录版本", &r.version, 64),
            ("历史记录工具名", &r.tool_name, MAX_FIELD_CHARS),
            ("历史记录时间戳", &r.created_at, 32),
        ] {
            if v.len() > max {
                return Err(format!("{label}长度过长，最多允许 {max} 字节"));
            }
            if v.chars().any(char::is_control) {
                return Err(format!("{label}包含非法控制字符"));
            }
            total += v.len()
        }
        if total > 10 * 1024 * 1024 {
            return Err("历史记录总大小过大，最多允许 10485760 字节".to_string());
        }
    }
    Ok(())
}
pub(crate) fn sanitize_history(h: &[HistoryRecord]) -> Vec<HistoryRecord> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    h.iter()
        .take(MAX_SCAN)
        .enumerate()
        .filter_map(|(i, r)| {
            let command = logic::supported_history_command(&r.command)?;
            let (package_name, version, tool_name) =
                logic::history_metadata_from_command(&command)?;
            Some(HistoryRecord {
                id: if clean_id(&r.id) {
                    r.id.clone()
                } else {
                    format!("{now}-{i}")
                },
                command,
                package_name,
                version,
                tool_name,
                created_at: if clean_time(&r.created_at) {
                    r.created_at.clone()
                } else {
                    now.to_string()
                },
                input: r.input.clone(),
                method: r.method.clone(),
                conditional: r.conditional,
                install_dependencies: r.install_dependencies,
                show_remote_version: r.show_remote_version,
                verify_install: r.verify_install,
                cran_mirror: r.cran_mirror.clone(),
            })
        })
        .take(MAX_HISTORY_RECORDS)
        .collect()
}
fn clean_id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}
fn clean_time(v: &str) -> bool {
    !v.is_empty() && v.len() <= 32 && v.chars().all(|c| c.is_ascii_digit())
}
