use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub proxy: String,
    pub github_token: String,
    pub cran_mirror: String,
    pub full_search: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            github_token: String::new(),
            cran_mirror: "https://cloud.r-project.org".to_string(),
            full_search: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOptions {
    pub method: String,
    pub conditional: bool,
    pub install_dependencies: bool,
    pub mirror: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub logs: Vec<String>,
    pub stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: String,
    pub command: String,
    pub package_name: String,
    pub version: String,
    pub tool_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInput {
    pub raw: String,
    pub name: String,
    pub version: String,
}
