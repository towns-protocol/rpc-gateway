use crate::config::{
    CacheConfig, CannedResponseConfig, ChainConfig, ErrorHandlingConfig, LoadBalancingStrategy,
    UpstreamHealthChecksConfig,
};
use anvil_core::eth::EthRequest;
use anvil_rpc::error::RpcError;
use anvil_rpc::request::{RpcCall, RpcMethodCall};
use anvil_rpc::response::{ResponseResult, RpcResponse};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

use crate::cache::{LocalCache, RedisCache, RpcCache};
use crate::request_pool::{ChainRequestPool, RequestPoolError};

#[derive(Debug)]
pub struct ChainHandler {
    pub chain_config: Arc<ChainConfig>,
    pub request_pool: ChainRequestPool,
    pub cache: Option<Box<dyn RpcCache>>, // TODO: is this the right way to do this?
    pub canned_responses_config: CannedResponseConfig,
}
use std::sync::LazyLock;

static CANNED_RESPONSE_CLIENT_VERSION: LazyLock<ResponseResult> = LazyLock::new(|| {
    let version = env!("CARGO_PKG_VERSION");
    ResponseResult::Success(serde_json::json!(format!("RPC-Gateway/{}", version)))
});

impl ChainHandler {
    pub fn new(
        chain_config: ChainConfig,
        error_handling: ErrorHandlingConfig,
        load_balancing: LoadBalancingStrategy,
        upstream_health_checks: UpstreamHealthChecksConfig,
        cache_config: CacheConfig,
        canned_responses: CannedResponseConfig,
    ) -> Self {
        info!(
            chain = ?chain_config.chain,
            cache_config = ?cache_config,
            "Creating new ChainHandler"
        );

        let request_pool = ChainRequestPool::new(
            chain_config.clone(),
            error_handling,
            load_balancing,
            upstream_health_checks,
        );

        let cache = match (cache_config, chain_config.block_time) {
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
            request_pool,
            cache,
            canned_responses_config: canned_responses,
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

        match self.on_request(req, &raw_call).await {
            Ok(response_result) => RpcResponse::new(id, response_result),
            Err(err) => {
                error!(
                    target: "rpc",
                    method = ?method,
                    error = ?err,
                    "Failed to handle method call"
                );
                // TODO: do better error handling here
                RpcResponse::new(id, RpcError::internal_error())
            }
        }
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

    async fn on_request(
        &self,
        req: EthRequest,
        raw_call: &Value,
    ) -> Result<ResponseResult, Box<dyn std::error::Error>> {
        let method_name = raw_call
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if let Some(response) = self.try_canned_response(&req).await {
            info!(
                response_source = "canned",
                response_success = true,
                request_method = ?method_name,
                chain_id = %self.chain_config.chain.id(),
                "RPC response ready"
            );
            return Ok(response);
        }

        if let Some(response) = self.try_cache_read(&req).await {
            info!(
                response_source = "cache",
                response_success = true,
                request_method = ?method_name,
                chain_id = %self.chain_config.chain.id(),
                "RPC response ready"
            );
            return Ok(response);
        }

        match self.request_pool.forward_request(raw_call).await {
            Ok(response) => {
                info!(
                    response_source = "upstream",
                    response_success = true,
                    request_method = ?method_name,
                    chain_id = %self.chain_config.chain.id(),
                    "RPC response ready"
                );
                self.try_cache_write(&req, &response.result).await;
                Ok(response.result)
            }
            Err(RequestPoolError::NoUpstreamsAvailable) => {
                info!(
                    response_source = "error",
                    response_success = false,
                    request_method = ?method_name,
                    chain_id = %self.chain_config.chain.id(),
                    error_type = "no_upstreams",
                    "RPC response ready"
                );
                Ok(ResponseResult::Error(RpcError::internal_error_with(
                    "No upstreams available",
                )))
            }
            Err(RequestPoolError::UpstreamError(err)) => {
                info!(
                    response_source = "error",
                    response_success = false,
                    request_method = ?method_name,
                    chain_id = %self.chain_config.chain.id(),
                    error = ?err,
                    "RPC response ready"
                );
                Err(err)
            }
        }
    }
}
