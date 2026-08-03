use serde::{Deserialize, Serialize};

use crate::models::DependencyGraph;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorSpeedResult {
    pub mirror: String,
    pub label: String,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiagnostic {
    pub target: String,
    pub url: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub latency_ms: u64,
    pub proxy: String,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainCheck {
    pub tool: String,
    pub available: bool,
    pub version: String,
    pub advice: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseDependenciesInfo {
    pub package: String,
    pub depends: usize,
    pub imports: usize,
    pub suggests: usize,
    pub linking_to: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub package: String,
    pub requested_version: String,
    pub latest_version: String,
    pub repository: String,
    pub real_name: String,
    pub source: String,
    pub found: bool,
    pub message: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub run_id: u64,
    pub results: Vec<SearchResult>,
    pub logs: Vec<String>,
    pub stopped: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_timings: Vec<SearchStageTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_graph: Option<DependencyGraph>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStageTiming {
    pub stage: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInput {
    pub raw: String,
    pub name: String,
    pub version: String,
    pub source_hint: Option<String>,
}
