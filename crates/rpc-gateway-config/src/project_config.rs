use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub key: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            key: None,
        }
    }
}
