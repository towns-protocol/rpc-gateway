use anvil_core::eth::EthRequest;
use moka::Expiry;
use moka::future::Cache;
use redis::{AsyncCommands, FromRedisValue, RedisWrite, ToRedisArgs};
use rpc_gateway_config::{CacheConfig, ChainConfig};
use serde::{Deserialize, Serialize};
use std::{
    // hash::{DefaultHasher, Hash, Hasher},
    time::{Duration, Instant},
};
use tracing::{error, warn};

use crate::ttl::TTLManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReqRes {
    pub req: EthRequest,
    pub res: serde_json::Value,
}

impl FromRedisValue for ReqRes {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        match v {
            redis::Value::SimpleString(s) => {
                let mut s = s.clone(); // TODO: is this clone necessary?
                let reqres: ReqRes = unsafe { simd_json::from_str(&mut s) }.map_err(|e| {
                    redis::RedisError::from((
                        redis::ErrorKind::IoError,
                        "Failed to deserialize Redis value",
                        e.to_string(),
                    ))
                })?;
                Ok(reqres)
            }
            redis::Value::BulkString(s) => {
                let mut s = s.clone();
                let reqres: ReqRes = simd_json::from_slice(&mut s).map_err(|e| {
                    redis::RedisError::from((
                        redis::ErrorKind::IoError,
                        "Failed to deserialize Redis value",
                        e.to_string(),
                    ))
                })?;
                Ok(reqres)
            }
            _ => Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "Expected a simple string. Received a: ",
                format!("{:?}", v),
            ))),
        }
    }
}

impl ToRedisArgs for ReqRes {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + RedisWrite,
    {
        // Serialize the ReqRes to a JSON string
        let serialized = match serde_json::to_string(self) {
            Ok(s) => s,
            Err(_) => return, // Return early if serialization fails
        };

        // Write the serialized JSON string as a Redis argument
        out.write_arg(serialized.as_bytes());
    }
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
#[derive(Debug)]
pub struct LocalCache {
    /// The underlying cache implementation
    cache: Cache<String, CacheEntry>,
}

pub fn from_config(cache_config: &CacheConfig, chain_config: &ChainConfig) -> Option<RpcCache> {
    let block_time = match chain_config.block_time {
        Some(block_time) => block_time,
        None => {
            error!(
                chain = ?chain_config.chain,
                "Cache enabled but no block time available. Disabling cache."
            );
            return None;
        }
    };
    let ttl_manager = TTLManager::new(block_time);
    let rpc_cache_inner = match cache_config {
        CacheConfig::Disabled => {
            warn!(
                chain = ?chain_config.chain,
                "Cache disabled. Disabling cache."
            );
            return None;
        }
        CacheConfig::Local(config) => RpcCacheInner::Local(LocalCache::new(config.capacity)),
        CacheConfig::Redis(config) => {
            let url = config.url.clone();
            let client = match redis::Client::open(url) {
                Ok(client) => client,
                Err(err) => {
                    error!(error = ?err, "Failed to connect to Redis cache");
                    return None;
                }
            };

            RpcCacheInner::Redis(RedisCache::new(
                client,
                chain_config.chain.id(),
                config.key_prefix.clone(),
            ))
        }
    };
    let rpc_cache = RpcCache {
        inner: rpc_cache_inner,
        ttl_manager,
    };
    Some(rpc_cache)
}

#[derive(Debug)]
pub struct RpcCache {
    inner: RpcCacheInner,
    pub ttl_manager: TTLManager,
}

impl RpcCache {
    pub fn get_ttl(&self, req: &EthRequest) -> Option<Duration> {
        self.ttl_manager.get_ttl(req)
    }

    pub async fn get(&self, req: &EthRequest) -> Option<ReqRes> {
        match &self.inner {
            RpcCacheInner::Local(local_cache) => local_cache.get(req).await,
            RpcCacheInner::Redis(redis_cache) => redis_cache.get(req).await,
        }
    }

    pub async fn insert(&self, req: &EthRequest, response: &serde_json::Value, ttl: Duration) {
        match &self.inner {
            RpcCacheInner::Local(local_cache) => local_cache.insert(req, response, ttl).await,
            RpcCacheInner::Redis(redis_cache) => redis_cache.insert(req, response, ttl).await,
        }
    }
}

#[derive(Debug)]
pub enum RpcCacheInner {
    Local(LocalCache),
    Redis(RedisCache),
}

impl LocalCache {
    /// Creates a new cache with the given maximum capacity and block time
    pub fn new(max_capacity: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .expire_after(TtlExpiry)
            .build();
        Self { cache }
    }
}

impl LocalCache {
    fn get_key(&self, req: &EthRequest) -> String {
        // let mut hasher = DefaultHasher::new();
        // req.hash(&mut hasher);
        // hasher.finish().to_string()
        serde_json::to_string(&req).unwrap() // TODO: is this the right way to do this?
        // TODO: may avoid saving the entire request as the key.
    }

    pub async fn get(&self, req: &EthRequest) -> Option<ReqRes> {
        let key = self.get_key(req);
        self.cache.get(&key).await.map(|entry| entry.value)
    }

    pub async fn insert(&self, req: &EthRequest, response: &serde_json::Value, ttl: Duration) {
        let key = self.get_key(req);
        let reqres = ReqRes {
            req: req.clone(),
            res: response.clone(),
        };
        let entry = CacheEntry::new(reqres, ttl);
        self.cache.insert(key, entry).await;
    }
}

#[derive(Debug)]
pub struct RedisCache {
    client: redis::Client,
    /// The latest block number for this chain
    chain_id: u64,
    key_prefix: Option<String>,
}

impl RedisCache {
    pub fn new(client: redis::Client, chain_id: u64, key_prefix: Option<String>) -> Self {
        Self {
            client,
            chain_id,
            key_prefix,
        }
    }
    fn get_key(&self, req: &EthRequest) -> String {
        // let mut hasher = DefaultHasher::new();
        // self.chain_id.hash(&mut hasher);
        // req.hash(&mut hasher);
        // let key = hasher.finish().to_string();
        // if let Some(prefix) = &self.key_prefix {
        //     format!("{}:{}", prefix, key)
        // } else {
        //     key
        // }
        format!("{}:{}", self.chain_id, serde_json::to_string(&req).unwrap()) // TODO: is this the right way to do this?
    }

    pub async fn get(&self, req: &EthRequest) -> Option<ReqRes> {
        let key = self.get_key(req);
        let mut con = match self.client.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(err) => {
                error!(error = ?err, "Failed to establish Redis connection");
                return None;
            }
        };

        // Get the serialized value from Redis
        // TODO: optimize. can we store the connection and reuse it?
        let value: Result<Option<ReqRes>, _> = con.get(&key).await;
        match value {
            Ok(reqres) => reqres,
            Err(e) => {
                error!(
                    error = ?e,
                    req = ?req,
                    "Redis error",
                );
                None
            }
        }
    }

    pub async fn insert(&self, req: &EthRequest, response: &serde_json::Value, ttl: Duration) {
        let key = self.get_key(req);
        let reqres = ReqRes {
            req: req.clone(),
            res: response.clone(),
        };

        // TODO: is there a better way to store the conneciton and reuse it?
        let mut connection = match self.client.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(err) => {
                error!(
                    error = ?err,
                    "Failed to establish Redis connection"
                );
                return;
            }
        };

        let result: Result<(), _> = connection.set_ex(&key, reqres, ttl.as_secs()).await;
        match result {
            Ok(_) => {}
            Err(err) => {
                error!(
                    error = ?err,
                    "Failed to store value in Redis cache"
                );
            }
        }
    }
}
