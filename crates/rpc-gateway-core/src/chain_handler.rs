use crate::config::{
    CacheConfig, ChainConfig, Config, ErrorHandlingConfig, LoadBalancingStrategy,
    UpstreamHealthChecksConfig,
};
use alloy_json_rpc::{Request, Response, ResponsePayload};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

use crate::cache::RpcCache;
use crate::request_pool::ChainRequestPool;

#[derive(Debug, Clone)]
pub struct ChainHandler {
    pub chain_config: Arc<ChainConfig>,
    pub request_pool: ChainRequestPool,
    cache: Option<RpcCache>,
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

    #[instrument(skip(self, request))]
    pub async fn handle_request(
        &self,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        let method = Cow::Owned(request.meta.method.to_string());

        // Check if cache is enabled and if this is a cacheable request
        if let Some(cache) = &self.cache {
            if let Some(ttl) = cache.get_ttl(&method) {
                debug!(
                    method = %method,
                    ttl = ?ttl,
                    "Method is cacheable"
                );

                // Try to get from cache first
                if let Some(cached_response) = cache.get(&method, &request.params).await {
                    info!(
                        method = %method,
                        "Cache hit"
                    );
                    return Ok(Response {
                        id: request.meta.id,
                        payload: ResponsePayload::Success(cached_response.response),
                    });
                }

                info!(
                    method = %method,
                    "Cache miss"
                );

                // If not in cache, forward to request pool
                let response = self.request_pool.forward_request(request.clone()).await?;

                // Cache successful responses
                if let ResponsePayload::Success(result) = &response.payload {
                    debug!(
                        method = %method,
                        ttl = ?ttl,
                        "Caching successful response"
                    );
                    cache.insert(&method, &request.params, result, ttl).await;
                } else {
                    warn!(
                        method = %method,
                        "Not caching error response"
                    );
                }

                Ok(response)
            } else {
                debug!(
                    method = %method,
                    "Method is not cacheable"
                );
                // Non-cacheable request, forward directly
                self.request_pool.forward_request(request).await
            }
        } else {
            debug!(
                method = %method,
                "Cache is disabled"
            );
            // Cache is disabled, forward directly
            self.request_pool.forward_request(request).await
        }
    }
}
