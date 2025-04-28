use std::time::Duration;

use duration_str::deserialize_duration;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamHealthChecksConfig {
    #[serde(default = "default_upstream_liveness_enabled")]
    pub enabled: bool,
    #[serde(
        default = "default_upstream_liveness_interval",
        deserialize_with = "deserialize_duration_with_default"
    )]
    pub interval: Duration,
}

fn deserialize_duration_with_default<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => deserialize_duration(serde::de::IntoDeserializer::into_deserializer(s)),
        None => Ok(default_upstream_liveness_interval()),
    }
}

// Default functions for health checks
fn default_upstream_liveness_enabled() -> bool {
    true
}

fn default_upstream_liveness_interval() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

impl Default for UpstreamHealthChecksConfig {
    fn default() -> Self {
        Self {
            enabled: default_upstream_liveness_enabled(),
            interval: default_upstream_liveness_interval(),
        }
    }
}
