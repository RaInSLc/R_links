#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use crate::logic::*;
    use crate::models::{
        GenerateOptions, InputRules, PackageInput, SearchResult, MAX_FIELD_CHARS, MAX_INPUT_CHARS,
        MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS,
    };

    #[test]
    fn custom_r_library_path_is_used_by_generated_script() {
        let script = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                conditional: false,
                install_dependencies: true,
                mirror: "https://cloud.r-project.org".to_string(),
                r_lib_path: "D:/R/project-library".to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("自定义 R 库路径应生成脚本");
        assert!(script.contains("lib = \"D:/R/project-library\""));
    }

    #[test]
    fn custom_r_library_path_is_used_by_all_r_install_sources() {
        let library = "D:/R/project-library";
        for (input, method, version) in [
            ("owner/repository", "github", ""),
            ("edgeR", "biocGit", "|3.18"),
            ("Rcmdr", "rForge", ""),
        ] {
            let script =
                generate_command_with_lib(input, method, version, false, "", true, library)
                    .expect("所有 R 安装来源均应支持自定义库路径");
            assert!(
                script.contains("lib = \"D:/R/project-library\""),
                "{method} 未写入自定义库路径: {script}"
            );
        }

        let local_script = generate_script(
            r"C:\packages\example_1.0.0.tar.gz",
            &GenerateOptions {
                method: "auto".to_string(),
                conditional: false,
                r_lib_path: library.to_string(),
                ..Default::default()
            },
            &[],
        )
        .expect("本地归档安装应支持自定义库路径");
        assert!(local_script.contains("lib = \"D:/R/project-library\""));
    }

    #[test]
    fn parallel_install_adds_ncpus_option() {
        let script = generate_script(
            "dplyr",
            &GenerateOptions {
                method: "base".to_string(),
                parallel_install: true,
                ..Default::default()
            },
            &[],
        )
        .expect("应生成脚本");
        assert!(script.contains("options(Ncpus = parallel::detectCores())"));
    }

    #[test]
    fn builds_pip_package_page_url() {
        let url = build_package_page_url("numpy", "pip", "").expect("应生成 PyPI URL");
        assert_eq!(url, "https://pypi.org/project/numpy/");
        assert!(is_allowed_package_page_url(&url));
    }

    #[test]
    fn builds_conda_package_page_url() {
        let url =
            build_package_page_url("numpy", "conda", "conda-forge").expect("应生成 Conda URL");
        assert_eq!(url, "https://anaconda.org/conda-forge/numpy");
        assert!(is_allowed_package_page_url(&url));
    }

    #[test]
    fn rejects_conda_page_url_with_invalid_channel() {
        assert!(build_package_page_url("numpy", "conda", "").is_err());
        assert!(build_package_page_url("numpy", "conda", "evil/path").is_err());
    }

    #[test]
    fn builds_cran_package_page_url_and_validator_accepts_it() {
        let url = build_package_page_url("ggplot2", "cran", "").expect("应生成 CRAN 包页面 URL");
        assert_eq!(
            url,
            "https://cran.r-project.org/web/packages/ggplot2/index.html"
        );
        assert!(is_allowed_package_page_url(&url));
    }

    #[test]
    fn rejects_non_canonical_cran_package_page_urls() {
        // `/package=xxx` 只是 302 快捷写法；`/package/xxx` 在 CRAN 上返回 404。
        assert!(!is_allowed_package_page_url(
            "https://cran.r-project.org/package=ggplot2"
        ));
        assert!(!is_allowed_package_page_url(
            "https://cran.r-project.org/package/ggplot2"
        ));
        // 同主机下的其它路径同样不允许。
        assert!(!is_allowed_package_page_url(
            "https://cran.r-project.org/web/packages/ggplot2/NEWS"
        ));
        assert!(!is_allowed_package_page_url(
            "https://cran.r-project.org/web/packages/../index.html"
        ));
    }

    #[test]
    fn every_built_package_page_url_passes_its_own_validator() {
        // 回归：构建器与校验器必须对每一种来源保持闭环，任一侧单独调整都会在此暴露。
        for (package, source, repository) in [
            ("ggplot2", "cran", ""),
            ("edgeR", "bioc", ""),
            ("tidyverse/dplyr", "github", "tidyverse/dplyr"),
            ("Rcmdr", "r-forge", ""),
            ("numpy", "pip", ""),
            ("numpy", "conda", "conda-forge"),
        ] {
            let url = build_package_page_url(package, source, repository)
                .unwrap_or_else(|error| panic!("{source} 页面 URL 构建失败: {error}"));
            assert!(
                is_allowed_package_page_url(&url),
                "{source} 生成的 URL 未通过自身校验: {url}"
            );
        }
    }

    #[test]
    fn accepts_uppercase_http_scheme_consistently_with_frontend() {
        // 前端 `utils-url.ts` 用 `/^https?:\/\//i` 判定，因此后端必须同样宽松，
        // 否则会出现"工作台算作合法 URL 并把方法切到 github、点检索却被整批拒绝"的割裂。
        for value in [
            "HTTPS://github.com/tidyverse/dplyr",
            "Http://cran.r-project.org/src/contrib/Archive/pkg/pkg_1.0.tar.gz",
            "hTtPs://cloud.r-project.org",
        ] {
            assert!(starts_with_http_scheme(value), "{value} 应被识别为 http(s)");
        }
        assert!(!starts_with_http_scheme("ftp://example.com"));
        assert!(!starts_with_http_scheme("http"));
        assert!(!starts_with_http_scheme("//example.com"));
    }

    #[test]
    fn parses_uppercase_scheme_inputs_end_to_end() {
        let github = parse_inputs("HTTPS://github.com/tidyverse/dplyr")
            .expect("大写协议头的 GitHub URL 应可解析");
        assert_eq!(github.len(), 1);
        assert_eq!(github[0].name, "tidyverse/dplyr");
        assert_eq!(github[0].source_hint.as_deref(), Some("github"));

        let archive =
            parse_inputs("Http://cran.r-project.org/src/contrib/Archive/pkg/pkg_1.0.tar.gz")
                .expect("大写协议头的归档 URL 应可解析");
        assert_eq!(archive.len(), 1);
        assert_eq!(archive[0].name, "pkg");
    }

    #[test]
    fn still_rejects_unknown_schemes() {
        assert!(parse_inputs("ftp://example.com/pkg.tar.gz").is_err());
        assert!(parse_inputs("HTTPS://evil.example.com/x").is_err());
    }

    #[test]
    fn rejects_disallowed_pip_conda_page_urls() {
        assert!(!is_allowed_package_page_url(
            "https://pypi.org/project/numpy/../../etc"
        ));
        assert!(!is_allowed_package_page_url(
            "https://anaconda.org/conda-forge/numpy?evil=1"
        ));
        assert!(!is_allowed_package_page_url(
            "https://evil.com/project/numpy"
        ));
    }
}
