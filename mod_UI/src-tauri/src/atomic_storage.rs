use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const MAX_SETTINGS_FILE_BYTES: u64 = 64 * 1024;
pub(crate) const MAX_HISTORY_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub(crate) const OVERSIZED_BACKUP_NOTICE: &str = "原文件超过安全读取上限，内容未复制到备份。";
pub(crate) const MALFORMED_SETTINGS_BACKUP_NOTICE: &str = "设置文件格式损坏，原始内容未写入备份。";
const MAX_BACKUPS: usize = 5;
const MAX_SCAN: usize = 512;
const MAX_TMP_ATTEMPTS: usize = 8;
static WRITE_LOCK: Mutex<()> = Mutex::new(());
static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn ensure_storage_directory(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let metadata = fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata_is_reparse(&metadata) {
        return Err("应用数据目录不是普通目录".to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
#[cfg(not(windows))]
fn metadata_is_reparse(_: &fs::Metadata) -> bool {
    false
}

pub(crate) fn path_entry_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
fn storage_target_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(true),
        Ok(_) => Err("存储目标不是普通文件".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
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
pub(crate) fn read_limited_to_string(
    path: &Path,
    max: u64,
    label: &str,
) -> Result<Option<String>, String> {
    let file = open_storage_file(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!("{label}不是普通文件"));
    }
    if metadata.len() > max {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Ok(None);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{label}不是有效 UTF-8"))
}
pub(crate) fn read_storage_file_with_recovery(
    app: &tauri::AppHandle,
    name: &str,
    max: u64,
    label: &str,
) -> Result<Option<String>, String> {
    let path = super::data_file(app, name)?;
    let directory = path.parent().ok_or_else(|| "存储目录无效".to_string())?;
    read_storage_path_with_recovery(directory, &path, name, max, label)
}
pub(crate) fn read_storage_path_with_recovery(
    directory: &Path,
    path: &Path,
    name: &str,
    max: u64,
    label: &str,
) -> Result<Option<String>, String> {
    match read_limited_to_string(path, max, label) {
        Ok(Some(c)) => Ok(Some(c)),
        Ok(None) => {
            backup_corrupt_path(directory, name, OVERSIZED_BACKUP_NOTICE)?;
            Ok(None)
        }
        Err(e) => {
            backup_corrupt_path(directory, name, &e)?;
            Ok(None)
        }
    }
}
pub(crate) fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|_| "存储写入锁已损坏".to_string())?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "存储文件名无效".to_string())?;
    storage_target_exists(path)?;
    let tmp = create_temp(path, name, content)?;
    if let Err(e) = replace_storage_file(path, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}
fn create_temp(path: &Path, name: &str, content: &str) -> Result<PathBuf, String> {
    for _ in 0..MAX_TMP_ATTEMPTS {
        let tmp = path.with_file_name(format!("{name}.{}.tmp", unique_file_suffix()));
        match write_new(&tmp, content) {
            Ok(()) => return Ok(tmp),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("无法创建唯一的存储临时文件".to_string())
}
pub(crate) fn write_new(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(e) = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(e);
    }
    Ok(())
}
#[cfg(windows)]
pub(crate) fn replace_storage_file(path: &Path, tmp: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };
    fn wide(path: &Path) -> Result<Vec<u16>, String> {
        let mut v: Vec<u16> = path.as_os_str().encode_wide().collect();
        if v.contains(&0) {
            return Err("存储路径包含非法空字符".to_string());
        }
        v.push(0);
        Ok(v)
    }
    let target = wide(path)?;
    let replacement = wide(tmp)?;
    let ok = if storage_target_exists(path)? {
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
    if ok == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
pub(crate) fn replace_storage_file(path: &Path, tmp: &Path) -> Result<(), String> {
    storage_target_exists(path)?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}
pub(crate) fn backup_corrupt_file(
    app: &tauri::AppHandle,
    name: &str,
    content: &str,
) -> Result<(), String> {
    let dir = super::ensure_data_directory(app)?;
    backup_corrupt_path(&dir, name, content)
}
pub(crate) fn backup_corrupt_path(
    directory: &Path,
    name: &str,
    content: &str,
) -> Result<(), String> {
    ensure_storage_directory(directory)?;
    let saturated = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .take(MAX_SCAN)
        .count()
        >= MAX_SCAN;
    let path = if saturated {
        directory.join(format!("{name}.corrupt.overflow.bak"))
    } else {
        directory.join(format!("{name}.corrupt.{}.bak", unique_file_suffix()))
    };
    atomic_write(&path, content)?;
    if !saturated {
        prune_corrupt_backups(directory, name);
    }
    Ok(())
}
pub(crate) fn prune_corrupt_backups(directory: &Path, name: &str) -> usize {
    let Ok(entries) = fs::read_dir(directory) else {
        return 0;
    };
    let prefix = format!("{name}.corrupt.");
    let mut recent: Vec<(SystemTime, String, PathBuf)> = Vec::with_capacity(MAX_BACKUPS);
    let mut scanned = 0;
    for entry in entries.take(MAX_SCAN) {
        scanned += 1;
        let Ok(entry) = entry else { continue };
        let fname = entry.file_name().to_string_lossy().into_owned();
        if !fname.starts_with(&prefix)
            || !fname.ends_with(".bak")
            || !entry.file_type().ok().is_some_and(|t| t.is_file())
        {
            continue;
        };
        let item = (
            entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH),
            fname,
            entry.path(),
        );
        if recent.len() < MAX_BACKUPS {
            recent.push(item);
            continue;
        }
        if let Some((i, old)) = recent
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
        {
            if item.0.cmp(&old.0).then_with(|| item.1.cmp(&old.1)).is_gt() {
                let old_path = std::mem::replace(&mut recent[i], item).2;
                let _ = fs::remove_file(old_path);
            } else {
                let _ = fs::remove_file(item.2);
            }
        }
    }
    scanned
}
pub(crate) fn unique_file_suffix() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{stamp}.{}", FILE_COUNTER.fetch_add(1, Ordering::Relaxed))
}
