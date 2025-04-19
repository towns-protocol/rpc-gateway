use crate::config::{
    CacheConfig, CannedResponseConfig, ChainConfig, Config, RequestCoalescingConfig,
};
use anvil_core::eth::EthRequest;
use anvil_rpc::error::RpcError;
use anvil_rpc::request::{RpcCall, RpcMethodCall};
use anvil_rpc::response::{ResponseResult, RpcResponse};
use dashmap::DashMap;
use futures::FutureExt;
use futures::future::Shared;
use serde_json::Value;
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::cache::{LocalCache, RedisCache, RpcCache};
use crate::request_pool::{ChainRequestPool, RequestPoolError};

type BoxedResponseFuture =
    Pin<Box<dyn Future<Output = Result<ResponseResult, RequestPoolError>> + Send>>;
type SharedResponseFuture = Shared<BoxedResponseFuture>;

#[derive(Debug)]
pub struct ChainHandler {
    pub chain_config: Arc<ChainConfig>,
    pub request_coalescing_config: RequestCoalescingConfig,
    pub canned_responses_config: CannedResponseConfig,
    pub request_pool: Arc<ChainRequestPool>,
    pub cache: Option<Box<dyn RpcCache>>, // TODO: is this the right way to do this?
    in_flight_requests: DashMap<String, SharedResponseFuture>,
}
use std::sync::LazyLock;

static CANNED_RESPONSE_CLIENT_VERSION: LazyLock<ResponseResult> = LazyLock::new(|| {
    let version = env!("CARGO_PKG_VERSION");
    ResponseResult::Success(serde_json::json!(format!("RPC-Gateway/{}", version)))
});

impl ChainHandler {
    pub fn new(chain_config: &ChainConfig, config: &Config) -> Self {
        let request_pool = ChainRequestPool::new(
            chain_config.clone(),
            config.error_handling.clone(),
            config.load_balancing.clone(),
            config.upstream_health_checks.clone(),
        );

        let cache = match (config.cache.clone(), chain_config.block_time) {
            (CacheConfig::Disabled, _) => None,
            (_, None) => {
                error!(
                    chain = ?chain_config.chain,
                    "Cache enabled but no block time available. Disabling cache."
                );
                None
            }
            (CacheConfig::Local(config), Some(block_time)) => {
                let cache: Box<dyn RpcCache> =
                    Box::new(LocalCache::new(config.capacity, block_time));
                Some(cache)
            }
            (CacheConfig::Redis(config), Some(block_time)) => {
                match redis::Client::open(config.url) {
                    Ok(client) => {
                        let cache: Box<dyn RpcCache> = Box::new(RedisCache::new(
                            client,
                            block_time,
                            chain_config.chain.id(),
                            config.key_prefix,
                        ));
                        Some(cache)
                    }
                    Err(err) => {
                        error!(
                            chain = ?chain_config.chain,
                            err = %err,
                            "Failed to connect to Redis cache"
                        );
                        panic!("Failed to connect to Redis cache.");
                    }
                }
            }
        };

        Self {
            chain_config: Arc::new(chain_config.clone()),
            request_pool: Arc::new(request_pool),
            cache,
            request_coalescing_config: config.request_coalescing.clone(),
            canned_responses_config: config.canned_responses.clone(),
            in_flight_requests: DashMap::new(),
        }
    }

    /// handle a single RPC method call
    pub async fn handle_call(&self, call: RpcCall) -> Option<RpcResponse> {
        match call {
            RpcCall::MethodCall(call) => {
                trace!(target: "rpc", id = ?call.id , method = ?call.method,  "handling call");
                Some(self.on_method_call(call).await)
            }
            RpcCall::Notification(notification) => {
                // TODO: handle notifications
                trace!(target: "rpc", method = ?notification.method, "received rpc notification");
                None
            }
            RpcCall::Invalid { id } => {
                warn!(target: "rpc", ?id,  "invalid rpc call");
                Some(RpcResponse::invalid_request(id))
            }
        }
    }

    // TODO: how does anvil convert from RpcMethodCall to EthRequest? Do they also parse-down to json first?
    async fn on_method_call(&self, call: RpcMethodCall) -> RpcResponse {
        trace!(target: "rpc",  id = ?call.id , method = ?call.method, params = ?call.params, "received method call");
        let RpcMethodCall { method, id, .. } = call;

        let raw_call = serde_json::json!({
            "id": id,
            "jsonrpc": "2.0", // TODO: is this part necessary? maybe we can remove the jsonrpc field?
            "method": method,
            "params": call.params
        });

        let req = match serde_json::from_value::<EthRequest>(raw_call.clone()) {
            Ok(req) => req,
            Err(err) => {
                let err = err.to_string();
                if err.contains("unknown variant") {
                    error!(
                        target: "rpc",
                        method = ?method,
                        "Failed to deserialize method due to unknown variant"
                    );
                    // TODO: when the method is not found, we could just forward it anyway - just so we cover more exotic chains
                    return RpcResponse::new(id, RpcError::method_not_found());
                } else {
                    error!(
                        target: "rpc",
                        method = ?method,
                        error = ?err,
                        "Failed to deserialize method"
                    );
                    return RpcResponse::new(id, RpcError::invalid_params(err));
                }
            }
        };

        let response_result = self.on_request(req, &raw_call).await;
        RpcResponse::new(id, response_result)
    }

    async fn try_cache_read(&self, req: &EthRequest) -> Option<ResponseResult> {
        let cache = match &self.cache {
            Some(cache) => cache,
            None => return None,
        };

        let cache_ttl = cache.get_ttl(&req);

        if cache_ttl.is_some() {
            debug!(?req, "method is cacheable");
            if let Some(response) = cache.get(&req).await {
                debug!(?req, "cache hit");
                return Some(ResponseResult::Success(response.res));
            } else {
                debug!(?req, "cache miss");
            }
        } else {
            debug!(?req, "method is not cacheable");
        }
        None
    }

    async fn try_cache_write(&self, req: &EthRequest, res: &ResponseResult) {
        let cache = match &self.cache {
            Some(cache) => cache,
            None => return,
        };
        let cache_ttl = match cache.get_ttl(&req) {
            Some(cache_ttl) => cache_ttl,
            None => return,
        };
        let successful_response_result = match res {
            ResponseResult::Success(res) => res,
            ResponseResult::Error(_) => {
                debug!(?req, "method returned error, not caching");
                return;
            }
        };
        debug!(?req, "caching response");
        cache
            .insert(&req, successful_response_result, cache_ttl)
            .await;
    }

    async fn try_canned_response(&self, req: &EthRequest) -> Option<ResponseResult> {
        if !self.canned_responses_config.enabled {
            return None;
        }

        match req {
            EthRequest::Web3ClientVersion(_)
                if self.canned_responses_config.methods.web3_client_version =>
            {
                Some(CANNED_RESPONSE_CLIENT_VERSION.clone())
            }
            EthRequest::EthChainId(_) if self.canned_responses_config.methods.eth_chain_id => {
                Some(ResponseResult::Success(serde_json::json!(format!(
                    "0x{:x}",
                    self.chain_config.chain.id()
                ))))
            }
            // EthRequest::Web3Sha3(bytes) => todo!(), TODO: self-implement
            // EthRequest::EthNetworkId(_) => todo!(), TODO: self-implement
            _ => None,
        }
    }

    async fn handle_request_with_coalescing(
        &self,
        req: EthRequest,
        raw_call: &Value,
        method_name: &str,
    ) -> Result<ResponseResult, RequestPoolError> {
        let mut hasher = DefaultHasher::new();
        req.hash(&mut hasher);
        let cache_key = hasher.finish().to_string();

        let (outer_fut, coalesced) = {
            let request_pool = self.request_pool.clone();
            let raw_call_clone = raw_call.clone();

            match self.in_flight_requests.entry(cache_key.clone()) {
                dashmap::Entry::Occupied(e) => {
                    debug!("returning existing future");
                    (e.get().clone(), true)
                }
                dashmap::Entry::Vacant(e) => {
                    let inner_fut = async move {
                        let result = request_pool.forward_request(&raw_call_clone).await;
                        result.map(|r| r.result)
                    }
                    .boxed()
                    .shared();
                    debug!("storing new future");
                    e.insert(inner_fut.clone());
                    (inner_fut, false)
                }
            }
        };

        // Await the future
        let result = outer_fut.await;

        // Clean up
        if coalesced {
            info!(
                response_source = "coalesced",
                response_success = result.is_ok(),
                request_method = ?method_name,
                request_params = ?raw_call.get("params"),
                chain_id = %self.chain_config.chain.id(),
                "RPC response ready"
            );
            return result;
        }

        self.in_flight_requests.remove(&cache_key);

        match result {
            Ok(response) => {
                info!(
                    response_source = "upstream",
                    response_success = true,
                    request_method = ?method_name,
                    request_params = ?raw_call.get("params"),
                    chain_id = %self.chain_config.chain.id(),
                    "RPC response ready"
                );
                self.try_cache_write(&req, &response).await;
                Ok(response)
            }
            Err(RequestPoolError::NoUpstreamsAvailable) => {
                info!(
                    response_source = "error",
                    response_success = false,
                    request_method = ?method_name,
                    request_params = ?raw_call.get("params"),
                    chain_id = %self.chain_config.chain.id(),
                    error_type = "no_upstreams",
                    "RPC response ready"
                );
                Ok(ResponseResult::Error(RpcError::internal_error_with(
                    "No upstreams available",
                )))
            }
            Err(request_pool_error) => {
                // TODO: how can we only log the params as a debug log while keeping the rest in info?
                info!(
                    response_source = "error",
                    response_success = false,
                    request_method = ?method_name,
                    request_params = ?raw_call.get("params"),
                    chain_id = %self.chain_config.chain.id(),
                    error = ?request_pool_error,
                    "RPC response ready"
                );
                Err(request_pool_error)
            }
        }
    }

    #[instrument(skip(self, req, raw_call))]
    async fn on_request(&self, req: EthRequest, raw_call: &Value) -> ResponseResult {
        // TODO: log the actual params in string format here to help understand why getTransactionCount is not getting any cache hits.
        let method_name = raw_call
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Try canned response first
        if let Some(response_result) = self.try_canned_response(&req).await {
            info!(
                response_source = "canned",
                response_success = true,
                request_method = ?method_name,
                request_params = ?raw_call.get("params"),
                chain_id = %self.chain_config.chain.id(),
                "RPC response ready"
            );
            return response_result;
        }

        // Try cache read
        if let Some(response_result) = self.try_cache_read(&req).await {
            info!(
                response_source = "cache",
                response_success = true,
                request_method = ?method_name,
                request_params = ?raw_call.get("params"),
                chain_id = %self.chain_config.chain.id(),
                "RPC response ready"
            );
            return response_result;
        }

        // self.forward_to_upstream(req, raw_call, method_name).await
        match self
            .handle_request_with_coalescing(req, raw_call, method_name)
            .await
        {
            Ok(response) => response,
            Err(err) => {
                error!(?err, "failed to handle request");
                // TODO: do better error handling here
                ResponseResult::Error(RpcError::internal_error_with(format!("{:?}", err)))
            }
        }
    }
}
