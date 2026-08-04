#[cfg(test)]
mod tests {
    use crate::logic::*;
    use crate::models::{
        GenerateOptions, InputRules, PackageInput, SearchResult, MAX_FIELD_CHARS, MAX_INPUT_CHARS,
        MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
    };

    #[test]
    fn ignores_search_results_with_oversized_versions() {
        let oversized_version = "1".repeat(MAX_VERSION_CHARS + 1);
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
                latest_version: oversized_version.clone(),
                repository: String::new(),
                real_name: "demo".to_string(),
                source: "cran".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("超长版本检索结果应被忽略");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_version"));
        assert!(!output.contains(&oversized_version));
    }

    #[test]
    fn ignores_search_results_with_oversized_fields() {
        let huge_repository = format!("https://github.com/owner/{}", "a".repeat(MAX_FIELD_CHARS));
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
                repository: huge_repository.clone(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("超大字段结果应被忽略并回退基础安装");

        assert!(output.contains("install.packages(\"demo\""));
        assert!(!output.contains("install_github"));
        assert!(!output.contains(&huge_repository));
    }

    #[test]
    fn generate_script_github_with_version_ref() {
        let output = generate_script(
            "demo 1.2.3",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[SearchResult {
                package: "demo".to_string(),
                requested_version: "1.2.3".to_string(),
                latest_version: "1.2.3".to_string(),
                repository: "owner/demo".to_string(),
                real_name: "demo".to_string(),
                source: "github".to_string(),
                found: true,
                message: "验证成功".to_string(),
                status: "found".to_string(),
                stage: "final".to_string(),
            }],
        )
        .expect("GitHub 智能路由带版本结果应可生成带有 ref 的 install_github");

        assert!(output.contains("remotes::install_github(\"owner/demo@v1.2.3\""));
    }

    #[test]
    fn parses_comma_separated_quoted_packages() {
        let packages = parse_inputs_filtered(
            "\"ChIPseeker\", \"clusterProfiler\", \"TxDb.Hsapiens.UCSC.hg38.knownGene\"",
            &InputRules::default(),
        )
        .expect("逗号分隔引用包名应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "ChIPseeker");
        assert_eq!(packages[1].name, "clusterProfiler");
        assert_eq!(packages[2].name, "TxDb.Hsapiens.UCSC.hg38.knownGene");
    }

    #[test]
    fn parses_semicolon_separated_packages() {
        let packages =
            parse_inputs_filtered("dplyr; ggplot2; tidyr; shiny", &InputRules::default())
                .expect("分号分隔包名应可解析");
        assert_eq!(packages.len(), 4);
        assert_eq!(packages[0].name, "dplyr");
        assert_eq!(packages[2].name, "tidyr");
    }

    #[test]
    fn parses_r_c_vector_syntax() {
        let packages = parse_inputs_filtered(
            "c(\"Seurat\", \"dplyr\", \"ggplot2\")",
            &InputRules::default(),
        )
        .expect("R c() 向量应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "Seurat");
        assert_eq!(packages[1].name, "dplyr");
        assert_eq!(packages[2].name, "ggplot2");
    }

    #[test]
    fn parses_mixed_separator_lines() {
        let packages =
            parse_inputs_filtered("pkg1; pkg2, pkg3\npkg4, pkg5", &InputRules::default())
                .expect("混合分隔符多行输入应可解析");
        assert_eq!(packages.len(), 5);
        assert_eq!(packages[0].name, "pkg1");
        assert_eq!(packages[3].name, "pkg4");
    }

    #[test]
    fn parses_comma_separated_with_versions() {
        let packages =
            parse_inputs_filtered("GSVA 1.50.0, dplyr 1.0.0, ggplot2", &InputRules::default())
                .expect("逗号分隔带版本应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "GSVA");
        assert_eq!(packages[0].version, "1.50.0");
        assert_eq!(packages[1].name, "dplyr");
        assert_eq!(packages[1].version, "1.0.0");
        assert_eq!(packages[2].name, "ggplot2");
        assert_eq!(packages[2].version, "");
    }

    #[test]
    fn parses_list_variant_syntax() {
        let packages = parse_inputs_filtered("list(\"pkg1\", \"pkg2\")", &InputRules::default())
            .expect("list() 包裹应可解析");
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "pkg1");
        assert_eq!(packages[1].name, "pkg2");
    }

    #[test]
    fn parses_url_lines_bypass_separator_splitting() {
        let packages = parse_inputs_filtered(
            "https://example.org/src/contrib/demo_1.0.0.tar.gz",
            &InputRules::default(),
        )
        .expect("URL 行应保持完整");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "demo");
    }

    #[test]
    fn parses_markdown_package_source_table() {
        let input = r#"| 包名                | 来源        |
| ----------------- | --------- |
| DMwR              | CRAN（已归档） |
| kernelshap        | CRAN      |
| extraTrees        | CRAN      |
| DALEX             | CRAN      |
| ResourceSelection | CRAN      |
| DynNom            | CRAN      |
| shapper           | CRAN      |
| iml               | CRAN      |
| naivebayes        | CRAN      |
| ingredients       | CRAN      |
| adabag            | CRAN      |
| mice              | CRAN      |
| autoReg           | CRAN      |
| cvms              | CRAN      |
| ROSE              | CRAN      |
| CBCgrps           | CRAN      |"#;

        let packages = parse_inputs_filtered(input, &InputRules::default())
            .expect("Markdown 表格应可解析为包名列表");
        let names = packages
            .iter()
            .map(|pkg| pkg.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names.len(), 16);
        assert_eq!(names[0], "DMwR");
        assert_eq!(names[15], "CBCgrps");
        assert!(packages
            .iter()
            .all(|pkg| pkg.source_hint.as_deref() == Some("cran")));
    }

    #[test]
    fn test_strip_r_parens_wrapper() {
        assert_eq!(
            strip_r_parens_wrapper("c(\"pkg1\", \"pkg2\")"),
            "\"pkg1\", \"pkg2\""
        );
        assert_eq!(strip_r_parens_wrapper("list(\"pkg1\")"), "\"pkg1\"");
        assert_eq!(strip_r_parens_wrapper("plain_line"), "plain_line");
    }

    #[test]
    fn parses_space_separated_with_split_spaces_enabled() {
        let rules = InputRules {
            split_spaces: true,
            separators: Vec::new(),
            ..InputRules::default()
        };
        let packages = parse_inputs_filtered("pkg1 pkg2 pkg3", &rules)
            .expect("空格分隔 (split_spaces=true) 应可解析");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "pkg1");
        assert_eq!(packages[2].name, "pkg3");
    }

    #[test]
    fn parses_complex_real_world_input() {
        let input = "c(\"ChIPseeker\", \"clusterProfiler\", \"TxDb.Hsapiens.UCSC.hg38.knownGene\", \"org.Hs.eg.db\", \"enrichplot\")";
        let packages = parse_inputs_filtered(input, &InputRules::default())
            .expect("真实世界 R c() 输入应可解析");
        assert_eq!(packages.len(), 5);
        assert_eq!(packages[0].name, "ChIPseeker");
        assert_eq!(packages[4].name, "enrichplot");
    }

    #[test]
    fn parses_package_manager_and_full_width_separator_input() {
        let packages = parse_inputs_filtered(
            "pacman::p_load(dplyr，ggplot2)\nrenv::install(c(\"Seurat\", \"patchwork\"))",
            &InputRules::default(),
        )
        .expect("包管理器和中文分隔符输入应可解析");
        let names = packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["dplyr", "ggplot2", "Seurat", "patchwork"]);
    }

    #[test]
    fn parses_docker_run_rscript_input() {
        let packages = parse_inputs_filtered(
            "RUN Rscript -e 'install.packages(c(\"dplyr\", \"tidyr\"))'",
            &InputRules::default(),
        )
        .expect("Docker RUN Rscript 输入应可解析");
        assert_eq!(
            packages
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>(),
            ["dplyr", "tidyr"]
        );
    }

    #[test]
    fn parses_reverse_dependencies_from_cran_html() {
        let html = r#"<table summary="Reverse depends for dplyr">
<tr><td>Reverse depends:</td><td><a href="../.../">250</a></td></tr>
<tr><td>Reverse imports:</td><td><a href="../.../">1856</a></td></tr>
<tr><td>Reverse suggests:</td><td><a href="../.../">125</a></td></tr>
<tr><td>Reverse linking to:</td><td><a href="../.../">42</a></td></tr>
</table>"#;

        let info = parse_reverse_dependencies(html, "dplyr").expect("应可解析反向依赖");
        assert_eq!(info.depends, 250);
        assert_eq!(info.imports, 1856);
        assert_eq!(info.suggests, 125);
        assert_eq!(info.linking_to, 42);
    }

    #[test]
    fn parses_reverse_dependencies_partial_fields() {
        let html =
            r##"<table><tr><td>Reverse imports:</td><td><a href="#">3</a></td></tr></table>"##;
        let info = parse_reverse_dependencies(html, "pkg").expect("部分字段应可解析");
        assert_eq!(info.imports, 3);
        assert_eq!(info.depends, 0);
        assert_eq!(info.suggests, 0);
        assert_eq!(info.linking_to, 0);
    }

    #[test]
    fn reverse_dependencies_returns_none_for_no_match() {
        assert!(parse_reverse_dependencies("no reverse data here", "pkg").is_none());
    }

    #[test]
    fn generates_verify_script_with_correct_package_names() {
        let packages = vec![
            PackageInput {
                raw: "dplyr".to_string(),
                name: "dplyr".to_string(),
                version: String::new(),
                source_hint: None,
            },
            PackageInput {
                raw: "owner/demo".to_string(),
                name: "owner/demo".to_string(),
                version: String::new(),
                source_hint: None,
            },
        ];
        let verify = generate_verify_script(&packages);
        assert!(verify.contains("\"dplyr\""));
        assert!(verify.contains("\"demo\""));
        assert!(!verify.contains("owner/demo"));
        assert!(verify.contains("packageVersion(p)"));
        assert!(verify.contains("[OK]"));
        assert!(verify.contains("[FAIL]"));
        assert!(verify.contains("验证完成"));
    }

    #[test]
    fn append_verify_in_generated_script() {
        let output = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                append_verify: true,
                ..Default::default()
            },
            &[],
        )
        .expect("应生成带验证的脚本");

        assert!(output.contains("install.packages(\"dplyr\""));
        assert!(output.contains("# ===== 安装结果验证 ====="));
        assert!(output.contains("packageVersion(p)"));
    }

    #[test]
    fn rspm_mirror_generates_binary_install_setup() {
        let output = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://packagemanager.posit.co/cran/latest".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("RSPM 应生成安装脚本");

        assert!(output
            .contains("options(pkgType = if (.Platform$OS.type == \"windows\") \"win.binary\""));
        assert!(output.contains("else \"source\")"));
        assert!(output.contains(
            "install.packages(\"dplyr\", repos = \"https://packagemanager.posit.co/cran/latest/\""
        ));
    }

    #[test]
    fn ordinary_cran_mirror_does_not_force_binary_package_type() {
        let output = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                mirror: "https://cloud.r-project.org".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("普通 CRAN 镜像应生成安装脚本");

        assert!(!output.contains("pkgType = \"binary\""));
    }

    #[test]
    fn rspm_script_uses_writable_user_library() {
        let output = generate_script(
            "seurat",
            &GenerateOptions {
                method: "base".to_string(),
                mirror: "https://packagemanager.posit.co/cran/latest".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("RSPM 应生成用户库初始化代码");

        assert!(
            output.contains("user_library <- file.path(path.expand(\"~\"), \"R\", \"library\")")
        );
        assert!(output.contains(".libPaths(unique(c(user_library, .libPaths())))"));
        assert!(!output.contains("Would you like to use a personal library"));
    }
}
