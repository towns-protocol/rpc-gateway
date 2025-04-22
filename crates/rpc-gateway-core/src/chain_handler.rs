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
use metrics::counter;
use serde_json::Value;
use std::borrow::Cow;
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, error, instrument, trace, warn};

use crate::cache::{LocalCache, RedisCache, RpcCache};
use crate::request_pool::{ChainRequestPool, RequestPoolError};

#[derive(Debug, Clone)]
enum ChainHandlerResponseSource {
    Upstream,
    Coalesced,
    Cached,
    Canned,
    PreUpstreamError,
}

impl From<ChainHandlerResponseSource> for Cow<'static, str> {
    fn from(source: ChainHandlerResponseSource) -> Self {
        Cow::Borrowed(match source {
            ChainHandlerResponseSource::Upstream => "upstream",
            ChainHandlerResponseSource::Coalesced => "coalesced",
            ChainHandlerResponseSource::Cached => "cached",
            ChainHandlerResponseSource::Canned => "canned",
            ChainHandlerResponseSource::PreUpstreamError => "pre_upstream_error",
        })
    }
}

#[derive(Debug, Clone)]
struct ChainHandlerResponse {
    response_source: ChainHandlerResponseSource,
    response_result: ResponseResult,
}

type BoxedResponseFuture = Pin<Box<dyn Future<Output = ChainHandlerResponse> + Send>>;
type SharedResponseFuture = Shared<BoxedResponseFuture>;

#[derive(Debug)]
pub struct ChainHandler {
    pub chain_config: Arc<ChainConfig>,
    pub request_coalescing_config: RequestCoalescingConfig,
    pub canned_responses_config: CannedResponseConfig,
    pub request_pool: Arc<ChainRequestPool>,
    pub cache: Option<Arc<Box<dyn RpcCache>>>, // TODO: is this the right way to do this?
    in_flight_requests: DashMap<String, SharedResponseFuture>, // TODO: is there a max size here? what's the limit?
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
            cache: cache.map(Arc::new),
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

        let chain_handler_response = self.on_request(&req, raw_call).await;
        debug!(
            response = ?chain_handler_response,
            "RPC response ready"
        );

        let chain_id = self.chain_config.chain.id().to_string();
        let source: Cow<'static, str> = chain_handler_response.response_source.into(); // TODO: is this the right way to do this?
        let success = match chain_handler_response.response_result {
            ResponseResult::Success(_) => "true",
            ResponseResult::Error(_) => "false",
        };

        // TODO: how can i use the x.y namespacing here? they get overwritten to x_y_z
        counter!("rpc_gateway_response",
          "chain_id" => chain_id,
          "method" => method,
          "success" => success,
          "source" => source,
        )
        .increment(1);

        let response_result = chain_handler_response.response_result.clone();

        RpcResponse::new(id, response_result)
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
        req: &EthRequest,
        raw_call: Value,
    ) -> ChainHandlerResponse {
        let mut hasher = DefaultHasher::new();
        req.hash(&mut hasher);
        let cache_key = hasher.finish().to_string();

        let (outer_fut, coalesced) = {
            let request_pool = self.request_pool.clone();
            let raw_call = raw_call.clone();
            let cache = self.cache.clone();

            match self.in_flight_requests.entry(cache_key.clone()) {
                dashmap::Entry::Occupied(e) => {
                    debug!("returning existing future");
                    (e.get().clone(), true)
                }
                dashmap::Entry::Vacant(e) => {
                    let inner_fut = cache_then_upstream(req.clone(), request_pool, cache, raw_call)
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

        if coalesced {
            // TODO: make sure you cover all edge cases
            let final_response_source = match result.response_source {
                ChainHandlerResponseSource::Cached => ChainHandlerResponseSource::Cached,
                _ => ChainHandlerResponseSource::Coalesced,
            };

            return ChainHandlerResponse {
                response_source: final_response_source,
                response_result: result.response_result,
            };
        }

        // TODO: Why do new requests appear under coalesced even though they should be cached?

        if matches!(result.response_source, ChainHandlerResponseSource::Upstream) {
            try_cache_write(&self.cache, &req, &result.response_result).await;
        }

        // Clean up
        self.in_flight_requests.remove(&cache_key);

        result
    }

    #[instrument(skip(self, req, raw_call))]
    async fn on_request(&self, req: &EthRequest, raw_call: Value) -> ChainHandlerResponse {
        if let Some(response_result) = self.try_canned_response(&req).await {
            return ChainHandlerResponse {
                response_source: ChainHandlerResponseSource::Canned,
                response_result,
            };
        }

        // Try cache read
        if let Some(response_result) = try_cache_read(&self.cache, &req).await {
            return ChainHandlerResponse {
                response_source: ChainHandlerResponseSource::Cached,
                response_result,
            };
        }

        if self.request_coalescing_config.enabled {
            self.handle_request_with_coalescing(req, raw_call).await
        } else {
            // TODO: try not cloning here
            cache_then_upstream(
                req.clone(),
                self.request_pool.clone(),
                self.cache.clone(),
                raw_call,
            )
            .await
        }
    }
}

async fn try_cache_write(
    cache: &Option<Arc<Box<dyn RpcCache>>>,
    req: &EthRequest,
    res: &ResponseResult,
) {
    let cache = match cache {
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

async fn try_cache_read(
    cache: &Option<Arc<Box<dyn RpcCache>>>,
    req: &EthRequest,
) -> Option<ResponseResult> {
    let cache = match cache {
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

async fn cache_then_upstream(
    req: EthRequest,
    request_pool: Arc<ChainRequestPool>,
    cache: Option<Arc<Box<dyn RpcCache>>>,
    raw_call: Value,
) -> ChainHandlerResponse {
    if let Some(response_result) = try_cache_read(&cache, &req).await {
        return ChainHandlerResponse {
            response_source: ChainHandlerResponseSource::Cached,
            response_result,
        };
    }

    match request_pool.forward_request(&raw_call).await {
        Ok(response) => ChainHandlerResponse {
            response_source: ChainHandlerResponseSource::Upstream,
            response_result: response.result,
        },
        Err(RequestPoolError::NoUpstreamsAvailable) => ChainHandlerResponse {
            response_source: ChainHandlerResponseSource::PreUpstreamError,
            response_result: ResponseResult::Error(RpcError::internal_error_with(
                "No upstreams available",
            )),
        },
        Err(err) => ChainHandlerResponse {
            response_source: ChainHandlerResponseSource::Upstream,
            response_result: ResponseResult::Error(
                // TODO: is this the right way to handle this?
                RpcError::internal_error_with(format!("{:?}", err)),
            ),
        },
    }
}
