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
            let script = generate_command_with_lib(input, method, version, false, "", true, library)
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
