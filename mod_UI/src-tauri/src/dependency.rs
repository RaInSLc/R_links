use crate::models::{
    DependencyEdge, DependencyGraph, DependencyNode, DependencySummary, SearchResult, Settings,
};
use crate::storage;
use futures_util::future::join_all;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

#[path = "dependency_fetch.rs"]
mod dependency_fetch;
#[path = "dependency_parse.rs"]
mod dependency_parse;

use dependency_fetch::fetch_description;
use dependency_parse::parse_package_dependencies;

#[derive(Clone)]
struct DependencyRequest {
    package: String,
    depth: usize,
    path_roots: Vec<String>,
    source: String,
    version: String,
    repository: String,
}

fn dependency_cache_key(package: &str, source: &str, repository: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        package.to_ascii_lowercase(),
        source.to_ascii_lowercase(),
        repository.to_ascii_lowercase(),
    )
}

fn merge_roots(target: &mut Vec<String>, roots: Vec<String>) {
    for root in roots {
        if !target.contains(&root) {
            target.push(root);
        }
    }
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
                let cache_key =
                    dependency_cache_key(&request.package, &request.source, &request.repository);
                let cache_entry = dep_cache.get(&cache_key).cloned();
                async move {
                    if let Some(entry) = cache_entry.filter(|entry| {
                        request.version.is_empty() || entry.version == request.version
                    }) {
                        return (
                            request.package,
                            request.depth,
                            request.path_roots,
                            request.source,
                            request.version,
                            request.repository,
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
                        request.version,
                        request.repository,
                        parsed,
                    )
                }
            })
            .collect();

        let results = join_all(futures).await;
        let mut new_cache_entries = HashMap::new();

        for (pkg, depth, path_roots, source, _requested_version, repository, parsed_res) in results
        {
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
                    let cache_key = dependency_cache_key(&pkg, &source, &repository);
                    if !dep_cache.contains_key(&cache_key) {
                        new_cache_entries.insert(
                            cache_key,
                            storage::DependencyCacheEntry {
                                heavy_deps: heavy_deps.clone(),
                                light_deps: light_deps.clone(),
                                version: version.clone(),
                                source: source.clone(),
                                repository: repository.clone(),
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
}
