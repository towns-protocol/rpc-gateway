use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_allow_any_origin")]
    pub allow_any_origin: bool,
    #[serde(default = "default_allow_any_header")]
    pub allow_any_header: bool,
    #[serde(default = "default_allow_any_method")]
    pub allow_any_method: bool,
    #[serde(default = "default_expose_any_header")]
    pub expose_any_header: bool,
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
    #[serde(default = "default_max_age")]
    pub max_age: u32,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_any_origin: default_allow_any_origin(),
            allow_any_header: default_allow_any_header(),
            allow_any_method: default_allow_any_method(),
            expose_any_header: default_expose_any_header(),
            allowed_origins: default_allowed_origins(),
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
            max_age: default_max_age(),
        }
    }
}

fn default_allow_any_origin() -> bool {
    true
}

fn default_allow_any_header() -> bool {
    true
}

fn default_allow_any_method() -> bool {
    true
}

fn default_expose_any_header() -> bool {
    true
}

fn default_allowed_origins() -> Vec<String> {
    vec![]
}

fn default_allowed_methods() -> Vec<String> {
    // TODO: consider adding GET in the future.
    vec!["POST".to_string(), "OPTIONS".to_string()]
}

fn default_allowed_headers() -> Vec<String> {
    vec!["Content-Type".to_string(), "Authorization".to_string()]
}

fn default_max_age() -> u32 {
    3600
}
