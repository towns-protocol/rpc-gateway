use alloy_chains::Chain;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::path::PathBuf;
use url::Url;

use crate::cache_config::CacheConfig;
use crate::canned_response_config::CannedResponseConfig;
use crate::chain_config::ChainConfig;
use crate::cors_config::CorsConfig;
use crate::error_handling_config::ErrorHandlingConfig;
use crate::load_balancing_config::LoadBalancingStrategy;
use crate::logging_config::LoggingConfig;
use crate::metrics_config::MetricsConfig;
use crate::project_config::ProjectConfig;
use crate::request_coalescing_config::RequestCoalescingConfig;
use crate::server_config::ServerConfig;
use crate::upstream_health_checks_config::UpstreamHealthChecksConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub load_balancing: LoadBalancingStrategy,

    #[serde(default)]
    pub upstream_health_checks: UpstreamHealthChecksConfig,

    #[serde(default)]
    pub error_handling: ErrorHandlingConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub cache: CacheConfig,

    #[serde(default)]
    pub canned_responses: CannedResponseConfig,

    #[serde(default)]
    pub request_coalescing: RequestCoalescingConfig,

    #[serde(default)]
    pub metrics: MetricsConfig,

    #[serde(default)]
    pub cors: CorsConfig,

    #[serde(default)]
    #[serde(with = "chain_map_serde")]
    pub chains: HashMap<u64, ChainConfig>,

    #[serde(default)]
    #[serde(with = "projects_serde")]
    pub projects: HashMap<String, ProjectConfig>,
}

fn default_projects() -> HashMap<String, ProjectConfig> {
    let mut projects = HashMap::new();
    projects.insert("default".to_string(), ProjectConfig::default());
    projects
}

fn default_chains() -> HashMap<u64, ChainConfig> {
    HashMap::new()
}

impl Config {
    pub fn from_yaml_str(s: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config: Config =
            serde_yaml::from_str(s).map_err(|e| format!("invalid yaml: {}", e))?;

        if config.chains.is_empty() {
            return Err("chains map cannot be empty".into());
        }

        config.process_urls()?;
        config.process_project_keys()?;
        Ok(config)
    }

    pub fn from_yaml_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&contents)
    }

    pub fn from_yaml_path_buf(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&contents)
    }

    fn process_project_keys(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Process project keys
        for (_, project_config) in &mut self.projects {
            if let Some(key) = &project_config.key {
                if key.starts_with('$') {
                    let env_var = key.trim_start_matches('$');
                    let env_value = std::env::var(env_var).map_err(|e| {
                        format!("Environment variable '{}' not found: {}", env_var, e)
                    })?;
                    project_config.key = Some(env_value);
                }
            }
        }

        Ok(())
    }

    fn process_urls(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Process upstream URLs
        for (_, chain_config) in &mut self.chains {
            for upstream in chain_config.upstreams.iter_mut() {
                if upstream.url.as_str().starts_with('$') {
                    let env_var = upstream.url.as_str().trim_start_matches('$');
                    let env_value = std::env::var(env_var).map_err(|e| {
                        format!("Environment variable '{}' not found: {}", env_var, e)
                    })?;
                    upstream.url = Url::parse(&env_value)?;
                }
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            load_balancing: LoadBalancingStrategy::default(),
            upstream_health_checks: UpstreamHealthChecksConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
            logging: LoggingConfig::default(),
            cache: CacheConfig::default(),
            canned_responses: CannedResponseConfig::default(),
            request_coalescing: RequestCoalescingConfig::default(),
            metrics: MetricsConfig::default(),
            projects: default_projects(),
            chains: default_chains(),
            cors: CorsConfig::default(),
        }
    }
}

mod chain_map_serde {
    use super::*;
    use serde::{Deserializer, Serializer};
    use std::collections::HashMap;
    use std::str::FromStr;

    pub fn serialize<S>(map: &HashMap<u64, ChainConfig>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let string_map: HashMap<String, ChainConfig> = map
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        string_map.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<u64, ChainConfig>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string_map: HashMap<String, ChainConfig> = HashMap::deserialize(deserializer)?;
        let mut map = HashMap::new();
        for (k, mut v) in string_map {
            let key = u64::from_str(&k).map_err(serde::de::Error::custom)?;
            v.chain = Chain::from_id(key);
            v.block_time = v.block_time.or(v.chain.average_blocktime_hint());
            map.insert(key, v);
        }
        if map.is_empty() {
            return Err(serde::de::Error::custom("chains map cannot be empty"));
        }
        Ok(map)
    }
}

mod projects_serde {
    use super::*;
    use serde::{Deserializer, Serializer};
    use std::collections::HashMap;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<String, ProjectConfig>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // First try to deserialize as a list
        let list: Vec<ProjectConfig> = Vec::deserialize(deserializer)?;

        // Convert list to HashMap using name as key
        let mut map = HashMap::new();
        for project in list {
            map.insert(project.name.clone(), project);
        }

        if !map.contains_key("default") {
            map.insert("default".to_string(), ProjectConfig::default());
        }

        Ok(map)
    }

    pub fn serialize<S>(
        map: &HashMap<String, ProjectConfig>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert HashMap to Vec for serialization
        let list: Vec<&ProjectConfig> = map.values().collect();
        list.serialize(serializer)
    }
}

#[cfg(test)]
mod test_helpers {
    use std::thread;
    use std::time::Duration;

    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 50;

    pub fn set_env_var_with_retry(key: &str, value: &str) -> Result<(), String> {
        let mut attempts = 0;
        while attempts < MAX_RETRIES {
            unsafe {
                std::env::set_var(key, value);
                match std::env::var(key) {
                    Ok(val) if val == value => return Ok(()),
                    _ => {
                        attempts += 1;
                        if attempts < MAX_RETRIES {
                            thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                        }
                    }
                }
            }
        }
        Err(format!(
            "Failed to set environment variable '{}' after {} attempts",
            key, MAX_RETRIES
        ))
    }

    pub fn remove_env_var_with_retry(key: &str) -> Result<(), String> {
        let mut attempts = 0;
        while attempts < MAX_RETRIES {
            unsafe {
                std::env::remove_var(key);
                match std::env::var(key) {
                    Err(_) => return Ok(()),
                    _ => {
                        attempts += 1;
                        if attempts < MAX_RETRIES {
                            thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                        }
                    }
                }
            }
        }
        Err(format!(
            "Failed to remove environment variable '{}' after {} attempts",
            key, MAX_RETRIES
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::config::test_helpers::{remove_env_var_with_retry, set_env_var_with_retry};

    use super::*;
    use std::time::Duration;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert!(matches!(
            config.load_balancing,
            LoadBalancingStrategy::PrimaryOnly
        ));
        assert!(matches!(
            config.error_handling,
            ErrorHandlingConfig::FailFast
        ));
        assert!(config.canned_responses.enabled);
        assert!(config.canned_responses.methods.web3_client_version);
        assert!(config.canned_responses.methods.eth_chain_id);
        assert!(config.chains.is_empty());
    }

    #[test]
    fn test_parse_valid_config() {
        let config_str = r#"
server:
  host: "127.0.0.1"
  port: 8080

load_balancing:
  strategy: "weighted_order"

error_handling:
  type: "retry"
  max_retries: 3
  retry_delay: "1s"
  jitter: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
        timeout: "10s"
        weight: 1
"#;

        let config = Config::from_yaml_str(config_str).unwrap();

        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert!(matches!(
            config.load_balancing,
            LoadBalancingStrategy::WeightedOrder
        ));
        assert!(matches!(
            config.error_handling,
            ErrorHandlingConfig::Retry {
                max_retries,
                retry_delay,
                jitter,
            } if max_retries == 3 && retry_delay == Duration::from_secs(1) && jitter == true
        ));

        let chain = config.chains.get(&1).unwrap();
        let upstream = chain.upstreams.iter().next().unwrap();
        assert_eq!(upstream.url.as_str(), "http://example.com/");
        assert_eq!(upstream.timeout, Duration::from_secs(10));
        assert_eq!(upstream.weight, 1);
    }

    #[test]
    fn test_multiple_chains() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://chain1.example.com"
  2:
    upstreams:
      - url: "http://chain2.example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();

        assert_eq!(config.chains.len(), 2);

        let chain1 = config.chains.get(&1).unwrap();
        assert_eq!(
            chain1.upstreams.iter().next().unwrap().url.as_str(),
            "http://chain1.example.com/"
        );

        let chain2 = config.chains.get(&2).unwrap();
        assert_eq!(
            chain2.upstreams.iter().next().unwrap().url.as_str(),
            "http://chain2.example.com/"
        );
    }

    #[test]
    fn test_duration_parsing() {
        let config_str = r#"
error_handling:
  type: "retry"
  max_retries: 3
  retry_delay: "1s"
  jitter: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
        timeout: "30s"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();

        assert!(matches!(
            config.error_handling,
            ErrorHandlingConfig::Retry {
                retry_delay,
                ..
            } if retry_delay == Duration::from_secs(1)
        ));
        assert_eq!(
            config
                .chains
                .get(&1)
                .unwrap()
                .upstreams
                .iter()
                .next()
                .unwrap()
                .timeout,
            Duration::from_secs(30)
        );
    }

    #[test]
    fn test_empty_upstreams() {
        let config_str = r#"
chains:
  1:
    upstreams: []
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("upstreams cannot be empty"));
    }

    #[test]
    fn test_invalid_duration() {
        let config_str = r#"
error_handling:
  type: "retry"
  max_retries: 3
  retry_delay: "invalid duration"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // TODO: give better error message. describe the failing field and value.
        assert!(err.to_string().contains("invalid duration"));
    }

    #[test]
    fn test_invalid_port() {
        let config_str = r#"
server:
  port: 70000

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid value: integer `70000`, expected u16")
        );
    }

    #[test]
    fn test_invalid_load_balancing_config() {
        let config_str = r#"
load_balancing:
  strategy: "invalid_mode"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn test_invalid_error_handling_config() {
        let config_str = r#"
error_handling:
  type: "invalid_mode"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn test_zero_weight() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
        weight: 0
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("weight cannot be zero"));
    }

    #[test]
    fn test_zero_timeout() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
        timeout: "0s"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("timeout cannot be zero"));
    }

    #[test]
    fn test_zero_max_retries() {
        let config_str = r#"
error_handling:
  type: "retry"
  max_retries: 0
  retry_delay: "1s"
  jitter: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("max_retries cannot be zero"));
    }

    #[test]
    fn test_mixed_urls() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "https://api1.example.com"
        weight: 1
      - url: "https://api2.example.com"
        weight: 1
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert_eq!(
            chain.upstreams.iter().next().unwrap().url.as_str(),
            "https://api1.example.com/"
        );
        assert_eq!(
            chain.upstreams.iter().nth(1).unwrap().url.as_str(),
            "https://api2.example.com/"
        );
    }

    #[test]
    fn test_env_var_url() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "$ALCHEMY_URL"
        weight: 1
"#;

        // Set up test environment
        set_env_var_with_retry(
            "ALCHEMY_URL",
            "https://eth-mainnet.g.alchemy.com/v2/test-key",
        )
        .unwrap();

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert_eq!(
            chain.upstreams.iter().next().unwrap().url.as_str(),
            "https://eth-mainnet.g.alchemy.com/v2/test-key/"
        );

        // Clean up
        remove_env_var_with_retry("ALCHEMY_URL").unwrap();
    }

    #[test]
    fn test_mixed_env_and_static_urls() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "$ALCHEMY_URL"
        weight: 1
      - url: "https://static-url.com"
        weight: 1
"#;

        // Set up test environment
        set_env_var_with_retry(
            "ALCHEMY_URL",
            "https://eth-mainnet.g.alchemy.com/v2/test-key",
        )
        .unwrap();

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert_eq!(
            chain.upstreams.iter().next().unwrap().url.as_str(),
            "https://eth-mainnet.g.alchemy.com/v2/test-key/"
        );
        assert_eq!(
            chain.upstreams.iter().nth(1).unwrap().url.as_str(),
            "https://static-url.com/"
        );

        // Clean up
        remove_env_var_with_retry("ALCHEMY_URL").unwrap();
    }

    #[test]
    fn test_chain_config_with_block_time() {
        let config_str = r#"
chains:
  1:
    block_time: 13000ms
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert_eq!(chain.block_time, Some(Duration::from_millis(13000)));
    }

    #[test]
    fn test_chain_config_without_block_time() {
        let config_str = r#"
chains:
  999888777:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&999888777).unwrap();
        assert_eq!(chain.block_time, None);
    }

    #[test]
    fn test_get_block_time_config_override() {
        let mut chain_config = ChainConfig::default();
        chain_config.block_time = Some(Duration::from_millis(5000)); // 5 seconds
        assert_eq!(chain_config.block_time, Some(Duration::from_millis(5000)));
    }

    #[test]
    fn test_get_block_time_alloy_fallback() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
  137:
    upstreams:
      - url: "http://example.com"
  56:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();

        let chain_config = config.chains.get(&1).unwrap();
        assert_eq!(chain_config.block_time, Some(Duration::from_millis(12000))); // 12 seconds

        let chain_config = config.chains.get(&137).unwrap();
        assert_eq!(chain_config.block_time, Some(Duration::from_millis(2100))); // 2 seconds

        let chain_config = config.chains.get(&56).unwrap();
        assert_eq!(chain_config.block_time, Some(Duration::from_millis(3000))); // 3 seconds
    }

    #[test]
    fn test_get_block_time_priority() {
        let config_str = r#"
chains:
  1:
    block_time: "5s"
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain_config = config.chains.get(&1).unwrap();

        // Config should take precedence over alloy chain's value
        assert_eq!(chain_config.block_time, Some(Duration::from_millis(5000)));

        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain_config = config.chains.get(&1).unwrap();

        assert_eq!(chain_config.block_time, Some(Duration::from_millis(12000)));
    }

    #[test]
    fn test_get_block_time_unknown_chain() {
        let mut chain_config = ChainConfig::default();
        chain_config.chain = Chain::from_id(999999); // Unknown chain
        chain_config.block_time = None;
        assert_eq!(chain_config.block_time, None);
    }

    #[test]
    fn test_cache_config_default() {
        let config = Config::default();
        assert!(matches!(config.cache, CacheConfig::Disabled));
    }

    #[test]
    fn test_cache_config_redis() {
        let config_str = r#"
cache:
  type: "redis"
  url: "redis://localhost:6379"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(matches!(config.cache, CacheConfig::Redis(_)));
    }

    #[test]
    fn test_cache_config_local() {
        let config_str = r#"
cache:
  type: "local"
  capacity: 5000

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(matches!(config.cache, CacheConfig::Local(_)));
    }

    #[test]
    fn test_cache_config_disabled() {
        let config_str = r#"
cache:
  type: "disabled"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(matches!(config.cache, CacheConfig::Disabled));
    }

    #[test]
    fn test_cache_config_omitted() {
        let config_str = r#"
server:
  host: "localhost"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(matches!(config.cache, CacheConfig::Disabled));
    }

    #[test]
    fn test_upstream_health_checks_default() {
        let config = Config::default();
        assert!(config.upstream_health_checks.enabled);
        assert_eq!(
            config.upstream_health_checks.interval,
            Duration::from_secs(300)
        );
    }

    #[test]
    fn test_upstream_health_checks_from_yaml() {
        let config_str = r#"
upstream_health_checks:
  enabled: true
  interval: "1m"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.upstream_health_checks.enabled);
        assert_eq!(
            config.upstream_health_checks.interval,
            Duration::from_secs(60)
        );
    }

    #[test]
    fn test_upstream_health_checks_optional_interval() {
        let config_str = r#"
upstream_health_checks:
  enabled: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.upstream_health_checks.enabled);
        assert_eq!(
            config.upstream_health_checks.interval,
            Duration::from_secs(300)
        );
    }

    #[test]
    fn test_upstream_health_checks_disabled() {
        let config_str = r#"
upstream_health_checks:
  enabled: false
  interval: "1m"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(!config.upstream_health_checks.enabled);
        assert_eq!(
            config.upstream_health_checks.interval,
            Duration::from_secs(60)
        );
    }

    #[test]
    fn test_upstream_health_checks_invalid_duration() {
        let config_str = r#"
upstream_health_checks:
  enabled: true
  interval: "invalid duration"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid duration"));
    }

    #[test]
    fn test_canned_responses_default() {
        let config = Config::default();
        assert!(
            config.canned_responses.enabled,
            "enabled should be true by default"
        );
        assert!(
            config.canned_responses.methods.web3_client_version,
            "web3_client_version should be true by default"
        );
        assert!(
            config.canned_responses.methods.eth_chain_id,
            "eth_chain_id should be true by default"
        );
    }

    #[test]
    fn test_canned_responses_from_yaml() {
        let config_str = r#"
canned_responses:
  enabled: true
  methods:
    web3_client_version: true
    eth_chain_id: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.canned_responses.enabled);
        assert!(config.canned_responses.methods.web3_client_version);
        assert!(config.canned_responses.methods.eth_chain_id);
    }

    #[test]
    fn test_canned_responses_disabled() {
        let config_str = r#"
canned_responses:
  enabled: false
  methods:
    web3_client_version: true
    eth_chain_id: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(!config.canned_responses.enabled);
        assert!(config.canned_responses.methods.web3_client_version);
        assert!(config.canned_responses.methods.eth_chain_id);
    }

    #[test]
    fn test_canned_responses_partial_methods() {
        let config_str = r#"
canned_responses:
  enabled: true
  methods:
    web3_client_version: false

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.canned_responses.enabled);
        assert!(!config.canned_responses.methods.web3_client_version);
        assert!(config.canned_responses.methods.eth_chain_id); // should default to true
    }

    #[test]
    fn test_canned_responses_omitted() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.canned_responses.enabled); // should default to true
        assert!(config.canned_responses.methods.web3_client_version); // should default to true
        assert!(config.canned_responses.methods.eth_chain_id); // should default to true
    }

    #[test]
    fn test_empty_chains_map() {
        let config_str = r#"
server:
  host: "localhost"
  port: 8080
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("chains map cannot be empty"));
    }

    #[test]
    fn test_non_empty_chains_map() {
        let config_str = r#"
server:
  host: "localhost"
  port: 8080

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(!config.chains.is_empty());
        assert_eq!(config.chains.len(), 1);
        assert!(config.chains.contains_key(&1));
    }

    #[test]
    fn test_invalid_yaml() {
        let config_str = r#"
invalid_yaml: [
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid yaml"));
    }

    #[test]
    fn test_cache_config_valid_redis() {
        let config_str = r#"
cache:
  type: "redis"
  url: "redis://localhost:6379"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(matches!(config.cache, CacheConfig::Redis(_)));
    }

    #[test]
    fn test_cache_config_valid_local() {
        let config_str = r#"
cache:
  type: "local"
  capacity: 5000

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(matches!(config.cache, CacheConfig::Local(_)));
    }

    #[test]
    fn test_request_coalescing_default() {
        let config = Config::default();
        assert!(
            config.request_coalescing.enabled,
            "enabled should be true by default"
        );
    }

    #[test]
    fn test_request_coalescing_from_yaml() {
        let config_str = r#"
request_coalescing:
  enabled: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.request_coalescing.enabled);
    }

    #[test]
    fn test_request_coalescing_disabled() {
        let config_str = r#"
request_coalescing:
  enabled: false

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(!config.request_coalescing.enabled);
    }

    #[test]
    fn test_request_coalescing_omitted() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.request_coalescing.enabled); // should default to true
    }

    #[test]
    fn test_request_coalescing_invalid_value() {
        let config_str = r#"
request_coalescing:
  enabled: "not a boolean"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid type"));
    }

    #[test]
    fn test_metrics_config_default() {
        let config = Config::default();
        assert!(config.metrics.enabled, "enabled should be true by default");
        assert_eq!(config.metrics.port, 8082, "port should be 8082 by default");
        assert_eq!(
            config.metrics.host,
            "127.0.0.1".to_string(),
            "host should be localhost by default"
        );
        assert_eq!(
            config.metrics.host_bytes(),
            Ok([127, 0, 0, 1]),
            "host_bytes should be localhost by default"
        );
    }

    #[test]
    fn test_metrics_config_from_yaml() {
        let config_str = r#"
metrics:
  enabled: true
  port: 9091
  host: "0.0.0.0"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.metrics.enabled);
        assert_eq!(config.metrics.port, 9091);
        assert_eq!(config.metrics.host, "0.0.0.0");
        assert_eq!(config.metrics.host_bytes(), Ok([0, 0, 0, 0]));
    }

    #[test]
    fn test_metrics_config_disabled() {
        let config_str = r#"
metrics:
  enabled: false
  port: 9091
  host: "0.0.0.0"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(!config.metrics.enabled);
        assert_eq!(config.metrics.port, 9091);
        assert_eq!(config.metrics.host, "0.0.0.0");
        assert_eq!(config.metrics.host_bytes(), Ok([0, 0, 0, 0]));
    }

    #[test]
    fn test_metrics_config_omitted() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.metrics.enabled, "enabled should default to true");
        assert_eq!(config.metrics.port, 8082, "port should default to 8082");
        assert_eq!(
            config.metrics.host,
            "127.0.0.1".to_string(),
            "host should default to localhost"
        );
        assert_eq!(
            config.metrics.host_bytes(),
            Ok([127, 0, 0, 1]),
            "host_bytes should default to localhost"
        );
    }

    #[test]
    fn test_metrics_config_invalid_port() {
        let config_str = r#"
metrics:
  enabled: true
  port: 70000
  host: "127.0.0.1"

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid value: integer `70000`, expected u16")
        );
    }

    #[test]
    fn test_metrics_config_invalid_host() {
        let config_str = r#"
metrics:
  enabled: true
  port: 8082
  host: "127.0.0"  # Invalid host format

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let result = std::panic::catch_unwind(|| {
            config.metrics.host_bytes().unwrap();
        });
        assert!(result.is_err(), "Should panic on invalid host format");
    }

    #[test]
    fn test_metrics_config_partial() {
        let config_str = r#"
metrics:
  port: 9091

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.metrics.enabled, "enabled should default to true");
        assert_eq!(config.metrics.port, 9091);
        assert_eq!(
            config.metrics.host,
            "127.0.0.1".to_string(),
            "host should default to localhost"
        );
        assert_eq!(
            config.metrics.host_bytes(),
            Ok([127, 0, 0, 1]),
            "host_bytes should default to localhost"
        );
    }

    #[test]
    fn test_metrics_config_host_bytes() {
        let config = MetricsConfig {
            enabled: true,
            port: 8082,
            host: "192.168.1.1".to_string(),
        };
        assert_eq!(config.host_bytes(), Ok([192, 168, 1, 1]));

        let config = MetricsConfig {
            enabled: true,
            port: 8082,
            host: "0.0.0.0".to_string(),
        };
        assert_eq!(config.host_bytes(), Ok([0, 0, 0, 0]));

        let config = MetricsConfig {
            enabled: true,
            port: 8082,
            host: "invalid".to_string(),
        };
        assert_eq!(
            config.host_bytes(),
            Err("Invalid IPv4 address format: invalid".to_string())
        );
    }

    #[test]
    fn test_projects_config_default() {
        let config = Config::default();
        assert_eq!(config.projects.len(), 1);
        assert!(config.projects.contains_key("default"));
    }
}
