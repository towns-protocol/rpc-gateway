use crate::config::{
    CacheConfig, ChainConfig, ErrorHandlingConfig, LoadBalancingStrategy,
    UpstreamHealthChecksConfig,
};
use anvil_core::eth::EthRequest;
use anvil_rpc::error::RpcError;
use anvil_rpc::request::{RpcCall, RpcMethodCall};
use anvil_rpc::response::{ResponseResult, RpcResponse};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

use crate::cache::RpcCache;
use crate::request_pool::ChainRequestPool;

#[derive(Debug)]
pub struct ChainHandler {
    pub chain_config: Arc<ChainConfig>,
    pub request_pool: ChainRequestPool,
    pub cache: Option<RpcCache>,
}

impl ChainHandler {
    pub fn new(
        chain_config: ChainConfig,
        error_handling: ErrorHandlingConfig,
        load_balancing: LoadBalancingStrategy,
        upstream_health_checks: UpstreamHealthChecksConfig,
        cache_config: CacheConfig,
    ) -> Self {
        info!(
            chain = ?chain_config.chain,
            cache_capacity = %cache_config.capacity,
            cache_enabled = %cache_config.enabled,
            "Creating new ChainHandler"
        );

        let request_pool = ChainRequestPool::new(
            chain_config.clone(),
            error_handling,
            load_balancing,
            upstream_health_checks,
        );

        let cache = if cache_config.enabled {
            if let Some(block_time) = chain_config.block_time {
                Some(RpcCache::new(cache_config.capacity, block_time))
            } else {
                error!(
                    chain = ?chain_config.chain,
                    "Cache enabled but no block time available. Disabling cache."
                );
                None
            }
        } else {
            None
        };

        Self {
            chain_config: Arc::new(chain_config.clone()),
            request_pool,
            cache,
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

    async fn on_method_call(&self, call: RpcMethodCall) -> RpcResponse {
        trace!(target: "rpc",  id = ?call.id , method = ?call.method, params = ?call.params, "received method call");
        let RpcMethodCall { method, id, .. } = call;

        let raw_call = serde_json::json!({
            "id": id,
            "method": method,
            "params": call.params
        });

        let response_result = match serde_json::from_value::<EthRequest>(raw_call.clone()) {
            Ok(req) => self.on_request(req, &raw_call).await,
            Err(err) => {
                let err = err.to_string();
                if err.contains("unknown variant") {
                    error!(target: "rpc", ?method, "failed to deserialize method due to unknown variant");
                    // TODO: when the method is not found, we could just forward it anyway - just so we cover more exotic chains
                    return RpcResponse::new(id, RpcError::method_not_found());
                } else {
                    error!(target: "rpc", ?method, ?err, "failed to deserialize method");
                    return RpcResponse::new(id, RpcError::invalid_params(err));
                }
            }
        };

        match response_result {
            Ok(response_result) => RpcResponse::new(id, response_result),
            Err(err) => {
                error!(target: "rpc", ?method, ?err, "failed to handle method call");
                // TODO: do better error handling here
                return RpcResponse::new(id, RpcError::internal_error());
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

    async fn on_request(
        &self,
        req: EthRequest,
        raw_call: &Value,
    ) -> Result<ResponseResult, Box<dyn std::error::Error>> {
        if let Some(response) = self.try_cache_read(&req).await {
            return Ok(response);
        }

        let response = self.request_pool.forward_request(raw_call).await;

        if let Ok(response) = &response {
            self.try_cache_write(&req, &response.result).await;
        } else {
            debug!(?req, "internal error, not caching");
        }

        return response.map(|res| res.result);
    }
}
