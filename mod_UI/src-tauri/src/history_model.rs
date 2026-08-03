use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: String,
    pub command: String,
    pub package_name: String,
    pub version: String,
    pub tool_name: String,
    pub created_at: String,
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub conditional: bool,
    #[serde(default)]
    pub install_dependencies: bool,
    #[serde(default)]
    pub show_remote_version: bool,
    #[serde(default)]
    pub verify_install: bool,
    #[serde(default)]
    pub cran_mirror: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageCacheEntry {
    pub package_name: String,
    pub source: String,
    pub version: String,
    pub repository: String,
    pub real_name: String,
    pub cached_at: String,
    #[serde(default)]
    pub verified_count: u32,
    #[serde(default)]
    pub up_votes: u32,
    #[serde(default)]
    pub down_votes: u32,
    #[serde(default)]
    pub invalidated: bool,
}

impl PackageCacheEntry {
    pub fn is_trusted(&self) -> bool {
        self.verified_count >= crate::models::CACHE_TRUST_THRESHOLD
            && self.up_votes >= self.down_votes
            && !self.invalidated
    }
}
