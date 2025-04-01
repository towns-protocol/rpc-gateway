use moka::future::Cache;
use moka::Expiry;
use serde_json::Value;
use std::time::{Duration, Instant};

/// Represents a cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// The actual value stored in the cache
    pub value: Value,
    /// Duration after which this entry should expire
    pub ttl: Duration,
}

impl CacheEntry {
    /// Creates a new cache entry with the given value and TTL
    pub fn new(value: Value, ttl: Duration) -> Self {
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
#[derive(Debug)]
pub struct RpcCache {
    /// The underlying cache implementation
    cache: Cache<String, CacheEntry>,
}

impl RpcCache {
    /// Creates a new cache with the given maximum capacity
    pub fn new(max_capacity: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .expire_after(TtlExpiry)
            .build();
        Self { cache }
    }

    /// Gets a value from the cache if it exists
    pub async fn get(&self, key: &str) -> Option<Value> {
        self.cache.get(key).await.map(|entry| entry.value)
    }

    /// Inserts a value into the cache with the given TTL
    pub async fn insert(&self, key: String, value: Value, ttl: Duration) {
        let entry = CacheEntry::new(value, ttl);
        self.cache.insert(key, entry).await;
    }

    /// Removes a value from the cache
    pub async fn remove(&self, key: &str) {
        self.cache.invalidate(key).await;
    }

    /// Returns the number of entries in the cache
    pub async fn len(&self) -> u64 {
        self.cache.entry_count()
    }

    /// Returns true if the cache is empty
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_cache_ttl() {
        let cache = RpcCache::new(100);
        let key = "test_key".to_string();
        let value = Value::String("test_value".to_string());
        let ttl = Duration::from_millis(100);

        // Insert a value
        cache.insert(key.clone(), value.clone(), ttl).await;

        // Value should be present immediately
        assert_eq!(cache.get(&key).await, Some(value.clone()));

        // Wait for TTL to expire
        sleep(ttl + Duration::from_millis(10)).await;

        // Value should be expired
        assert_eq!(cache.get(&key).await, None);
    }

    #[tokio::test]
    async fn test_cache_removal() {
        let cache = RpcCache::new(100);
        let key = "test_key".to_string();
        let value = Value::String("test_value".to_string());
        let ttl = Duration::from_secs(60);

        // Insert a value
        cache.insert(key.clone(), value.clone(), ttl).await;

        // Value should be present
        assert_eq!(cache.get(&key).await, Some(value.clone()));

        // Remove the value
        cache.remove(&key).await;

        // Value should be gone
        assert_eq!(cache.get(&key).await, None);
    }

    #[tokio::test]
    async fn test_cache_capacity() {
        let max_capacity = 2;
        let cache = RpcCache::new(max_capacity);
        let ttl = Duration::from_secs(60);

        // Insert values up to capacity
        cache
            .insert("key1".to_string(), Value::String("value1".to_string()), ttl)
            .await;
        cache
            .insert("key2".to_string(), Value::String("value2".to_string()), ttl)
            .await;

        // Both values should be present
        assert_eq!(cache.len().await, 2);

        // Insert one more value
        cache
            .insert("key3".to_string(), Value::String("value3".to_string()), ttl)
            .await;

        // Cache should maintain max capacity
        assert_eq!(cache.len().await, max_capacity);
    }
}
