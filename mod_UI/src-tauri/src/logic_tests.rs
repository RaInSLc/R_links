#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use crate::logic::*;
    use crate::models::{
        GenerateOptions, InputRules, PackageInput, SearchResult, MAX_FIELD_CHARS, MAX_INPUT_CHARS,
        MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
    };

    #[test]
    fn parses_package_and_version() {
        let value = parse_input_line("GSVA 1.50.0 说明").expect("应解析包输入");
        assert_eq!(value.name, "GSVA");
        assert_eq!(value.version, "1.50.0");
    }

    #[test]
    fn parses_github_repository_url_as_github_input() {
        let value = parse_input_line("https://github.com/davidsjoberg/ggsankey")
            .expect("GitHub 仓库 URL 应可解析");
        assert_eq!(value.name, "davidsjoberg/ggsankey");
        assert_eq!(value.source_hint.as_deref(), Some("github"));
    }

    #[test]
    fn parses_local_r_archive_path() {
        let value = parse_input_line(r"C:\packages\ggsankey_0.0.99999.tar.gz")
            .expect("本地 R 包归档路径应可解析");
        assert_eq!(value.name, "ggsankey");
        assert_eq!(value.source_hint.as_deref(), Some("local"));
    }

    #[test]
    fn rejects_relative_or_non_archive_local_path() {
        assert!(parse_input_line(r"packages\ggsankey.tar.gz").is_none());
        assert!(parse_input_line(r"C:\packages\ggsankey.pdf").is_none());
    }

    #[test]
    fn extracts_github_repository_name() {
        assert_eq!(
            extract_package_name("https://github.com/buenrostrolab/FigR/"),
            "FigR"
        );
        assert_eq!(
            extract_package_name("https://example.org/src/contrib/demo.tgz"),
            "demo"
        );
        assert_eq!(
            extract_package_name("https://example.org/src/contrib/demo.package.zip"),
            "demo.package"
        );
    }

    #[test]
    fn accepts_http_archive_url_with_explicit_port() {
        let value =
            parse_input_line("http://192.168.5.250:8011/softs/Rpackages/scTenifoldNet_1.3.tar.gz")
                .expect("本地 HTTP R 包归档 URL 应可解析");
        assert_eq!(value.name, "scTenifoldNet");
        assert_eq!(value.version, "");
    }

    #[test]
    fn rejects_archive_url_with_query_or_fragment() {
        assert!(
            parse_input_line("http://192.168.5.250:8011/scTenifoldNet_1.3.tar.gz?download=1")
                .is_none()
        );
        assert!(
            parse_input_line("http://192.168.5.250:8011/scTenifoldNet_1.3.tar.gz#download")
                .is_none()
        );
    }

    #[test]
    fn infers_bioconductor_versions() {
        assert_eq!(infer_bioc_version(1, 50), Some(18));
        assert_eq!(infer_bioc_version(1, 34), Some(10));
        assert_eq!(infer_bioc_version(2, 2), Some(22));
    }

    #[test]
    fn generates_conditional_cran_command() {
        let output = generate_script(
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
        assert!(output.contains("requireNamespace(\"dplyr\""));
        assert!(output.contains("dependencies = TRUE"));
    }

    #[test]
    fn auto_routes_explicit_github_repository_without_search_result() {
        let output = generate_script(
            "owner/demo",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("显式 GitHub 仓库应生成 GitHub 安装命令");

        assert!(output.contains("requireNamespace(\"demo\""));
        assert!(output.contains("remotes::install_github(\"owner/demo\""));
        assert!(!output.contains("install.packages(\"owner/demo\""));
    }

    #[test]
    fn check_system_uses_local_name_for_explicit_github_repository() {
        let output = generate_script(
            "owner/demo",
            &GenerateOptions {
                method: "checkSystem".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("系统检查应可处理显式 GitHub 仓库");

        assert!(output.contains("\"demo\""));
        assert!(output.contains("requireNamespace(p, quietly = TRUE)"));
        assert!(output.contains("library(p, character.only = TRUE)"));
        assert!(output.contains("未安装或加载报错"));
        assert!(!output.contains("\"owner/demo\""));
    }

    #[test]
    fn local_install_methods_use_local_name_for_explicit_github_repository() {
        for method in ["base", "version", "biocManager"] {
            let output = generate_script(
                "owner/demo",
                &GenerateOptions {
                    method: method.to_string(),
                    conditional: true,
                    install_dependencies: true,
                    mirror: "https://cloud.r-project.org".to_string(),
                    ..Default::default()
                },
                &[],
            )
            .expect("本地安装类方法应可规范化显式仓库名");

            assert!(output.contains("\"demo\""), "{method}");
            assert!(!output.contains("\"owner/demo\""), "{method}");
        }
    }

    #[test]
    fn install_url_condition_still_uses_archive_package_name() {
        let output = generate_script(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz",
            &GenerateOptions {
                method: "remotes".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("归档 URL 条件安装应保留从文件名提取的包名");

        assert!(output.contains("requireNamespace(\"demo\""));
        assert!(output.contains(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\""
        ));
        assert!(!output.contains("requireNamespace(\"https://"));
    }

    #[test]
    fn auto_routes_archive_urls_per_input_line() {
        let output = generate_script(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz\nother",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: true,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: String::new(),
                latest_version: "9.9.9".to_string(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("自动模式应按行处理安装归档 URL");

        assert!(output.contains(
            "remotes::install_url(\"https://example.org/src/contrib/demo_1.0.0.tar.gz\""
        ));
        assert!(output.contains("requireNamespace(\"demo\", quietly = TRUE)"));
        assert!(output.contains("install.packages(\"other\""));
        assert!(!output.contains("install_version(\"demo\""));
    }

    #[test]
    fn auto_routes_local_http_archive_url_to_install_url() {
        let input = "http://192.168.5.250:8011/softs/Rpackages/scTenifoldNet_1.3.tar.gz";
        let output = generate_script(
            input,
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: false,
                mirror: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("本地 HTTP 归档 URL 应使用远程归档安装");

        assert!(output.contains(
            "remotes::install_url(\"http://192.168.5.250:8011/softs/Rpackages/scTenifoldNet_1.3.tar.gz\""
        ));
        assert!(!output.contains("install.packages(\"scTenifoldNet\""));
    }

    #[test]
    fn rejects_archive_urls_for_incompatible_methods() {
        for method in ["base", "version", "biocManager", "github"] {
            assert!(
                generate_script(
                    "https://example.org/src/contrib/demo_1.0.0.tar.gz",
                    &GenerateOptions {
                        method: method.to_string(),
                        conditional: false,
                        install_dependencies: true,
                        mirror: "https://cloud.r-project.org".to_string(),
                        ..Default::default()
                    },
                    &[],
                )
                .is_err(),
                "{method}"
            );
        }
    }

    #[test]
    fn test_generate_script_for_cran_archive() {
        let options = GenerateOptions {
            method: "auto".to_string(),
            conditional: false,
            install_dependencies: false,
            mirror: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/".to_string(),
            ..Default::default()
        };
        let results = vec![SearchResult {
            package: "oncoPredict".to_string(),
            requested_version: String::new(),
            latest_version: "0.2.0".to_string(),
            repository: "archive".to_string(),
            real_name: "oncoPredict".to_string(),
            source: "cran".to_string(),
            found: true,
            message: "在 Archive 归档区中找到".to_string(),
            status: "found".to_string(),
            stage: "final".to_string(),
        }];

        let script = generate_script("oncoPredict", &options, &results).expect("生成脚本成功");

        assert!(script.contains("# [CRAN 已下架并归档: v0.2.0 | 自动同步]"));
        assert!(script.contains("remotes::install_version(\"oncoPredict\", version = \"0.2.0\", repos = \"https://cloud.r-project.org\", upgrade = \"never\", dependencies = FALSE)"));
    }

    #[test]
    fn generate_script_for_cran_archive_tarball_url() {
        let options = GenerateOptions {
            method: "auto".to_string(),
            conditional: false,
            install_dependencies: false,
            mirror: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/".to_string(),
            ..Default::default()
        };
        let results = vec![SearchResult {
            package: "fastshap".to_string(),
            requested_version: String::new(),
            latest_version: "0.1.1".to_string(),
            repository:
                "https://cran.r-project.org/src/contrib/Archive/fastshap/fastshap_0.1.1.tar.gz"
                    .to_string(),
            real_name: "fastshap".to_string(),
            source: "cran".to_string(),
            found: true,
            message: "在 Archive 归档区中找到".to_string(),
            status: "found".to_string(),
            stage: "final".to_string(),
        }];

        let script = generate_script("fastshap", &options, &results).expect("生成脚本成功");

        assert!(script.contains("# [CRAN 已下架并归档: v0.1.1 | 自动同步]"));
        assert!(script.contains("remotes::install_url(\"https://cran.r-project.org/src/contrib/Archive/fastshap/fastshap_0.1.1.tar.gz\", dependencies = FALSE)"));
        assert!(!script.contains("install.packages(\"fastshap\""));
    }

    #[test]
    fn cran_archive_tarball_takes_priority_over_close_github_version() {
        let options = GenerateOptions {
            method: "auto".to_string(),
            conditional: false,
            install_dependencies: false,
            mirror: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/".to_string(),
            ..Default::default()
        };
        let results = vec![
            SearchResult {
                package: "fastshap".to_string(),
                requested_version: String::new(),
                latest_version: "0.1.1".to_string(),
                repository:
                    "https://cran.r-project.org/src/contrib/Archive/fastshap/fastshap_0.1.1.tar.gz"
                        .to_string(),
                real_name: "fastshap".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "在 Archive 归档区中找到".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            },
            SearchResult {
                package: "fastshap".to_string(),
                requested_version: String::new(),
                latest_version: "0.2.0".to_string(),
                repository: "bgreenwell/fastshap".to_string(),
                real_name: "fastshap".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            },
        ];

        let script = generate_script("fastshap", &options, &results).expect("生成脚本成功");

        assert!(script.contains("# [CRAN 已下架并归档: v0.1.1 | 自动同步]"));
        assert!(script.contains("# [Archive/GitHub 决策: Archive v0.1.1，GitHub v0.2.0，主版本差 0 未达到阈值 1，保留 Archive]"));
        assert!(script.contains("remotes::install_url(\"https://cran.r-project.org/src/contrib/Archive/fastshap/fastshap_0.1.1.tar.gz\", dependencies = FALSE)"));
        assert!(!script.contains("install_github"));
    }

    #[test]
    fn github_replaces_archive_when_major_gap_reaches_threshold() {
        let options = GenerateOptions {
            method: "auto".to_string(),
            conditional: false,
            install_dependencies: false,
            mirror: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/".to_string(),
            archive_github_major_gap: 1,
            ..Default::default()
        };
        let results = vec![
            SearchResult {
                package: "fastshap".to_string(),
                requested_version: String::new(),
                latest_version: "0.1.1".to_string(),
                repository:
                    "https://cran.r-project.org/src/contrib/Archive/fastshap/fastshap_0.1.1.tar.gz"
                        .to_string(),
                real_name: "fastshap".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "在 Archive 归档区中找到".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            },
            SearchResult {
                package: "fastshap".to_string(),
                requested_version: String::new(),
                latest_version: "1.0.0".to_string(),
                repository: "bgreenwell/fastshap".to_string(),
                real_name: "fastshap".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            },
        ];

        let script = generate_script("fastshap", &options, &results).expect("生成脚本成功");

        assert!(script.contains("# [GitHub 已验证: v1.0.0 | 自动同步]"));
        assert!(script.contains("# [Archive/GitHub 决策: GitHub v1.0.0 主版本比 Archive v0.1.1 高 1，达到阈值 1，使用 GitHub]"));
        assert!(script.contains("remotes::install_github(\"bgreenwell/fastshap\", upgrade = \"never\", dependencies = FALSE)"));
        assert!(!script.contains("install_url"));
    }
}
