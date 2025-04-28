use ::serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestCoalescingConfig {
    #[serde(default = "default_request_coalescing_enabled")]
    pub enabled: bool,
}

fn default_request_coalescing_enabled() -> bool {
    true
}

impl Default for RequestCoalescingConfig {
    fn default() -> Self {
        Self {
            enabled: default_request_coalescing_enabled(),
        }
    }
}
