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
    assert_eq!(settings.search_concurrency, 6);
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
    let content = r#"{"githubToken":{"nested":"legacy-secret"},"proxy":["user:pass"],"broken":"#;

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
    let directory = std::env::temp_dir().join(format!("mod-ui-data-dir-{}", unique_file_suffix()));

    ensure_storage_directory(&directory).expect("普通数据目录应可创建");
    assert!(directory.is_dir());
    ensure_storage_directory(&directory).expect("已有普通数据目录应可复用");

    fs::remove_dir_all(directory).expect("应能清理临时目录");
}

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
    let directory = std::env::temp_dir().join(format!("mod-ui-storage-{}", unique_file_suffix()));
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
