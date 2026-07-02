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
    mirror: &str,
) -> Result<String, String> {
    let mut urls = Vec::new();
    let mirror_clean = mirror.trim_end_matches('/');

    if source.eq_ignore_ascii_case("cran") || source.eq_ignore_ascii_case("none") {
        urls.push(format!(
            "{}/web/packages/{}/DESCRIPTION",
            mirror_clean, package
        ));
    } else if source.eq_ignore_ascii_case("bioc") || source.eq_ignore_ascii_case("biocGit") {
        urls.push(format!(
            "https://raw.githubusercontent.com/bioconductor-packages/{}/master/DESCRIPTION",
            package
        ));
        urls.push(format!(
            "https://raw.githubusercontent.com/bioconductor/{}/master/DESCRIPTION",
            package
        ));
        urls.push(format!(
            "{}/web/packages/{}/DESCRIPTION",
            mirror_clean, package
        ));
    } else if source.eq_ignore_ascii_case("github") {
        if package.contains('/') {
            urls.push(format!(
                "https://raw.githubusercontent.com/{}/master/DESCRIPTION",
                package
            ));
            urls.push(format!(
                "https://raw.githubusercontent.com/{}/main/DESCRIPTION",
                package
            ));
        } else {
            urls.push(format!(
                "https://raw.githubusercontent.com/cran/{}/master/DESCRIPTION",
                package
            ));
        }
    } else {
        urls.push(format!(
            "{}/web/packages/{}/DESCRIPTION",
            mirror_clean, package
        ));
    }

    for url in urls {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(text) = resp.text().await {
                    if !text.trim().is_empty() {
                        return Ok(text);
                    }
                }
            }
            _ => {}
        }
    }

    Err(format!("无法获取包 {} 的 DESCRIPTION 元数据", package))
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
    for res in root_results {
        if res.found {
            roots.push(res.package.clone());
            root_sources.insert(res.package.clone(), res.source.clone());
            root_versions.insert(res.package.clone(), res.latest_version.clone());
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

    let mut queue: VecDeque<(String, usize, Vec<String>)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();

    for r in &roots {
        queue.push_back((r.clone(), 0, vec![r.clone()]));
        visited.insert(r.clone());
    }

    while !queue.is_empty() && !cancelled.load(Ordering::SeqCst) {
        if nodes_map.len() >= settings.max_dependency_nodes {
            break;
        }

        let level_size = queue.len();
        let mut level_tasks = Vec::new();

        for _ in 0..level_size {
            if let Some((pkg, depth, path_roots)) = queue.pop_front() {
                if let Some(existing_node) = nodes_map.get_mut(&pkg) {
                    for r in path_roots {
                        if !existing_node.root_packages.contains(&r) {
                            existing_node.root_packages.push(r);
                        }
                    }
                    continue;
                }

                let source = root_sources
                    .get(&pkg)
                    .cloned()
                    .unwrap_or_else(|| "none".to_string());
                let version = root_versions.get(&pkg).cloned().unwrap_or_default();

                level_tasks.push((pkg, depth, path_roots, source, version));
            }
        }

        if level_tasks.is_empty() {
            continue;
        }

        let futures: Vec<_> = level_tasks
            .into_iter()
            .map(|(pkg, depth, path_roots, source, _version)| {
                let client_clone = client.clone();
                let mirror = settings.cran_mirror.clone();
                let cache_entry = dep_cache.get(&pkg).cloned();
                async move {
                    if let Some(entry) = cache_entry {
                        return (
                            pkg,
                            depth,
                            path_roots,
                            source,
                            Ok((entry.heavy_deps, entry.light_deps, entry.version)),
                        );
                    }

                    let fetch_result =
                        fetch_description(&client_clone, &pkg, &source, &mirror).await;
                    let parsed = fetch_result.map(|content| parse_package_dependencies(&content));
                    (pkg, depth, path_roots, source, parsed)
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

                            if !visited.contains(&heavy) {
                                visited.insert(heavy.clone());
                                queue.push_back((heavy, depth + 1, path_roots.clone()));
                            }
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

                                if !visited.contains(&light) {
                                    visited.insert(light.clone());
                                    queue.push_back((
                                        light,
                                        settings.max_dependency_depth,
                                        path_roots.clone(),
                                    ));
                                }
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
}
