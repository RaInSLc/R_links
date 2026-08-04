#[cfg(test)]
mod tests {
    use crate::logic::*;
    use crate::models::{
        GenerateOptions, InputRules, PackageInput, SearchResult, MAX_FIELD_CHARS, MAX_INPUT_CHARS,
        MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
    };

    #[test]
    fn builds_history_from_supported_conditional_command_body() {
        let script = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("应生成命令");

        let records = build_history_records(&script);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].package_name, "dplyr");
        assert_eq!(records[0].tool_name, "base R");
        assert!(records[0].command.starts_with("install.packages("));
    }

    #[test]
    fn rejects_unsupported_history_commands() {
        assert!(supported_history_command("system(\"calc.exe\")").is_none());
        assert!(supported_history_command("not_a_supported_command()").is_none());
        assert!(supported_history_command("install.packages(pkg)").is_none());
        assert!(supported_history_command(
            "install.packages(\"demo\", repos = \"http://example.com\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org\", dependencies = TRUE)"
        )
        .is_some());
        assert!(supported_history_command(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org?token=secret\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "remotes::install_version(\"demo\", version = \"1.2.3\", repos = \"https://cloud.r-project.org/\", upgrade = \"never\", dependencies = TRUE)"
        )
        .is_some());
        assert!(supported_history_command(
            "remotes::install_version(\"demo\", version = \"1.2.3\", repos = \"https://cloud.r-project.org/#cran\", upgrade = \"never\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\", dependencies = TRUE)"
        )
        .is_some());
        assert!(supported_history_command(
            "remotes::install_url(\"https://example.org:443/src/contrib/demo_1.0.0.tar.gz\", dependencies = TRUE)"
        )
        .is_some());
        assert!(supported_history_command(
            "remotes::install_url(\"https://github.com/owner/demo\", dependencies = TRUE)"
        )
        .is_none());
        assert!(supported_history_command(
            "devtools::install_url(\"https://example.org/demo_1.0.0.tar.gz?token=secret\", dependencies = TRUE)"
        )
        .is_none());
    }

    #[test]
    fn rejects_invalid_github_repository() {
        assert!(!is_valid_github_repository("../repo"));
        assert!(!is_valid_github_repository("owner_name/repo"));
        assert!(!is_valid_github_repository("owner.name/repo"));
        assert!(!is_valid_github_repository("-owner/repo"));
        assert!(!is_valid_github_repository("owner-/repo"));
        assert!(!is_valid_github_repository(&format!(
            "{}/repo",
            "a".repeat(40)
        )));
        assert!(is_valid_github_repository("owner/repo.name"));
        assert!(is_valid_github_repository("owner-name/repo_name"));
        assert!(is_valid_github_repository("owner/repo/extra"));
        assert!(is_valid_github_repository("owner/repo/path/to/subdir"));
        assert!(normalize_github_repository("https://github.com/owner.name/repo").is_none());
        assert!(normalize_github_repository("https://github.com:443/owner/repo").is_none());
    }

    #[test]
    fn rejects_oversized_input() {
        let input = "pkg\n".repeat(MAX_PACKAGE_LINES + 1);
        assert!(parse_inputs(&input).is_err());
    }

    #[test]
    fn ignores_comment_lines_when_counting_package_limit() {
        let input = format!(
            "{}\n{}",
            "# comment\n".repeat(MAX_PACKAGE_LINES + 10),
            "pkg\n".repeat(MAX_PACKAGE_LINES)
        );

        assert_eq!(
            parse_inputs(&input)
                .expect("注释行不应占用包数量限制")
                .len(),
            MAX_PACKAGE_LINES
        );
    }

    #[test]
    fn still_rejects_oversized_comment_lines() {
        let input = format!("# {}", "x".repeat(MAX_INPUT_LINE_BYTES));

        assert!(validate_input_size(&input).is_err());
    }

    #[test]
    fn rejects_oversized_or_controlled_input_before_parse() {
        let multibyte = "注".repeat((MAX_INPUT_CHARS / "注".len()) + 1);
        assert!(multibyte.chars().count() < MAX_INPUT_CHARS);
        assert!(multibyte.len() > MAX_INPUT_CHARS);
        assert!(validate_input_size(&multibyte).is_err());
        assert!(parse_inputs("demo\u{7f}\n").is_err());
        assert!(parse_inputs("demo\t1.2.3").is_ok());
    }

    #[test]
    fn rejects_oversized_input_line_before_parse() {
        let long_line = format!("demo {}", "1".repeat(MAX_INPUT_LINE_BYTES));
        assert!(validate_input_size(&long_line).is_err());

        let multibyte_line = format!(
            "demo {}",
            "注".repeat((MAX_INPUT_LINE_BYTES / "注".len()) + 1)
        );
        assert!(multibyte_line.chars().count() < MAX_INPUT_LINE_BYTES);
        assert!(multibyte_line.len() > MAX_INPUT_LINE_BYTES);
        assert!(parse_inputs(&multibyte_line).is_err());
    }

    #[test]
    fn rejects_oversized_requested_versions() {
        let input = format!("demo {}", "1".repeat(MAX_VERSION_CHARS + 1));

        assert!(parse_input_line(&input).is_none());
        assert!(parse_inputs(&input).is_err());
    }

    #[test]
    fn bounds_result_message_by_utf8_bytes() {
        let message = clean_result_text(&"注".repeat(MAX_RESULT_MESSAGE_CHARS));

        assert!(message.len() <= MAX_RESULT_MESSAGE_CHARS);
        assert!(message.ends_with('注'));
    }

    #[test]
    fn validates_browser_search_url_scope() {
        assert!(is_allowed_browser_search_url(
            "https://www.google.com/search?q=R%20package%20GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "http://www.google.com/search?q=GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://example.com/search?q=GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://www.google.com/search?q=GSVA&source=desktop"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://www.google.com/preferences?q=GSVA"
        ));
        assert!(!is_allowed_browser_search_url(
            "https://www.google.com/search?q=GSVA#frag"
        ));
    }

    #[test]
    fn rejects_oversized_history_script() {
        let script = "install.packages(\"demo\")\n".repeat((MAX_SCRIPT_CHARS / 25) + 10);
        assert!(build_history_records(&script).is_empty());
        assert!(validate_script_size(&script).is_err());
    }

    #[test]
    fn rejects_oversized_multibyte_script_by_bytes() {
        let script = "注".repeat((MAX_SCRIPT_CHARS / "注".len()) + 1);
        assert!(script.chars().count() < MAX_SCRIPT_CHARS);
        assert!(script.len() > MAX_SCRIPT_CHARS);
        assert!(validate_script_size(&script).is_err());
        assert!(build_history_records(&script).is_empty());
        assert!(clean_script(&script).is_err());
    }

    #[test]
    fn cleans_script_and_rejects_oversized_cleaned_output() {
        let cleaned = clean_script("# comment\n\ninstall.packages(\"demo\")\n")
            .expect("普通脚本应可清理注释");
        assert_eq!(cleaned, "install.packages(\"demo\")");

        let script = "x\n".repeat((MAX_SCRIPT_CHARS / 3) + 2);
        assert!(validate_script_size(&script).is_ok());
        assert!(clean_script(&script).is_err());
    }

    #[test]
    fn bounds_history_scan_lines() {
        let script = format!(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org/\", dependencies = TRUE)\n{}",
            "not_a_supported_command()\n".repeat(MAX_HISTORY_SCAN_LINES + 1)
        );

        assert!(build_history_records(&script).is_empty());

        let script = format!(
            "install.packages(\"demo\", repos = \"https://cloud.r-project.org/\", dependencies = TRUE)\n{}",
            "# ignored\n\n".repeat(MAX_HISTORY_SCAN_LINES + 10)
        );

        assert_eq!(build_history_records(&script).len(), 1);
    }

    #[test]
    fn rejects_oversized_generated_script() {
        let input = (0..MAX_PACKAGE_LINES)
            .map(|index| format!("package{index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let oversized_mirror = format!("https://{}.example.org/CRAN/", "a".repeat(1900));

        let result = generate_script(
            &input,
            &GenerateOptions {
                method: "base".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: oversized_mirror,
                ..Default::default()
            },
            &[],
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_generate_method_without_echoing_value() {
        let method = format!("bad{}\n{}", "x".repeat(128), "system(\"calc.exe\")");
        let error = generate_script(
            "demo",
            &GenerateOptions {
                method: method.clone(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect_err("非法安装方式应被拒绝");

        assert_eq!(error, "安装方式无效");
        assert!(!error.contains(&method));
        assert!(!error.contains("calc.exe"));
    }

    #[test]
    fn rejects_unbounded_generate_search_results() {
        let results = (0..=MAX_GENERATE_SEARCH_RESULTS)
            .map(|index| SearchResult {
                package: format!("demo{index}"),
                requested_version: String::new(),
                latest_version: "1.0.0".to_string(),
                repository: String::new(),
                real_name: format!("demo{index}"),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            })
            .collect::<Vec<_>>();

        let error = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &results,
        )
        .expect_err("超出上限的检索结果应被拒绝");

        assert!(error.contains("检索结果数量过多"));
    }

    #[test]
    fn uses_search_results_across_the_full_accepted_range() {
        let mut results = vec![
            SearchResult {
                package: "other".to_string(),
                requested_version: String::new(),
                latest_version: String::new(),
                repository: String::new(),
                real_name: "other".to_string(),
                source: "none".to_string(),
                found: false,
                message: "未找到".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            };
            MAX_GENERATE_SEARCH_RESULTS - 1
        ];
        results.push(SearchResult {
            package: "target".to_string(),
            requested_version: String::new(),
            latest_version: "9.9.9".to_string(),
            repository: String::new(),
            real_name: "target".to_string(),
            source: "cran".to_string(),
            found: true,
            message: "验证成功".to_string(),
            status: "found".to_string(),
            stage: "final".to_string(),
        });

        let output = generate_script(
            "target",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &results,
        )
        .expect("允许范围末端的合法检索结果应参与脚本生成");

        assert!(output.contains("remotes::install_version(\"target\", version = \"9.9.9\""));
    }
}
