use alloy_chains::Chain;
use moka::Expiry;
use moka::future::Cache;
use serde_json::Value;
use std::{
    borrow::Cow,
    hash::{DefaultHasher, Hash, Hasher},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct ReqRes {
    pub method: Cow<'static, str>,
    pub params: Value,
    pub response: Value,
}

/// Represents a cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// The actual value stored in the cache
    pub value: ReqRes,
    /// Duration after which this entry should expire
    pub ttl: Duration,
}

impl CacheEntry {
    /// Creates a new cache entry with the given value and TTL
    pub fn new(value: ReqRes, ttl: Duration) -> Self {
        Self { value, ttl }
    }
}

/// An expiry policy that uses the TTL from the cache entry
#[derive(Debug)]
pub struct TtlExpiry;

impl Expiry<String, CacheEntry> for TtlExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &CacheEntry,
        _: Instant,
    ) -> Option<Duration> {
        Some(value.ttl)
    }

    fn expire_after_update(
        &self,
        key: &String,
        value: &CacheEntry,
        updated_at: Instant,
        _: Option<Duration>,
    ) -> Option<Duration> {
        self.expire_after_create(key, value, updated_at)
    }
}

/// A cache implementation with field-level TTL
#[derive(Debug, Clone)]
pub struct RpcCache {
    /// The underlying cache implementation
    cache: Cache<String, CacheEntry>,
    /// The block time for this chain
    block_time: Duration,
}

impl RpcCache {
    /// Creates a new cache with the given maximum capacity and block time
    pub fn new(max_capacity: u64, block_time: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .expire_after(TtlExpiry)
            .build();
        Self { cache, block_time }
    }

    fn hash_key(method: &Cow<'static, str>, params: &Value) -> String {
        let mut hasher = DefaultHasher::new();
        method.hash(&mut hasher);
        params.hash(&mut hasher);
        hasher.finish().to_string()
    }

    /// Gets a value from the cache if it exists
    pub async fn get(&self, method: &Cow<'static, str>, params: &Value) -> Option<ReqRes> {
        let key = RpcCache::hash_key(&method, params);
        let reqres = match self.cache.get(&key).await {
            Some(entry) => entry.value,
            None => return None,
        };

        if reqres.method.eq(method) && reqres.params.eq(params) {
            Some(reqres)
        } else {
            None
        }
    }

    /// Inserts a value into the cache with the given TTL
    pub async fn insert(
        &self,
        method: &Cow<'static, str>,
        params: &Value,
        response: &Value,
        ttl: Duration,
    ) {
        let key = RpcCache::hash_key(method, params);
        let reqres = ReqRes {
            method: method.clone(),
            params: params.clone(),
            response: response.clone(),
        };
        let entry = CacheEntry::new(reqres, ttl);
        self.cache.insert(key.to_string(), entry).await;
    }

    /// Returns the TTL for a given method if it's cacheable
    pub fn get_ttl(&self, method: &str) -> Option<Duration> {
        match method {
            "eth_blockNumber" => Some(Duration::from_secs(1)),
            "eth_getBalance" => Some(Duration::from_secs(10)),
            "eth_getTransactionCount" => Some(Duration::from_secs(10)),
            "eth_getCode" => Some(Duration::from_secs(300)), // 5 minutes
            "eth_call" => Some(Duration::from_secs(1)),
            "eth_estimateGas" => Some(Duration::from_secs(1)),
            "eth_gasPrice" => Some(Duration::from_secs(10)),
            "eth_maxPriorityFeePerGas" => Some(Duration::from_secs(10)),
            "eth_feeHistory" => Some(Duration::from_secs(10)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_cache_ttl() {
        let block_time = Duration::from_secs(12);
        let cache = RpcCache::new(100, block_time);
        let method = Cow::Borrowed("eth_getBalance");
        let params = Value::Array(vec![
            Value::String("0x123".to_string()),
            Value::String("latest".to_string()),
        ]);
        let response = Value::String("0x1000".to_string());
        let ttl = Duration::from_millis(100);

        // Insert a value
        cache.insert(&method, &params, &response, ttl).await;

        // Value should be present immediately
        let cached = cache.get(&method, &params).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().response, response);

        // Wait for TTL to expire
        sleep(ttl + Duration::from_millis(10)).await;

        // Value should be expired
        assert!(cache.get(&method, &params).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_different_methods() {
        let block_time = Duration::from_secs(12);
        let cache = RpcCache::new(100, block_time);
        let ttl = Duration::from_secs(60);

        // Insert values for different methods
        let method1 = Cow::Borrowed("eth_getBalance");
        let method2 = Cow::Borrowed("eth_blockNumber");
        let params1 = Value::Array(vec![Value::String("0x123".to_string())]);
        let params2 = Value::Array(vec![]);
        let response1 = Value::String("0x1000".to_string());
        let response2 = Value::String("0x1234".to_string());

        cache.insert(&method1, &params1, &response1, ttl).await;
        cache.insert(&method2, &params2, &response2, ttl).await;

        // Both values should be present
        let cached1 = cache.get(&method1, &params1).await;
        let cached2 = cache.get(&method2, &params2).await;

        assert!(cached1.is_some());
        assert!(cached2.is_some());
        assert_eq!(cached1.unwrap().response, response1);
        assert_eq!(cached2.unwrap().response, response2);
    }

    #[tokio::test]
    async fn test_cache_different_params() {
        let block_time = Duration::from_secs(12);
        let cache = RpcCache::new(100, block_time);
        let method = Cow::Borrowed("eth_getBalance");
        let ttl = Duration::from_secs(60);

        // Insert values for different addresses
        let params1 = Value::Array(vec![
            Value::String("0x123".to_string()),
            Value::String("latest".to_string()),
        ]);
        let params2 = Value::Array(vec![
            Value::String("0x456".to_string()),
            Value::String("latest".to_string()),
        ]);
        let response1 = Value::String("0x1000".to_string());
        let response2 = Value::String("0x2000".to_string());

        cache.insert(&method, &params1, &response1, ttl).await;
        cache.insert(&method, &params2, &response2, ttl).await;

        // Both values should be present
        let cached1 = cache.get(&method, &params1).await;
        let cached2 = cache.get(&method, &params2).await;

        assert!(cached1.is_some());
        assert!(cached2.is_some());
        assert_eq!(cached1.unwrap().response, response1);
        assert_eq!(cached2.unwrap().response, response2);
    }

    #[test]
    fn test_ttl_values() {
        let block_time = Duration::from_secs(12);
        let cache = RpcCache::new(100, block_time);

        assert_eq!(
            cache.get_ttl("eth_blockNumber"),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            cache.get_ttl("eth_getBalance"),
            Some(Duration::from_secs(10))
        );
        assert_eq!(cache.get_ttl("eth_getCode"), Some(Duration::from_secs(300)));
        assert_eq!(cache.get_ttl("eth_sendTransaction"), None);
    }
}
