#[path = "atomic_storage.rs"]
mod atomic_storage;
#[path = "cache_storage.rs"]
mod cache_storage;
#[path = "history_storage.rs"]
mod history_storage;
#[path = "settings_storage.rs"]
mod settings_storage;

#[cfg(test)]
use crate::models::{HistoryRecord, MAX_FIELD_CHARS, MAX_HISTORY_COMMAND_CHARS};
#[cfg(test)]
use crate::models::{PackageCacheEntry, Settings};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::fs;
#[cfg(all(test, windows))]
fn metadata_is_windows_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
#[cfg(all(test, not(windows)))]
fn metadata_is_windows_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}
#[cfg(test)]
fn write_synced_new_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    atomic_storage::write_new(path, content)
}

pub(crate) use cache_storage::{
    cache_entry_matches, clear_cache, clear_invalidated_cache, delete_cache_entry, export_cache,
    import_cache, load_cache, load_dependency_cache, package_cache_key, save_cache,
    save_dependency_cache, DependencyCacheEntry,
};
pub(crate) use history_storage::{load_history, save_history};
pub(crate) use settings_storage::{
    data_file, ensure_data_directory, load_existing_settings, load_input_rules, load_settings,
    save_default_input_rules, save_input_rules, save_settings,
};
#[cfg(test)]
pub(crate) const MAX_CORRUPT_BACKUPS_PER_FILE: usize = 5;
#[cfg(test)]
pub(crate) const MAX_CORRUPT_BACKUP_SCAN_ENTRIES: usize = 512;
#[cfg(test)]
pub(crate) const MAX_HISTORY_LOAD_SCAN_RECORDS: usize = crate::models::MAX_HISTORY_RECORDS * 20;
#[cfg(test)]
pub(crate) const MAX_HISTORY_SAVE_RECORDS: usize = crate::models::MAX_HISTORY_RECORDS * 4;
#[cfg(test)]
pub(crate) fn sanitize_history(value: &[HistoryRecord]) -> Vec<HistoryRecord> {
    history_storage::test_sanitize_history(value)
}
#[cfg(test)]
pub(crate) fn validate_history_save_payload(value: &[HistoryRecord]) -> Result<(), String> {
    history_storage::test_validate_history(value)
}
#[cfg(test)]
pub(crate) fn save_history_to_path(
    path: &std::path::Path,
    value: &[HistoryRecord],
) -> Result<Vec<HistoryRecord>, String> {
    let sanitized = sanitize_history(value);
    atomic_write(
        path,
        &serde_json::to_string_pretty(&sanitized).map_err(|e| e.to_string())?,
    )?;
    Ok(sanitized)
}

#[cfg(test)]
pub(crate) use atomic_storage::{
    atomic_write, backup_corrupt_path, ensure_storage_directory, path_entry_exists,
    prune_corrupt_backups, read_limited_to_string, read_storage_path_with_recovery,
    replace_storage_file, unique_file_suffix, MALFORMED_SETTINGS_BACKUP_NOTICE,
    MAX_HISTORY_FILE_BYTES, MAX_SETTINGS_FILE_BYTES,
};
#[cfg(test)]
pub(crate) use cache_storage::sorted_cache_entries;
#[cfg(test)]
#[cfg(test)]
pub(crate) use settings_storage::{redact_settings_backup_content, StoredSettings};

#[cfg(test)]
#[path = "storage_tests_1.rs"]
mod storage_tests_1;
#[cfg(test)]
#[path = "storage_tests_2.rs"]
mod storage_tests_2;
