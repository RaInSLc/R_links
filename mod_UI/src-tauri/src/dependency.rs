use crate::models::{
    DependencyEdge, DependencyGraph, DependencyNode, DependencySummary, SearchResult, Settings,
};
use crate::storage;
use futures_util::future::join_all;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

const CORE_PACKAGES: &[&str] = &[
    "R",
    "base",
    "compiler",
    "datasets",
    "grDevices",
    "graphics",
    "grid",
    "methods",
    "parallel",
    "splines",
    "stats",
    "stats4",
    "tcltk",
    "tools",
    "utils",
];
const BIOC_DEPENDENCY_CATEGORIES: &[&str] =
    &["bioc", "data/annotation", "data/experiment", "workflows"];

#[derive(Clone)]
struct DependencyRequest {
    package: String,
    depth: usize,
    path_roots: Vec<String>,
    source: String,
    version: String,
    repository: String,
}

fn merge_roots(target: &mut Vec<String>, roots: Vec<String>) {
    for root in roots {
        if !target.contains(&root) {
            target.push(root);
        }
    }
}

/// 解析 Debian control (RFC 822) 格式的 DESCRIPTION 文件
pub fn parse_description(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut current_key = String::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if !current_key.is_empty() {
                let val = map.entry(current_key.clone()).or_insert_with(String::new);
                if !val.is_empty() && !val.ends_with(' ') {
                    val.push(' ');
                }
                val.push_str(line.trim());
            }
        } else if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_string();
            let val = line[pos + 1..].trim().to_string();
            current_key = key.clone();
            map.insert(key, val);
        }
    }
    map
}

/// 清洗依赖包名并去除版本约束，如 "ggplot2 (>= 3.0.0)" -> "ggplot2"
fn clean_package_name(dep: &str) -> String {
    let dep = dep.trim();
    if let Some(pos) = dep.find('(') {
        dep[..pos].trim().to_string()
    } else {
        dep.to_string()
    }
}

/// 解析依赖字段（如 Depends, Imports, Suggests, LinkingTo）
fn parse_dependency_field(field_value: &str) -> Vec<String> {
    field_value
        .split(',')
        .map(clean_package_name)
        .filter(|name| !name.is_empty() && !CORE_PACKAGES.contains(&name.as_str()))
        .collect()
}

/// 发送请求获取包的 DESCRIPTION 文本
async fn fetch_description(
    client: &reqwest::Client,
    package: &str,
    source: &str,
    version: &str,
    repository: &str,
    mirror: &str,
) -> Result<String, String> {
    let mut urls: Vec<(String, bool)> = Vec::new();
    let mirror_clean = mirror.trim_end_matches('/');

    if source.eq_ignore_ascii_case("cran") || source.eq_ignore_ascii_case("none") {
        urls.push((
            format!("{}/web/packages/{}/DESCRIPTION", mirror_clean, package),
            false,
        ));
    } else if source.eq_ignore_ascii_case("bioc") || source.eq_ignore_ascii_case("biocGit") {
        let bioc_versions =
            if source.eq_ignore_ascii_case("biocGit") && !repository.trim().is_empty() {
                vec![
                    repository.trim().trim_matches('/').to_string(),
                    "release".to_string(),
                ]
            } else {
                vec!["release".to_string()]
            };
        for bioc_version in bioc_versions {
            for category in BIOC_DEPENDENCY_CATEGORIES {
                urls.push((format!(
                    "https://bioconductor.org/packages/{bioc_version}/{category}/src/contrib/PACKAGES",
                ), true));
            }
        }
    } else if source.eq_ignore_ascii_case("github") {
        let github_repo = if !repository.trim().is_empty() {
            repository.trim()
        } else if package.contains('/') {
            package
        } else {
            ""
        };
        if !github_repo.is_empty() {
            urls.push((
                format!(
                    "https://raw.githubusercontent.com/{}/master/DESCRIPTION",
                    github_repo
                ),
                false,
            ));
            urls.push((
                format!(
                    "https://raw.githubusercontent.com/{}/main/DESCRIPTION",
                    github_repo
                ),
                false,
            ));
        }
        urls.push((
            format!(
                "https://raw.githubusercontent.com/cran/{}/master/DESCRIPTION",
                package
            ),
            false,
        ));
    } else {
        urls.push((
            format!("{}/web/packages/{}/DESCRIPTION", mirror_clean, package),
            false,
        ));
    }

    for (url, is_packages_index) in urls {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(text) = resp.text().await {
                    if is_packages_index {
                        if let Some(entry) = extract_packages_index_entry(&text, package, version) {
                            return Ok(entry);
                        }
                    } else if !text.trim().is_empty() {
                        return Ok(text);
                    }
                }
            }
            _ => {}
        }
    }

    Err(format!("无法获取包 {} 的 DESCRIPTION 元数据", package))
}

fn extract_packages_index_entry(text: &str, package: &str, version: &str) -> Option<String> {
    for entry in text.split("\n\n") {
        let meta = parse_description(entry);
        let Some(entry_package) = meta.get("Package") else {
            continue;
        };
        if !entry_package.eq_ignore_ascii_case(package) {
            continue;
        }
        if !version.is_empty()
            && meta
                .get("Version")
                .is_some_and(|entry_version| entry_version != version)
        {
            continue;
        }
        return Some(entry.to_string());
    }
    None
}

fn enqueue_dependency(
    queue: &mut VecDeque<DependencyRequest>,
    visited: &mut HashSet<String>,
    pending_roots: &mut HashMap<String, Vec<String>>,
    nodes_map: &mut HashMap<String, DependencyNode>,
    request: DependencyRequest,
) {
    if let Some(node) = nodes_map.get_mut(&request.package) {
        merge_roots(&mut node.root_packages, request.path_roots);
        return;
    }
    if let Some(queued) = queue
        .iter_mut()
        .find(|queued| queued.package == request.package)
    {
        merge_roots(&mut queued.path_roots, request.path_roots);
        return;
    }
    if !visited.insert(request.package.clone()) {
        let roots = pending_roots.entry(request.package).or_default();
        merge_roots(roots, request.path_roots);
        return;
    }
    queue.push_back(request);
}

/// 解析单包的依赖项，返回 (heavy_deps, light_deps, version)
fn parse_package_dependencies(content: &str) -> (Vec<String>, Vec<String>, String) {
    let meta = parse_description(content);
    let mut heavy_deps = Vec::new();
    let mut light_deps = Vec::new();
    let version = meta.get("Version").cloned().unwrap_or_default();

    if let Some(depends) = meta.get("Depends") {
        heavy_deps.extend(parse_dependency_field(depends));
    }
    if let Some(imports) = meta.get("Imports") {
        heavy_deps.extend(parse_dependency_field(imports));
    }
    if let Some(linking_to) = meta.get("LinkingTo") {
        heavy_deps.extend(parse_dependency_field(linking_to));
    }
    if let Some(suggests) = meta.get("Suggests") {
        light_deps.extend(parse_dependency_field(suggests));
    }

    heavy_deps.sort();
    heavy_deps.dedup();
    light_deps.sort();
    light_deps.dedup();

    (heavy_deps, light_deps, version)
}

/// 拓扑依赖解析主入口
pub async fn resolve_dependencies(
    app: &AppHandle,
    client: &reqwest::Client,
    root_results: &[SearchResult],
    settings: &Settings,
    cancelled: &AtomicBool,
) -> Result<DependencyGraph, String> {
    let mut roots = Vec::new();
    let mut nodes_map: HashMap<String, DependencyNode> = HashMap::new();
    let mut edges: Vec<DependencyEdge> = Vec::new();

    let mut dep_cache = if settings.use_cache {
        storage::load_dependency_cache(app).unwrap_or_default()
    } else {
        HashMap::new()
    };

    let mut root_sources = HashMap::new();
    let mut root_versions = HashMap::new();
    let mut root_repositories = HashMap::new();
    for res in root_results {
        if res.found {
            roots.push(res.package.clone());
            root_sources.insert(res.package.clone(), res.source.clone());
            root_versions.insert(res.package.clone(), res.latest_version.clone());
            root_repositories.insert(res.package.clone(), res.repository.clone());
        }
    }

    if roots.is_empty() {
        return Ok(DependencyGraph {
            roots: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            summary: DependencySummary {
                total_nodes: 0,
                total_edges: 0,
                heavy_nodes: 0,
                light_nodes: 0,
                shared_nodes: 0,
            },
        });
    }

    let mut queue: VecDeque<DependencyRequest> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut pending_roots: HashMap<String, Vec<String>> = HashMap::new();

    for r in &roots {
        queue.push_back(DependencyRequest {
            package: r.clone(),
            depth: 0,
            path_roots: vec![r.clone()],
            source: root_sources
                .get(r)
                .cloned()
                .unwrap_or_else(|| "none".to_string()),
            version: root_versions.get(r).cloned().unwrap_or_default(),
            repository: root_repositories.get(r).cloned().unwrap_or_default(),
        });
        visited.insert(r.clone());
    }

    while !queue.is_empty() && !cancelled.load(Ordering::SeqCst) {
        if nodes_map.len() >= settings.max_dependency_nodes {
            break;
        }

        let level_size = queue.len();
        let mut level_tasks = Vec::new();

        for _ in 0..level_size {
            if let Some(mut request) = queue.pop_front() {
                if let Some(extra_roots) = pending_roots.remove(&request.package) {
                    merge_roots(&mut request.path_roots, extra_roots);
                }
                let pkg = request.package.clone();
                if let Some(existing_node) = nodes_map.get_mut(&pkg) {
                    merge_roots(&mut existing_node.root_packages, request.path_roots);
                    continue;
                }
                level_tasks.push(request);
            }
        }

        if level_tasks.is_empty() {
            continue;
        }

        let futures: Vec<_> = level_tasks
            .into_iter()
            .map(|request| {
                let client_clone = client.clone();
                let mirror = settings.cran_mirror.clone();
                let cache_entry = dep_cache.get(&request.package).cloned();
                async move {
                    if let Some(entry) = cache_entry {
                        return (
                            request.package,
                            request.depth,
                            request.path_roots,
                            request.source,
                            Ok((entry.heavy_deps, entry.light_deps, entry.version)),
                        );
                    }

                    let fetch_result = fetch_description(
                        &client_clone,
                        &request.package,
                        &request.source,
                        &request.version,
                        &request.repository,
                        &mirror,
                    )
                    .await;
                    let parsed = fetch_result.map(|content| parse_package_dependencies(&content));
                    (
                        request.package,
                        request.depth,
                        request.path_roots,
                        request.source,
                        parsed,
                    )
                }
            })
            .collect();

        let results = join_all(futures).await;
        let mut new_cache_entries = HashMap::new();

        for (pkg, depth, path_roots, source, parsed_res) in results {
            if cancelled.load(Ordering::SeqCst) {
                break;
            }
            if nodes_map.len() >= settings.max_dependency_nodes {
                break;
            }

            let mut path_roots = path_roots;
            if let Some(extra_roots) = pending_roots.remove(&pkg) {
                merge_roots(&mut path_roots, extra_roots);
            }

            match parsed_res {
                Ok((heavy_deps, light_deps, version)) => {
                    if !dep_cache.contains_key(&pkg) {
                        new_cache_entries.insert(
                            pkg.clone(),
                            storage::DependencyCacheEntry {
                                heavy_deps: heavy_deps.clone(),
                                light_deps: light_deps.clone(),
                                version: version.clone(),
                            },
                        );
                    }

                    let node = DependencyNode {
                        package: pkg.clone(),
                        source: source.clone(),
                        version: if version.is_empty() {
                            "unknown".to_string()
                        } else {
                            version
                        },
                        depth,
                        root_packages: path_roots.clone(),
                        direct_dependency_count: heavy_deps.len() + light_deps.len(),
                        heavy_dependency_count: heavy_deps.len(),
                        status: "resolved".to_string(),
                    };
                    nodes_map.insert(pkg.clone(), node);

                    if depth < settings.max_dependency_depth {
                        for heavy in heavy_deps {
                            edges.push(DependencyEdge {
                                from: pkg.clone(),
                                to: heavy.clone(),
                                relation: "Imports".to_string(),
                                strength: "heavy".to_string(),
                                depth: depth + 1,
                            });

                            enqueue_dependency(
                                &mut queue,
                                &mut visited,
                                &mut pending_roots,
                                &mut nodes_map,
                                DependencyRequest {
                                    package: heavy,
                                    depth: depth + 1,
                                    path_roots: path_roots.clone(),
                                    source: "none".to_string(),
                                    version: String::new(),
                                    repository: String::new(),
                                },
                            );
                        }

                        if settings.include_light_dependencies {
                            for light in light_deps {
                                edges.push(DependencyEdge {
                                    from: pkg.clone(),
                                    to: light.clone(),
                                    relation: "Suggests".to_string(),
                                    strength: "light".to_string(),
                                    depth: depth + 1,
                                });

                                enqueue_dependency(
                                    &mut queue,
                                    &mut visited,
                                    &mut pending_roots,
                                    &mut nodes_map,
                                    DependencyRequest {
                                        package: light,
                                        depth: settings.max_dependency_depth,
                                        path_roots: path_roots.clone(),
                                        source: "none".to_string(),
                                        version: String::new(),
                                        repository: String::new(),
                                    },
                                );
                            }
                        }
                    }
                }
                Err(_err) => {
                    let node = DependencyNode {
                        package: pkg.clone(),
                        source: source.clone(),
                        version: "unknown".to_string(),
                        depth,
                        root_packages: path_roots.clone(),
                        direct_dependency_count: 0,
                        heavy_dependency_count: 0,
                        status: "unresolved".to_string(),
                    };
                    nodes_map.insert(pkg.clone(), node);
                }
            }
        }

        if !new_cache_entries.is_empty() && settings.use_cache {
            for (k, v) in new_cache_entries {
                dep_cache.insert(k, v);
            }
            let _ = storage::save_dependency_cache(app, &dep_cache);
        }
    }

    let total_nodes = nodes_map.len();
    let total_edges = edges.len();
    let mut heavy_nodes = 0;
    let mut light_nodes = 0;
    let mut shared_nodes = 0;

    for node in nodes_map.values() {
        if node.root_packages.len() > 1 {
            shared_nodes += 1;
        }
    }

    let light_set: HashSet<String> = edges
        .iter()
        .filter(|e| e.strength == "light")
        .map(|e| e.to.clone())
        .collect();

    for pkg in nodes_map.keys() {
        if roots.contains(pkg) {
            heavy_nodes += 1;
        } else if light_set.contains(pkg) {
            light_nodes += 1;
        } else {
            heavy_nodes += 1;
        }
    }

    Ok(DependencyGraph {
        roots,
        nodes: nodes_map.into_values().collect(),
        edges,
        summary: DependencySummary {
            total_nodes,
            total_edges,
            heavy_nodes,
            light_nodes,
            shared_nodes,
        },
    })
}

#[cfg(test)]
mod tests {
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
}
