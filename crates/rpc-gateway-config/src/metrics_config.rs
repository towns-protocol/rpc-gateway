use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_metrics_enabled")]
    pub enabled: bool,
    #[serde(default = "default_metrics_port")]
    pub port: u16,
    #[serde(default = "default_metrics_host")]
    pub host: String,
}

impl MetricsConfig {
    pub fn host_bytes(&self) -> Result<[u8; 4], String> {
        let parts: Result<Vec<u8>, _> = self
            .host
            .split('.')
            .map(|part| part.parse::<u8>())
            .collect();

        match parts {
            Ok(parts) if parts.len() == 4 => Ok([parts[0], parts[1], parts[2], parts[3]]),
            _ => Err(format!("Invalid IPv4 address format: {}", self.host)),
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: default_metrics_enabled(),
            port: default_metrics_port(),
            host: default_metrics_host(),
        }
    }
}

fn default_metrics_enabled() -> bool {
    true
}

fn default_metrics_port() -> u16 {
    8082
}

fn default_metrics_host() -> String {
    "127.0.0.1".to_string()
}
