use super::atomic_storage::{
    atomic_write, backup_corrupt_file, path_entry_exists, read_storage_file_with_recovery,
};
use super::settings_storage::{data_file, load_existing_settings};
use crate::models::{PackageCacheEntry, MAX_FIELD_CHARS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
const CACHE_FILE_NAME: &str = "pkg_cache.json";
const DEP_CACHE_FILE_NAME: &str = "dep_cache.json";
const MAX_CACHE_IMPORT_BYTES: usize = 8 * 1024 * 1024;
const MAX_CACHE_STORAGE_BYTES: usize = 8 * 1024 * 1024;
const CACHE_KEY_SEPARATOR: char = '\u{1f}';
#[cfg(test)]
mod regression_tests {
    use super::*;
    #[test]
    fn 大缓存序列化后仍处于相同读取预算内() {
        let mut cache = HashMap::new();
        for index in 0..5000 {
            let name = format!("package{index}");
            cache.insert(
                name.clone(),
                PackageCacheEntry {
                    package_name: name.clone(),
                    real_name: name,
                    source: "cran".into(),
                    version: "1.0.0".into(),
                    repository: String::new(),
                    cached_at: "1800000000".into(),
                    verified_count: 1,
                    up_votes: 0,
                    down_votes: 0,
                    invalidated: false,
                },
            );
        }
        let content = serialize_package_cache(&cache, 10000).unwrap();
        assert!(content.len() > 1024 * 1024);
        assert!(content.len() <= MAX_CACHE_STORAGE_BYTES);
        let entries: Vec<PackageCacheEntry> = serde_json::from_str(&content).unwrap();
        assert_eq!(entries.len(), 5000);
        for entry in cache.values_mut() {
            entry.repository = "x".repeat(2048);
        }
        assert!(serialize_package_cache(&cache, 10000).is_err());
    }
}
fn serialize_package_cache(
    cache: &HashMap<String, PackageCacheEntry>,
    limit: usize,
) -> Result<String, String> {
    let content = serde_json::to_string_pretty(&sorted_cache_entries(cache, limit))
        .map_err(|error| error.to_string())?;
    if content.len() > MAX_CACHE_STORAGE_BYTES {
        return Err("缓存内容超过 8 MiB 存储限制，请减少缓存条目".to_string());
    }
    Ok(content)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyCacheEntry {
    #[serde(default)]
    pub cached_at: u64,
    pub heavy_deps: Vec<String>,
    pub light_deps: Vec<String>,
    pub version: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub repository: String,
}
pub(crate) fn package_cache_key(source: &str, package_name: &str, repository: &str) -> String {
    format!("{source}{CACHE_KEY_SEPARATOR}{package_name}{CACHE_KEY_SEPARATOR}{repository}")
}
fn limit(app: &AppHandle) -> usize {
    load_existing_settings(app)
        .ok()
        .flatten()
        .map(|s| s.max_cache_entries)
        .unwrap_or(1000)
}
pub(crate) fn load_cache(app: &AppHandle) -> Result<HashMap<String, PackageCacheEntry>, String> {
    let mut cache = load_raw_cache(app)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    cache.retain(|_, entry| {
        !entry.invalidated
            && entry
                .cached_at
                .parse::<u64>()
                .map(|time| now.saturating_sub(time) < 7 * 24 * 3600)
                .unwrap_or(false)
    });
    Ok(cache)
}
fn load_raw_cache(app: &AppHandle) -> Result<HashMap<String, PackageCacheEntry>, String> {
    let p = data_file(app, CACHE_FILE_NAME)?;
    if !path_entry_exists(&p)? {
        return Ok(HashMap::new());
    }
    let Some(c) = read_storage_file_with_recovery(
        app,
        CACHE_FILE_NAME,
        MAX_CACHE_STORAGE_BYTES as u64,
        "包缓存文件",
    )?
    else {
        return Ok(HashMap::new());
    };
    match serde_json::from_str::<Vec<PackageCacheEntry>>(&c) {
        Ok(es) => {
            let mut cache = HashMap::new();
            for e in es.into_iter().take(limit(app)) {
                let k = package_cache_key(&e.source, &e.package_name, &e.repository);
                #[allow(clippy::nonminimal_bool)]
                if !k.is_empty()
                    && !e.source.is_empty()
                    && e.package_name.len() <= MAX_FIELD_CHARS
                    && e.real_name.len() <= MAX_FIELD_CHARS
                    && e.repository.len() <= MAX_FIELD_CHARS
                    && !(e.package_name.eq_ignore_ascii_case("oncopredict")
                        && e.source == "cran"
                        && e.repository.is_empty())
                {
                    cache.insert(k, e);
                }
            }
            Ok(cache)
        }
        Err(_) => {
            backup_corrupt_file(app, CACHE_FILE_NAME, &c)?;
            Ok(HashMap::new())
        }
    }
}
pub(crate) fn save_cache(
    app: &AppHandle,
    cache: &HashMap<String, PackageCacheEntry>,
) -> Result<(), String> {
    let c = serialize_package_cache(cache, limit(app))?;
    atomic_write(&data_file(app, CACHE_FILE_NAME)?, &c)
}
pub(crate) fn export_cache(app: &AppHandle) -> Result<String, String> {
    serde_json::to_string_pretty(&sorted_cache_entries(&load_cache(app)?, usize::MAX))
        .map_err(|e| format!("缓存导出失败: {e}"))
}
pub(crate) fn import_cache(app: &AppHandle, content: &str) -> Result<usize, String> {
    if content.len() > MAX_CACHE_IMPORT_BYTES {
        return Err("缓存文件超过 8 MB 导入限制".to_string());
    }
    let entries = serde_json::from_str::<Vec<PackageCacheEntry>>(content)
        .map_err(|_| "缓存文件格式无效，应为缓存条目 JSON 数组".to_string())?;
    let mut cache = load_cache(app)?;
    let before = cache.len();
    for e in entries.into_iter().take(10_000) {
        if e.package_name.trim().is_empty()
            || e.real_name.trim().is_empty()
            || e.source.trim().is_empty()
            || e.package_name.len() > MAX_FIELD_CHARS
            || e.real_name.len() > MAX_FIELD_CHARS
            || e.version.len() > 64
            || e.repository.len() > MAX_FIELD_CHARS
        {
            continue;
        }
        let k = package_cache_key(&e.source, &e.package_name, &e.repository);
        if cache.get(&k).is_none_or(|c| {
            (!c.is_trusted() && e.is_trusted())
                || e.cached_at.parse::<u64>().unwrap_or_default()
                    > c.cached_at.parse::<u64>().unwrap_or_default()
        }) {
            cache.insert(k, e);
        }
    }
    save_cache(app, &cache)?;
    Ok(cache.len().min(limit(app)).saturating_sub(before))
}
pub(crate) fn sorted_cache_entries(
    cache: &HashMap<String, PackageCacheEntry>,
    limit: usize,
) -> Vec<&PackageCacheEntry> {
    let mut v: Vec<_> = cache
        .values()
        .map(|e| {
            (
                e.cached_at.parse::<u64>().unwrap_or_default(),
                e.package_name.to_ascii_lowercase(),
                e,
            )
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    v.truncate(limit);
    v.into_iter().map(|x| x.2).collect()
}
pub(crate) fn clear_cache(app: &AppHandle) -> Result<(), String> {
    atomic_write(&data_file(app, CACHE_FILE_NAME)?, "[]")?;
    atomic_write(&data_file(app, DEP_CACHE_FILE_NAME)?, "{}")
}
pub(crate) fn clear_invalidated_cache(app: &AppHandle) -> Result<usize, String> {
    let mut c = load_raw_cache(app)?;
    let n = c.len();
    c.retain(|_, e| !e.invalidated);
    save_cache(app, &c)?;
    Ok(n - c.len())
}
/// 缓存身份匹配：大小写敏感，GitHub 身份仅靠 `repository` 区分。
/// 大小写不同的包名（如 `Scissor` 与 `scissor`）必须视为不同记录。
pub(crate) fn cache_entry_matches(
    entry: &PackageCacheEntry,
    package: &str,
    source: &str,
    version: &str,
    repository: &str,
    real_name: &str,
) -> bool {
    entry.package_name == package.trim()
        && entry.source == source
        && entry.version == version
        && entry.repository == repository
        && entry.real_name == real_name
}

pub(crate) fn delete_cache_entry(
    app: &AppHandle,
    package: &str,
    source: &str,
    version: &str,
    repository: &str,
    real_name: &str,
) -> Result<(), String> {
    if package.trim().is_empty() || source.trim().is_empty() {
        return Err("缓存删除参数无效".to_string());
    }
    let mut c = load_cache(app)?;
    let n = c.len();
    c.retain(|_, e| !cache_entry_matches(e, package, source, version, repository, real_name));
    if c.len() == n {
        return Err("没有找到匹配的缓存记录".to_string());
    }
    save_cache(app, &c)
}
pub(crate) fn load_dependency_cache(
    app: &AppHandle,
) -> Result<HashMap<String, DependencyCacheEntry>, String> {
    let p = data_file(app, DEP_CACHE_FILE_NAME)?;
    if !path_entry_exists(&p)? {
        return Ok(HashMap::new());
    }
    let Some(c) =
        read_storage_file_with_recovery(app, DEP_CACHE_FILE_NAME, 5 * 1024 * 1024, "依赖缓存文件")?
    else {
        return Ok(HashMap::new());
    };
    match serde_json::from_str::<HashMap<String, DependencyCacheEntry>>(&c) {
        Ok(mut v) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            v.retain(|_, entry| {
                entry.cached_at > 0 && now.saturating_sub(entry.cached_at) < 7 * 24 * 3600
            });
            Ok(v)
        }
        Err(_) => {
            backup_corrupt_file(app, DEP_CACHE_FILE_NAME, &c)?;
            Ok(HashMap::new())
        }
    }
}
pub(crate) fn save_dependency_cache(
    app: &AppHandle,
    cache: &HashMap<String, DependencyCacheEntry>,
) -> Result<(), String> {
    let mut entries: Vec<_> = cache.iter().collect();
    entries.sort_by(|(left_key, left), (right_key, right)| {
        right
            .cached_at
            .cmp(&left.cached_at)
            .then_with(|| left_key.cmp(right_key))
    });
    entries.truncate(limit(app));
    let bounded: HashMap<_, _> = entries.into_iter().collect();
    let content = serde_json::to_string_pretty(&bounded).map_err(|error| error.to_string())?;
    if content.len() > 5 * 1024 * 1024 {
        return Err("依赖缓存超过 5 MiB 存储限制".to_string());
    }
    atomic_write(&data_file(app, DEP_CACHE_FILE_NAME)?, &content)
}
