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
    assert_eq!(meta.get("Package").unwrap(), "Seurat");
    assert_eq!(meta.get("Version").unwrap(), "5.1.0");
    assert_eq!(
        meta.get("Depends").unwrap(),
        "R (>= 4.0.0), SeuratObject (>= 5.0.1)"
    );
    assert_eq!(
        meta.get("Imports").unwrap(),
        "fitdistrplus, ggplot2 (>= 3.0.0)"
    );
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
