use alloy_chains::Chain;
use duration_str::deserialize_duration;
use nonempty::NonEmpty;
use serde::{Deserialize, Deserializer, Serialize};
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
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum LoadBalancingStrategy {
    PrimaryOnly,
    RoundRobin,
    WeightedOrder,
}

impl Default for LoadBalancingStrategy {
    fn default() -> Self {
        LoadBalancingStrategy::PrimaryOnly
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    #[serde(skip)]
    pub chain: Chain,
    #[serde(
        deserialize_with = "deserialize_nonempty_upstreams",
        serialize_with = "serialize_nonempty_upstreams"
    )]
    pub upstreams: NonEmpty<UpstreamConfig>,

    #[serde(default, deserialize_with = "deserialize_option_duration")]
    pub block_time: Option<Duration>,
}

fn deserialize_option_duration<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    // This will deserialize the value into an Option<String>
    // If the value is null, we get None; otherwise, we parse it
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let duration = deserialize_duration(serde::de::IntoDeserializer::into_deserializer(s))?;
            Ok(Some(duration))
        }
        None => Ok(None),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    #[serde(with = "url_serde")]
    pub url: Url,
    #[serde(default = "default_timeout", deserialize_with = "validate_timeout")]
    pub timeout: Duration,
    #[serde(default = "default_weight")]
    #[serde(deserialize_with = "validate_weight")]
    pub weight: u32,
}

fn validate_timeout<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let duration = deserialize_duration(deserializer)?;
    if duration.is_zero() {
        return Err(serde::de::Error::custom("timeout cannot be zero"));
    }
    Ok(duration)
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CacheConfig {
    Disabled,
    Redis(RedisCacheConfig),
    Local(LocalCacheConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisCacheConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
    pub key_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalCacheConfig {
    #[serde(default = "default_cache_capacity")]
    pub capacity: u64,
}

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

fn default_retry_jitter() -> bool {
    true
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
    8080
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
    false
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
    false
}

fn default_include_thread_ids() -> bool {
    false
}

fn default_include_thread_names() -> bool {
    false
}

fn default_include_file() -> bool {
    true
}

fn default_include_line_number() -> bool {
    true
}

fn default_cache_capacity() -> u64 {
    10_000 // Default cache capacity of 10,000 entries
}

// Default functions for health checks
fn default_upstream_liveness_enabled() -> bool {
    true
}

fn default_upstream_liveness_interval() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CannedResponseConfig {
    #[serde(default = "default_canned_responses_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub methods: CannedResponseMethods,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CannedResponseMethods {
    #[serde(default = "default_web3_client_version_enabled")]
    pub web3_client_version: bool,
    #[serde(default = "default_eth_chain_id_enabled")]
    pub eth_chain_id: bool,
}

fn default_canned_responses_enabled() -> bool {
    true
}

fn default_web3_client_version_enabled() -> bool {
    true
}

fn default_eth_chain_id_enabled() -> bool {
    true
}

impl Default for CannedResponseConfig {
    fn default() -> Self {
        Self {
            enabled: default_canned_responses_enabled(),
            methods: CannedResponseMethods::default(),
        }
    }
}

impl Default for CannedResponseMethods {
    fn default() -> Self {
        Self {
            web3_client_version: default_web3_client_version_enabled(),
            eth_chain_id: default_eth_chain_id_enabled(),
        }
    }
}

impl Config {
    pub fn from_yaml_str(s: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config: Config =
            serde_yaml::from_str(s).map_err(|e| format!("invalid yaml: {}", e))?;

        if config.chains.is_empty() {
            return Err("chains map cannot be empty".into());
        }

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
        let mut chains = HashMap::new();
        chains.insert(1, ChainConfig::default());

        Self {
            server: ServerConfig::default(),
            load_balancing: LoadBalancingStrategy::default(),
            upstream_health_checks: UpstreamHealthChecksConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
            logging: LoggingConfig::default(),
            cache: CacheConfig::Disabled,
            canned_responses: CannedResponseConfig::default(),
            chains,
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

// TODO: audit the default values for everything
impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain: Chain::from_id(1),
            upstreams: NonEmpty::new(UpstreamConfig {
                url: Url::parse("http://example.com").unwrap(),
                timeout: Duration::from_secs(10),
                weight: 1,
            }),
            block_time: None,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self::Disabled
    }
}

impl Default for UpstreamHealthChecksConfig {
    fn default() -> Self {
        Self {
            enabled: default_upstream_liveness_enabled(),
            interval: default_upstream_liveness_interval(),
        }
    }
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
            key_prefix: None,
        }
    }
}

impl Default for LocalCacheConfig {
    fn default() -> Self {
        Self {
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

fn deserialize_nonempty_upstreams<'de, D>(
    deserializer: D,
) -> Result<NonEmpty<UpstreamConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec: Vec<UpstreamConfig> = Vec::deserialize(deserializer)?;
    NonEmpty::from_vec(vec).ok_or_else(|| serde::de::Error::custom("upstreams cannot be empty"))
}

fn serialize_nonempty_upstreams<S>(
    upstreams: &NonEmpty<UpstreamConfig>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let vec: Vec<_> = upstreams.iter().cloned().collect();
    vec.serialize(serializer)
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
        assert!(!config.chains.is_empty());
        assert_eq!(config.chains.len(), 1);
        assert!(config.chains.contains_key(&1));
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
}

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}
