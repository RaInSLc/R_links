use super::*;
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
            ..HistoryRecord::default()
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
            ..HistoryRecord::default()
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
                ..HistoryRecord::default()
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
            ..HistoryRecord::default()
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
            ..HistoryRecord::default()
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
            ..HistoryRecord::default()
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
            ..HistoryRecord::default()
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
            ..HistoryRecord::default()
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
                ..HistoryRecord::default()
            })
            .collect::<Vec<_>>();

        let error = validate_history_save_payload(&history)
            .expect_err("总大小过大的历史保存 payload 应被拒绝");

        assert!(error.contains("历史记录总大小过大"));
    }
