use alloy_json_rpc::{Request, Response, ResponsePayload};
use rpc_gateway_config::{ChainConfig, ErrorHandlingConfig, LoadBalancingConfig};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

use crate::cache::RpcCache;
use crate::request_pool::ChainRequestPool;

#[derive(Debug)]
pub struct ChainHandler {
    chain_config: Arc<ChainConfig>,
    request_pool: ChainRequestPool,
    cache: RpcCache,
}

impl ChainHandler {
    pub fn new(
        chain_config: ChainConfig,
        error_handling: ErrorHandlingConfig,
        load_balancing: LoadBalancingConfig,
        cache_capacity: u64,
    ) -> Self {
        info!(
            chain = ?chain_config.chain,
            cache_capacity = %cache_capacity,
            "Creating new ChainHandler"
        );

        let request_pool =
            ChainRequestPool::new(chain_config.clone(), error_handling, load_balancing);

        Self {
            chain_config: Arc::new(chain_config.clone()),
            request_pool,
            cache: RpcCache::new(cache_capacity, chain_config.chain),
        }
    }

    #[instrument(skip(self, request))]
    pub async fn handle_request(
        &self,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        let method = Cow::Owned(request.meta.method.to_string());

        // Check if this is a cacheable request
        if let Some(ttl) = self.cache.get_ttl(&method) {
            debug!(
                method = %method,
                ttl = ?ttl,
                "Method is cacheable"
            );

            // Try to get from cache first
            if let Some(cached_response) = self.cache.get(&method, &request.params).await {
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
                self.cache
                    .insert(&method, &request.params, result, ttl)
                    .await;
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
    }
}
