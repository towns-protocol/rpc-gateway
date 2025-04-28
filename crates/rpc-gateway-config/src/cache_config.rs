use serde::{Deserialize, Serialize};

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

impl Default for CacheConfig {
    fn default() -> Self {
        Self::Disabled
    }
}

impl Default for LocalCacheConfig {
    fn default() -> Self {
        Self {
            capacity: default_cache_capacity(),
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

fn default_cache_capacity() -> u64 {
    10_000 // Default cache capacity of 10,000 entries
}

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}
