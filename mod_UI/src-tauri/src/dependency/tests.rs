use super::dependency_fetch::extract_packages_index_entry;
use super::dependency_parse::{clean_package_name, parse_description};
use super::*;

#[test]
fn test_parse_description_debian_control() {
    let content = "\
Package: Seurat
Version: 5.1.0
Depends:
    R (>= 4.0.0),
    SeuratObject (>= 5.0.1)
Imports:
    fitdistrplus,
    ggplot2 (>= 3.0.0)
Suggests:
    ape,
    future
";
    let meta = parse_description(content);
    assert_eq!(meta.get("package").unwrap(), "Seurat");
    assert_eq!(meta.get("version").unwrap(), "5.1.0");
    assert_eq!(
        meta.get("depends").unwrap(),
        "R (>= 4.0.0), SeuratObject (>= 5.0.1)"
    );
    assert_eq!(
        meta.get("imports").unwrap(),
        "fitdistrplus, ggplot2 (>= 3.0.0)"
    );
}

#[test]
fn description_field_names_are_case_insensitive() {
    let content = "\
package: demo
version: 1.0.0
imports: rlang,
    cli
LINKINGTO: Rcpp
";

    let meta = parse_description(content);
    assert_eq!(meta.get("package").unwrap(), "demo");
    assert_eq!(meta.get("version").unwrap(), "1.0.0");
    assert_eq!(meta.get("imports").unwrap(), "rlang, cli");
    assert_eq!(meta.get("linkingto").unwrap(), "Rcpp");

    let (heavy, _light, version) = parse_package_dependencies(content);
    assert_eq!(version, "1.0.0");
    assert!(heavy.contains(&"rlang".to_string()));
    assert!(heavy.contains(&"cli".to_string()));
    assert!(heavy.contains(&"Rcpp".to_string()));
}

#[test]
fn dependency_metadata_urls_pass_search_url_validation() {
    use super::dependency_fetch::build_dependency_urls;
    use crate::search_urls::validate_search_request_url_with_mirror;

    let mirror = "https://cloud.r-project.org";
    let cases = [
        ("dplyr", "cran", "", mirror),
        ("GSVA", "bioc", "", mirror),
        ("GSVA", "biocGit", "3.18", mirror),
        ("owner/repo", "github", "owner/repo", mirror),
        ("dplyr", "none", "", mirror),
    ];

    for (package, source, repository, mirror) in cases {
        let urls = build_dependency_urls(package, source, repository, mirror);
        assert!(!urls.is_empty(), "来源 {source} 应构造出候选依赖元数据 URL");
        for (url, _) in urls {
            assert!(
                validate_search_request_url_with_mirror(&url, Some(mirror)).is_ok(),
                "{source} 构造的 URL 未通过校验: {url}"
            );
        }
    }
}

#[test]
fn dependency_metadata_urls_drop_invalid_github_repository() {
    use super::dependency_fetch::build_dependency_urls;

    // 非法的仓库路径不应出现在候选 URL 中。
    let urls = build_dependency_urls("demo", "github", "owner/repo/../../etc", "");
    assert!(urls.iter().all(|(url, _)| !url.contains("..")));
}

#[test]
fn test_clean_package_name() {
    assert_eq!(clean_package_name("ggplot2 (>= 3.0.0)"), "ggplot2");
    assert_eq!(
        clean_package_name(" SeuratObject   (>= 5.0.1) "),
        "SeuratObject"
    );
    assert_eq!(clean_package_name("Matrix"), "Matrix");
}

#[test]
fn test_parse_package_dependencies() {
    let content = "\
Package: Seurat
Version: 5.1.0
Depends:
    R (>= 4.0.0),
    SeuratObject (>= 5.0.1)
Imports:
    ggplot2 (>= 3.0.0)
Suggests:
    future
";
    let (heavy, light, version) = parse_package_dependencies(content);
    assert_eq!(version, "5.1.0");
    assert!(heavy.contains(&"SeuratObject".to_string()));
    assert!(heavy.contains(&"ggplot2".to_string()));
    assert!(!heavy.contains(&"R".to_string()));
    assert!(light.contains(&"future".to_string()));
}

#[test]
fn extracts_matching_bioconductor_packages_entry() {
    let content = "\
Package: Other
Version: 1.0.0
Imports: wrong

Package: GSVA
Version: 1.52.0
Imports: BiocGenerics, matrixStats
Suggests: knitr
";

    let entry = extract_packages_index_entry(content, "GSVA", "1.52.0")
        .expect("应提取匹配包的 PACKAGES 条目");

    assert!(entry.contains("Package: GSVA"));
    assert!(entry.contains("Imports: BiocGenerics, matrixStats"));
}

#[test]
fn enqueue_dependency_merges_roots_for_queued_package() {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut pending_roots = HashMap::new();
    let mut nodes_map = HashMap::new();

    enqueue_dependency(
        &mut queue,
        &mut visited,
        &mut pending_roots,
        &mut nodes_map,
        DependencyRequest {
            package: "shared".to_string(),
            depth: 1,
            path_roots: vec!["root_a".to_string()],
            source: "none".to_string(),
            version: String::new(),
            repository: String::new(),
        },
    );
    enqueue_dependency(
        &mut queue,
        &mut visited,
        &mut pending_roots,
        &mut nodes_map,
        DependencyRequest {
            package: "shared".to_string(),
            depth: 1,
            path_roots: vec!["root_b".to_string()],
            source: "none".to_string(),
            version: String::new(),
            repository: String::new(),
        },
    );

    let request = queue.pop_front().expect("共享依赖应只入队一次");
    assert_eq!(request.path_roots, vec!["root_a", "root_b"]);
    assert!(pending_roots.is_empty());
}
