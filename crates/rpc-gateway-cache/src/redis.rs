use std::time::Duration;

use anvil_core::eth::EthRequest;
use redis::{AsyncCommands, FromRedisValue, RedisWrite, ToRedisArgs};
use tracing::error;

use crate::reqres::ReqRes;

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
