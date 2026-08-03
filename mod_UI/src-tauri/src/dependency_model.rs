use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraph {
    pub roots: Vec<String>,
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    pub summary: DependencySummary,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencyNode {
    pub package: String,
    pub source: String,
    pub version: String,
    pub depth: usize,
    pub root_packages: Vec<String>,
    pub direct_dependency_count: usize,
    pub heavy_dependency_count: usize,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub strength: String,
    pub depth: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DependencySummary {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub heavy_nodes: usize,
    pub light_nodes: usize,
    pub shared_nodes: usize,
}
