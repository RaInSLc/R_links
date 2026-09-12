pub const MAX_INPUT_CHARS: usize = 100_000;
pub const MAX_PACKAGE_LINES: usize = 500;
pub const MAX_FIELD_CHARS: usize = 2_048;
pub const CACHE_TRUST_THRESHOLD: u32 = 3;
pub const MAX_TOKEN_CHARS: usize = 512;
pub const MAX_HISTORY_RECORDS: usize = 10000;
pub const MAX_HISTORY_COMMAND_CHARS: usize = 8_000;
pub const MAX_SCRIPT_CHARS: usize = 1_000_000;

#[path = "dependency_model.rs"]
mod dependency_model;
#[path = "history_model.rs"]
mod history_model;
#[path = "input_rules_model.rs"]
mod input_rules_model;
#[path = "search_model.rs"]
mod search_model;
#[path = "settings_model.rs"]
mod settings_model;
#[path = "url_normalization.rs"]
mod url_normalization;

pub use dependency_model::*;
pub use history_model::*;
pub use input_rules_model::*;
pub use search_model::*;
pub use settings_model::*;
pub use url_normalization::*;

#[cfg(test)]
#[path = "models/tests.rs"]
mod tests;
