#[cfg(test)]
mod tests {
    use crate::logic::*;
    use crate::models::{
        GenerateOptions, InputRules, PackageInput, SearchResult, MAX_FIELD_CHARS, MAX_INPUT_CHARS,
        MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
    };

    #[test]
    fn rejects_unsafe_install_url_inputs() {
        assert!(parse_input_line("https://example.org/src/contrib/demo_1.0.0.tar.gz").is_some());
        assert!(parse_input_line("demo https://example.org/pkg_1.0.tar.gz").is_none());
        assert!(parse_input_line("https://user:pass@example.com/pkg_1.0.tar.gz").is_none());
        assert!(parse_input_line("https://example.org:443/pkg_1.0.tar.gz").is_some());
        assert!(parse_input_line("http://example.com/pkg_1.0.tar.gz").is_some());
        assert!(parse_input_line("ftp://example.com/pkg_1.0.tar.gz").is_none());
        assert_eq!(
            parse_input_line("https://github.com/owner/demo")
                .expect("合法 GitHub 仓库 URL 应可解析")
                .name,
            "owner/demo"
        );
        assert!(parse_input_line("https://example.com/pkg_1.0.tar.gz?token=secret").is_none());
        assert!(parse_input_line("https://example.com/pkg_1.0.tar.gz#section").is_none());
        assert!(parse_input_line("https://example.com/index.html").is_none());

        let output = generate_script(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("合法安装归档 URL 应可生成 install_url 命令");
        assert!(output.contains(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\""
        ));

        assert!(generate_script(
            "https://user:pass@example.com/pkg_1.0.tar.gz",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "https://github.com/owner/demo",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "demo https://example.org/pkg_1.0.tar.gz",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "demo",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "http://cran.example.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
        assert!(generate_script(
            "demo",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org?token=secret".to_string(),
                ..Default::default()
            },
            &[],
        )
        .is_err());
    }

    #[test]
    fn rejects_untrusted_search_results_for_auto_script() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: "https://example.com/github.com/evil/demo".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("非法检索结果应被忽略并回退基础安装");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains("evil/demo"));
    }

    #[test]
    fn ignores_inconsistent_found_search_results_for_auto_script() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "9.9.9".to_string(),
                    repository: String::new(),
                    real_name: "demo".to_string(),
                    source: "none".to_string(),
                    found: true,
                    message: "伪造成功".to_string(),
                    status: "found".to_string(),
                    stage: "final".to_string(),
                },
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "8.8.8".to_string(),
                    repository: String::new(),
                    real_name: "demo".to_string(),
                    source: "github".to_string(),
                    found: true,
                    message: "缺少仓库".to_string(),
                    status: "found".to_string(),
                    stage: "final".to_string(),
                },
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "7.7.7".to_string(),
                    repository: "owner/demo".to_string(),
                    real_name: "demo\nbad".to_string(),
                    source: "github".to_string(),
                    found: true,
                    message: "非法真实包名".to_string(),
                    status: "found".to_string(),
                    stage: "final".to_string(),
                },
            ],
        )
        .expect("矛盾检索结果应被忽略并回退基础安装");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains("9.9.9"));
        assert!(!output.contains("8.8.8"));
        assert!(!output.contains("7.7.7"));
        assert!(!output.contains("owner/demo"));
    }

    #[test]
    fn ignores_search_results_without_real_name_or_repository_match() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: "owner/not-demo".to_string(),
                real_name: "not-demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("身份不匹配的检索结果应被忽略");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains("owner/not-demo"));
    }

    #[test]
    fn prefers_case_insensitive_real_name_matches() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "1.0.0".to_string(),
                    repository: "https://github.com/other/demo".to_string(),
                    real_name: "not-demo".to_string(),
                    source: "github".to_string(),
                    found: true,
                    message: "验证成功".to_string(),
                    status: "found".to_string(),
                    stage: "final".to_string(),
                },
                SearchResult {
                    package: "demo".to_string(),
                    requested_version: String::new(),
                    latest_version: "2.0.0".to_string(),
                    repository: String::new(),
                    real_name: "Demo".to_string(),
                    source: "cran".to_string(),
                    found: true,
                    message: "验证成功".to_string(),
                    status: "found".to_string(),
                    stage: "final".to_string(),
                },
            ],
        )
        .expect("大小写差异的真实包名应参与优先排序");

        assert!(output.contains("remotes::install_version(\"demo\", version = \"2.0.0\""));
        assert!(!output.contains("install_github"));
    }

    #[test]
    fn accepts_sanitized_search_results_for_auto_script() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: " demo ".to_string(),
                requested_version: String::new(),
                latest_version: " 1.2.3 ".to_string(),
                repository: "https://github.com/owner/demo.git".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("合法检索结果应可参与自动路由");

        assert!(output.contains("remotes::install_github(\"owner/demo\""));
        assert!(!output.contains("https://github.com/owner/demo.git"));
    }

    #[test]
    fn hides_remote_versions_without_losing_source_routing() {
        let options = GenerateOptions {
            method: "auto".to_string(),
            conditional: true,
            install_dependencies: true,
            mirror: "https://cloud.r-project.org".to_string(),
            ..Default::default()
        };
        let cran_output = generate_script_with_remote_versions(
            "demo",
            &options,
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
            false,
        )
        .expect("隐藏远程版本时 CRAN 来源仍应生成脚本");

        assert!(cran_output.contains("install.packages(\"demo\""));
        assert!(!cran_output.contains("install_version"));
        assert!(!cran_output.contains("1.2.3"));

        let github_output = generate_script_with_remote_versions(
            "demo",
            &options,
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "2.0.0".to_string(),
                repository: "owner/demo".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
            false,
        )
        .expect("隐藏远程版本时 GitHub 来源路由仍应保留");

        assert!(github_output.contains("remotes::install_github(\"owner/demo\""));
        assert!(!github_output.contains("2.0.0"));
    }

    #[test]
    fn rejects_invalid_bioc_git_package_names() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3".to_string(),
                repository: "3.18".to_string(),
                real_name: "demo".to_string(),
                source: "biocGit".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("合法 Bioconductor 历史版本结果应可生成安装命令");
        assert!(output.contains("https://git.bioconductor.org/packages/demo"));

        let error = generate_command(
            "owner/demo",
            "biocGit",
            "1.2.3|3.18",
            false,
            "https://cloud.r-project.org/",
            true,
        )
        .expect_err("仓库式路径不应进入 Bioconductor Git URL");

        assert!(error.contains("不是有效的 Bioconductor 包名"));
    }

    #[test]
    fn rejects_result_versions_with_control_characters() {
        let output = generate_script(
            "demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "1.2.3\nInjected".to_string(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("非法版本检索结果应被忽略");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_version"));
        assert!(!output.contains("Injected"));
    }
}
