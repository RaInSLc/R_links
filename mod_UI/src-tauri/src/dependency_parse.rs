use std::collections::HashMap;

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
pub(crate) fn parse_description(content: &str) -> HashMap<String, String> {
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
            // 字段名统一小写归一化，兼容 `Imports` / `imports` / `IMPORTS` 等写法。
            let key = line[..pos].trim().to_ascii_lowercase();
            let val = line[pos + 1..].trim().to_string();
            current_key = key.clone();
            map.insert(key, val);
        }
    }
    map
}

/// 清洗依赖包名并去除版本约束，如 "ggplot2 (>= 3.0.0)" -> "ggplot2"
pub(crate) fn clean_package_name(dep: &str) -> String {
    let dep = dep.trim();
    if let Some(pos) = dep.find('(') {
        dep[..pos].trim().to_string()
    } else {
        dep.to_string()
    }
}

/// 解析依赖字段（如 Depends, Imports, Suggests, LinkingTo）
pub(crate) fn parse_dependency_field(field_value: &str) -> Vec<String> {
    field_value
        .split(',')
        .map(clean_package_name)
        .filter(|name| !name.is_empty() && !CORE_PACKAGES.contains(&name.as_str()))
        .collect()
}

/// 解析单包的依赖项，返回 (heavy_deps, light_deps, version)
pub(crate) fn parse_package_dependencies(content: &str) -> (Vec<String>, Vec<String>, String) {
    let meta = parse_description(content);
    let mut heavy_deps = Vec::new();
    let mut light_deps = Vec::new();
    let version = meta.get("version").cloned().unwrap_or_default();

    if let Some(depends) = meta.get("depends") {
        heavy_deps.extend(parse_dependency_field(depends));
    }
    if let Some(imports) = meta.get("imports") {
        heavy_deps.extend(parse_dependency_field(imports));
    }
    if let Some(linking_to) = meta.get("linkingto") {
        heavy_deps.extend(parse_dependency_field(linking_to));
    }
    if let Some(suggests) = meta.get("suggests") {
        light_deps.extend(parse_dependency_field(suggests));
    }

    heavy_deps.sort();
    heavy_deps.dedup();
    light_deps.sort();
    light_deps.dedup();
    (heavy_deps, light_deps, version)
}
