#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputRules {
    pub separators: Vec<String>,
    pub strip_quotes: bool,
    pub strip_c_parens: bool,
    pub comment_chars: Vec<String>,
    pub split_spaces: bool,
    #[serde(default)]
    pub exclude_regex: Vec<String>,
    #[serde(default)]
    pub exclude_keywords: Vec<String>,
}

impl Default for InputRules {
    fn default() -> Self {
        Self {
            separators: vec![",".to_string(), ";".to_string()],
            strip_quotes: true,
            strip_c_parens: true,
            comment_chars: vec!["#".to_string()],
            split_spaces: false,
            exclude_regex: Vec::new(),
            exclude_keywords: Vec::new(),
        }
    }
}

impl InputRules {
    pub fn normalized(&self) -> Self {
        let mut separators: Vec<String> = self
            .separators
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 16 && !s.chars().any(char::is_control))
            .take(20)
            .collect();
        if separators.is_empty() {
            separators = vec![",".to_string(), ";".to_string()];
        }
        let mut comment_chars: Vec<String> = self
            .comment_chars
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 16 && !s.chars().any(char::is_control))
            .take(20)
            .collect();
        if comment_chars.is_empty() {
            comment_chars = vec!["#".to_string()];
        }
        let exclude_regex: Vec<String> = self
            .exclude_regex
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .filter(|s| regex::Regex::new(s).is_ok())
            .take(10)
            .collect();
        let exclude_keywords: Vec<String> = self
            .exclude_keywords
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() <= 64 && !s.chars().any(char::is_control))
            .take(50)
            .collect();
        Self {
            separators,
            strip_quotes: self.strip_quotes,
            strip_c_parens: self.strip_c_parens,
            comment_chars,
            split_spaces: self.split_spaces,
            exclude_regex,
            exclude_keywords,
        }
    }
}

pub const INPUT_RULES_FILE_NAME: &str = "input_rules.json";
