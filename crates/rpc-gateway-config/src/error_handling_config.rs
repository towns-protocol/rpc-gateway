use duration_str::deserialize_duration;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for error handling behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        /// JSON-RPC error codes that should trigger failover to the next upstream.
        /// Common codes: -32603 (internal error, e.g., "state is pruned")
        #[serde(default)]
        failover_on_rpc_error_codes: Vec<i64>,
    },
    FailFast {
        /// JSON-RPC error codes that should trigger failover to the next upstream.
        /// Common codes: -32603 (internal error, e.g., "state is pruned")
        #[serde(default)]
        failover_on_rpc_error_codes: Vec<i64>,
    },
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
        /// JSON-RPC error codes that should trigger failover to the next upstream.
        /// Common codes: -32603 (internal error, e.g., "state is pruned")
        #[serde(default)]
        failover_on_rpc_error_codes: Vec<i64>,
    },
}

impl ErrorHandlingConfig {
    /// Returns the list of JSON-RPC error codes that should trigger failover.
    pub fn failover_error_codes(&self) -> &[i64] {
        match self {
            ErrorHandlingConfig::Retry {
                failover_on_rpc_error_codes,
                ..
            } => failover_on_rpc_error_codes,
            ErrorHandlingConfig::FailFast {
                failover_on_rpc_error_codes,
            } => failover_on_rpc_error_codes,
            ErrorHandlingConfig::CircuitBreaker {
                failover_on_rpc_error_codes,
                ..
            } => failover_on_rpc_error_codes,
        }
    }
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
    ErrorHandlingConfig::FailFast {
        failover_on_rpc_error_codes: vec![],
    }
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
