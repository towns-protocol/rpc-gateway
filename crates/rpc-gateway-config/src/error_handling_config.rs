use duration_str::deserialize_duration;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ErrorHandlingConfig {
    Retry {
        #[serde(default = "default_max_retries")]
        #[serde(deserialize_with = "validate_max_retries")]
        max_retries: u32,
        #[serde(
            default = "default_retry_delay",
            deserialize_with = "deserialize_duration"
        )]
        retry_delay: Duration,
        #[serde(default = "default_retry_jitter")]
        jitter: bool,
    },
    FailFast,
    CircuitBreaker {
        #[serde(default = "default_failure_threshold")]
        failure_threshold: u32,
        #[serde(
            default = "default_reset_timeout",
            deserialize_with = "deserialize_duration"
        )]
        reset_timeout: Duration,
        #[serde(default = "default_half_open_requests")]
        half_open_requests: u32,
    },
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        default_error_handling_config()
    }
}

fn validate_max_retries<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    if value == 0 {
        return Err(serde::de::Error::custom("max_retries cannot be zero"));
    }
    Ok(value)
}

fn default_error_handling_config() -> ErrorHandlingConfig {
    ErrorHandlingConfig::FailFast
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay() -> Duration {
    Duration::from_secs(1)
}

fn default_retry_jitter() -> bool {
    true
}

fn default_failure_threshold() -> u32 {
    3
}

fn default_reset_timeout() -> Duration {
    Duration::from_secs(30)
}

// TODO: remove this or make use of it.
fn default_half_open_requests() -> u32 {
    1
}
