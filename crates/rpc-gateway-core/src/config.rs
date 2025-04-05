use alloy_chains::Chain;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub load_balancing: LoadBalancingConfig,
    #[serde(default)]
    pub error_handling: ErrorHandlingConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    #[serde(with = "chain_map_serde")]
    pub chains: HashMap<u64, ChainConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoadBalancingConfig {
    RoundRobin,
    WeightedRoundRobin {
        #[serde(default = "default_weight_decay")]
        weight_decay: f64,
    },
    LeastConnections {
        #[serde(default = "default_connection_multiplier")]
        connection_multiplier: f64,
    },
}

impl Default for LoadBalancingConfig {
    fn default() -> Self {
        default_load_balancing_config()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ErrorHandlingConfig {
    Retry {
        #[serde(default = "default_max_retries")]
        #[serde(deserialize_with = "validate_max_retries")]
        max_retries: u32,
        #[serde(default = "default_retry_delay", with = "duration_serde")]
        retry_delay: Duration,
        #[serde(default = "default_retry_jitter")]
        jitter: bool,
    },
    FailFast {
        #[serde(default = "default_error_threshold")]
        error_threshold: u32,
        #[serde(default = "default_error_window", with = "duration_serde")]
        error_window: Duration,
    },
    CircuitBreaker {
        #[serde(default = "default_failure_threshold")]
        failure_threshold: u32,
        #[serde(default = "default_reset_timeout", with = "duration_serde")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    #[serde(skip)]
    pub chain: Chain,
    pub upstreams: Vec<UpstreamConfig>,
    #[serde(default)]
    block_time_ms: Option<u64>,
}

impl ChainConfig {
    pub fn get_block_time(&self) -> Option<Duration> {
        if let Some(block_time_ms) = self.block_time_ms {
            Some(Duration::from_millis(block_time_ms))
        } else {
            self.chain.average_blocktime_hint()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    #[serde(with = "url_serde")]
    pub url: Url,
    #[serde(default = "default_timeout", with = "duration_serde")]
    pub timeout: Duration,
    #[serde(default = "default_weight")]
    #[serde(deserialize_with = "validate_weight")]
    pub weight: u32,
}

fn validate_weight<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    if value == 0 {
        return Err(serde::de::Error::custom("weight cannot be zero"));
    }
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default)]
    pub console: ConsoleLogConfig,
    #[serde(default)]
    pub file: FileLogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLogConfig {
    #[serde(default = "default_console_enabled")]
    pub enabled: bool,
    #[serde(default = "default_console_level")]
    pub level: String,
    #[serde(default = "default_console_format")]
    pub format: String,
    #[serde(default = "default_include_target")]
    pub include_target: bool,
    #[serde(default = "default_include_thread_ids")]
    pub include_thread_ids: bool,
    #[serde(default = "default_include_thread_names")]
    pub include_thread_names: bool,
    #[serde(default = "default_include_file")]
    pub include_file: bool,
    #[serde(default = "default_include_line_number")]
    pub include_line_number: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogConfig {
    #[serde(default = "default_file_enabled")]
    pub enabled: bool,
    #[serde(default = "default_file_level")]
    pub level: String,
    #[serde(default = "default_file_format")]
    pub format: String,
    #[serde(default = "default_file_path")]
    pub path: String,
    #[serde(default = "default_file_rotation")]
    pub rotation: String,
    #[serde(default = "default_include_target")]
    pub include_target: bool,
    #[serde(default = "default_include_thread_ids")]
    pub include_thread_ids: bool,
    #[serde(default = "default_include_thread_names")]
    pub include_thread_names: bool,
    #[serde(default = "default_include_file")]
    pub include_file: bool,
    #[serde(default = "default_include_line_number")]
    pub include_line_number: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
    #[serde(default = "default_cache_capacity")]
    pub capacity: u64,
}

// Default functions for new fields
fn default_weight_decay() -> f64 {
    0.5
}

fn default_connection_multiplier() -> f64 {
    1.5
}

fn default_retry_jitter() -> bool {
    true
}

fn default_error_threshold() -> u32 {
    5
}

fn default_error_window() -> Duration {
    Duration::from_secs(60)
}

fn default_failure_threshold() -> u32 {
    3
}

fn default_reset_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_half_open_requests() -> u32 {
    1
}

// Existing default functions
fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    9090
}

fn default_load_balancing_config() -> LoadBalancingConfig {
    LoadBalancingConfig::WeightedRoundRobin {
        weight_decay: default_weight_decay(),
    }
}

fn default_error_handling_config() -> ErrorHandlingConfig {
    ErrorHandlingConfig::Retry {
        max_retries: default_max_retries(),
        retry_delay: default_retry_delay(),
        jitter: default_retry_jitter(),
    }
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay() -> Duration {
    Duration::from_secs(1)
}

fn default_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_weight() -> u32 {
    1
}

// Default functions for logging configuration
fn default_console_enabled() -> bool {
    true
}

fn default_console_level() -> String {
    if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "info".to_string()
    }
}

fn default_console_format() -> String {
    if cfg!(debug_assertions) {
        "text".to_string()
    } else {
        "json".to_string()
    }
}

fn default_file_enabled() -> bool {
    !cfg!(debug_assertions) // Enable file logging by default in release mode
}

fn default_file_level() -> String {
    "info".to_string()
}

fn default_file_format() -> String {
    "json".to_string()
}

fn default_file_path() -> String {
    "logs/rpc-gateway.log".to_string()
}

fn default_file_rotation() -> String {
    "daily".to_string()
}

fn default_include_target() -> bool {
    true
}

fn default_include_thread_ids() -> bool {
    true
}

fn default_include_thread_names() -> bool {
    true
}

fn default_include_file() -> bool {
    true
}

fn default_include_line_number() -> bool {
    true
}

fn default_cache_enabled() -> bool {
    false
}

fn default_cache_capacity() -> u64 {
    10_000 // Default cache capacity of 10,000 entries
}

impl Config {
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        let config: Config = toml::from_str(s)?;

        // Validate error handling configuration
        match &config.error_handling {
            ErrorHandlingConfig::Retry { max_retries, .. } if *max_retries == 0 => {
                return Err(serde::de::Error::custom(
                    "max_retries must be greater than 0",
                ));
            }
            ErrorHandlingConfig::FailFast {
                error_threshold, ..
            } if *error_threshold == 0 => {
                return Err(serde::de::Error::custom(
                    "error_threshold must be greater than 0",
                ));
            }
            ErrorHandlingConfig::CircuitBreaker {
                failure_threshold,
                half_open_requests,
                ..
            } if *failure_threshold == 0 || *half_open_requests == 0 => {
                return Err(serde::de::Error::custom(
                    "failure_threshold and half_open_requests must be greater than 0",
                ));
            }
            _ => {}
        }

        // Validate weights
        for (chain_id, chain) in &config.chains {
            for upstream in &chain.upstreams {
                if upstream.weight == 0 {
                    return Err(serde::de::Error::custom(format!(
                        "weight must be greater than 0 in chain {}",
                        chain_id
                    )));
                }
            }
        }

        Ok(config)
    }

    pub fn from_toml_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        Ok(Self::from_toml_str(&contents)?)
    }

    pub fn from_path_buf(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        Ok(Self::from_toml_str(&contents)?)
    }

    pub fn from_yaml_str(s: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config: Config = serde_yaml::from_str(s)?;
        config.process_urls()?;
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

    fn process_urls(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (_, chain_config) in &mut self.chains {
            for upstream in &mut chain_config.upstreams {
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
            load_balancing: default_load_balancing_config(),
            error_handling: ErrorHandlingConfig::default(),
            logging: LoggingConfig::default(),
            cache: CacheConfig::default(),
            chains: HashMap::new(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            console: ConsoleLogConfig::default(),
            file: FileLogConfig::default(),
        }
    }
}

impl Default for ConsoleLogConfig {
    fn default() -> Self {
        Self {
            enabled: default_console_enabled(),
            level: default_console_level(),
            format: default_console_format(),
            include_target: default_include_target(),
            include_thread_ids: default_include_thread_ids(),
            include_thread_names: default_include_thread_names(),
            include_file: default_include_file(),
            include_line_number: default_include_line_number(),
        }
    }
}

impl Default for FileLogConfig {
    fn default() -> Self {
        Self {
            enabled: default_file_enabled(),
            level: default_file_level(),
            format: default_file_format(),
            path: default_file_path(),
            rotation: default_file_rotation(),
            include_target: default_include_target(),
            include_thread_ids: default_include_thread_ids(),
            include_thread_names: default_include_thread_names(),
            include_file: default_include_file(),
            include_line_number: default_include_line_number(),
        }
    }
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain: Chain::from_id(1),
            upstreams: Vec::new(),
            block_time_ms: None,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_cache_enabled(),
            capacity: default_cache_capacity(),
        }
    }
}

pub trait UrlProcessor {
    fn process_url(&self, url_str: &str) -> Result<String, String>;
}

#[derive(Debug, Clone)]
pub struct EnvVarUrlProcessor;

impl UrlProcessor for EnvVarUrlProcessor {
    fn process_url(&self, url_str: &str) -> Result<String, String> {
        if url_str.starts_with('$') {
            let var_name = url_str.trim_start_matches('$');
            std::env::var(var_name)
                .map_err(|e| format!("Environment variable '{}' not found: {}", var_name, e))
        } else {
            Ok(url_str.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct DefaultUrlProcessor;

impl UrlProcessor for DefaultUrlProcessor {
    fn process_url(&self, url_str: &str) -> Result<String, String> {
        EnvVarUrlProcessor.process_url(url_str)
    }
}

mod url_serde {
    use super::*;
    use serde::{Deserializer, Serializer};
    use std::str::FromStr;

    pub fn serialize<S>(url: &Url, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(url.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Url, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_with_processor(deserializer, &DefaultUrlProcessor)
    }

    pub fn deserialize_with_processor<'de, D, P>(
        deserializer: D,
        processor: &P,
    ) -> Result<Url, D::Error>
    where
        D: Deserializer<'de>,
        P: UrlProcessor,
    {
        let s = String::deserialize(deserializer)?;
        let processed_url = processor
            .process_url(&s)
            .map_err(serde::de::Error::custom)?;
        let mut url = Url::from_str(&processed_url).map_err(serde::de::Error::custom)?;
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        Ok(url)
    }
}

mod duration_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}s", duration.as_secs()))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if let Some(seconds) = s.strip_suffix('s') {
            if let Ok(secs) = seconds.parse::<u64>() {
                if secs > 0 {
                    return Ok(Duration::from_secs(secs));
                }
            }
        }
        Err(serde::de::Error::custom(
            "Duration must be a positive number followed by 's'",
        ))
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
            map.insert(key, v);
        }
        Ok(map)
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
    use super::*;
    use crate::config::test_helpers::*;
    use std::time::Duration;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9090);
        assert!(matches!(
            config.load_balancing,
            LoadBalancingConfig::WeightedRoundRobin { weight_decay } if weight_decay == 0.5
        ));
        assert!(matches!(
            config.error_handling,
            ErrorHandlingConfig::Retry {
                max_retries,
                retry_delay,
                jitter,
            } if max_retries == 3 && retry_delay == Duration::from_secs(1) && jitter == true
        ));
        assert!(config.chains.is_empty());
    }

    #[test]
    fn test_parse_valid_config() {
        let config_str = r#"
server:
  host: "127.0.0.1"
  port: 8080

load_balancing:
  type: "weighted_round_robin"
  weight_decay: 0.5

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
            LoadBalancingConfig::WeightedRoundRobin { weight_decay } if weight_decay == 0.5
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
        let upstream = &chain.upstreams[0];
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
            chain1.upstreams[0].url.as_str(),
            "http://chain1.example.com/"
        );

        let chain2 = config.chains.get(&2).unwrap();
        assert_eq!(
            chain2.upstreams[0].url.as_str(),
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
            config.chains.get(&1).unwrap().upstreams[0].timeout,
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

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert!(chain.upstreams.is_empty());
    }

    #[test]
    fn test_invalid_duration() {
        let config_str = r#"
error_handling:
  type: "retry"
  max_retries: 3
  retry_delay: "invalid duration"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_port() {
        let config_str = r#"
server:
  port: 70000
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_load_balancing_config() {
        let config_str = r#"
load_balancing:
  type: "invalid_mode"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_error_handling_config() {
        let config_str = r#"
error_handling:
  type: "invalid_mode"
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_duplicate_chain_ids() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example1.com"
  1:
    upstreams:
      - url: "http://example2.com"
"#;

        // YAML parser will overwrite the first key with the second one
        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert_eq!(chain.upstreams[0].url.as_str(), "http://example2.com/");
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
    }

    #[test]
    fn test_zero_max_retries() {
        let config_str = r#"
error_handling:
  type: "retry"
  max_retries: 0
  retry_delay: "1s"
  jitter: true
"#;

        let result = Config::from_yaml_str(config_str);
        assert!(result.is_err());
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
        assert_eq!(chain.upstreams[0].url.as_str(), "https://api1.example.com/");
        assert_eq!(chain.upstreams[1].url.as_str(), "https://api2.example.com/");
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
            chain.upstreams[0].url.as_str(),
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
            chain.upstreams[0].url.as_str(),
            "https://eth-mainnet.g.alchemy.com/v2/test-key/"
        );
        assert_eq!(chain.upstreams[1].url.as_str(), "https://static-url.com/");

        // Clean up
        remove_env_var_with_retry("ALCHEMY_URL").unwrap();
    }

    #[test]
    fn test_chain_config_with_block_time() {
        let config_str = r#"
chains:
  1:
    block_time_ms: 12000
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert_eq!(chain.block_time_ms, Some(12000));
    }

    #[test]
    fn test_chain_config_without_block_time() {
        let config_str = r#"
chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        let chain = config.chains.get(&1).unwrap();
        assert_eq!(chain.block_time_ms, None);
    }

    #[test]
    fn test_get_block_time_config_override() {
        let mut chain_config = ChainConfig::default();
        chain_config.block_time_ms = Some(5000); // 5 seconds
        assert_eq!(
            chain_config.get_block_time(),
            Some(Duration::from_millis(5000))
        );
    }

    #[test]
    fn test_get_block_time_alloy_fallback() {
        let mut chain_config = ChainConfig::default();
        chain_config.block_time_ms = None;
        chain_config.chain = Chain::from_id(1); // Ethereum Mainnet
        assert_eq!(
            chain_config.get_block_time(),
            Some(Duration::from_millis(12000))
        ); // 12 seconds

        chain_config.chain = Chain::from_id(137); // Polygon
        assert_eq!(
            chain_config.get_block_time(),
            Some(Duration::from_millis(2100))
        ); // 2 seconds

        chain_config.chain = Chain::from_id(56); // BSC
        assert_eq!(
            chain_config.get_block_time(),
            Some(Duration::from_millis(3000))
        ); // 3 seconds
    }

    #[test]
    fn test_get_block_time_priority() {
        let mut chain_config = ChainConfig::default();
        chain_config.chain = Chain::from_id(1); // Ethereum Mainnet

        // Config should take precedence over alloy chain's value
        chain_config.block_time_ms = Some(5000); // 5 seconds
        assert_eq!(
            chain_config.get_block_time(),
            Some(Duration::from_millis(5000))
        );

        // When config is None, should fall back to alloy chain's value
        chain_config.block_time_ms = None;
        assert_eq!(
            chain_config.get_block_time(),
            Some(Duration::from_millis(12000))
        );
    }

    #[test]
    fn test_get_block_time_unknown_chain() {
        let mut chain_config = ChainConfig::default();
        chain_config.chain = Chain::from_id(999999); // Unknown chain
        chain_config.block_time_ms = None;
        assert_eq!(chain_config.get_block_time(), None);
    }

    #[test]
    fn test_cache_config_default() {
        let config = Config::default();
        assert!(!config.cache.enabled);
        assert_eq!(config.cache.capacity, 10_000);
    }

    #[test]
    fn test_cache_config_from_toml() {
        let config_str = r#"
cache:
  enabled: true
  capacity: 5000
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.cache.enabled);
        assert_eq!(config.cache.capacity, 5000);
    }

    #[test]
    fn test_cache_config_omitted() {
        let config_str = r#"
server:
  host: "localhost"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(!config.cache.enabled);
        assert_eq!(config.cache.capacity, 10_000);
    }

    #[test]
    fn test_cache_config_with_other_settings() {
        let config_str = r#"
server:
  host: "localhost"
  port: 8080

cache:
  enabled: true

chains:
  1:
    upstreams:
      - url: "http://example.com"
"#;

        let config = Config::from_yaml_str(config_str).unwrap();
        assert!(config.cache.enabled);
        assert_eq!(config.server.host, "localhost");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.chains.get(&1).unwrap().upstreams.len(), 1);
    }
}
