use crate::cache::RpcCache;
use crate::request_pool::{ChainRequestPool, RequestPoolError};
use anvil_core::eth::EthRequest;
use anvil_rpc::error::RpcError;
use anvil_rpc::request::{RpcCall, RpcMethodCall};
use anvil_rpc::response::{ResponseResult, RpcResponse};
use dashmap::DashMap;
use futures::FutureExt;
use futures::future::Shared;
use metrics::{counter, histogram};
use rpc_gateway_config::{
    CannedResponseConfig, ChainConfig, ProjectConfig, RequestCoalescingConfig,
};
use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, instrument, trace, warn};

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
    in_flight_requests: Arc<DashMap<String, SharedResponseFuture>>, // TODO: is there a max size here? what's the limit?
}
use std::sync::LazyLock;

static CANNED_RESPONSE_CLIENT_VERSION: LazyLock<ResponseResult> = LazyLock::new(|| {
    let version = env!("CARGO_PKG_VERSION");
    ResponseResult::Success(serde_json::json!(format!("RPC-Gateway/{}", version)))
});

impl ChainHandler {
    pub fn new(
        chain_config: &ChainConfig,
        request_coalescing_config: &RequestCoalescingConfig,
        canned_responses_config: &CannedResponseConfig,
        request_pool: ChainRequestPool,
        cache: Option<Box<dyn RpcCache>>,
    ) -> Self {
        Self {
            chain_config: Arc::new(chain_config.clone()),
            request_pool: Arc::new(request_pool),
            cache: cache.map(Arc::new),
            request_coalescing_config: request_coalescing_config.clone(),
            canned_responses_config: canned_responses_config.clone(),
            in_flight_requests: Arc::new(DashMap::new()),
        }
    }

    /// handle a single RPC method call
    pub async fn handle_call(
        &self,
        call: RpcCall,
        project_config: &ProjectConfig,
    ) -> Option<RpcResponse> {
        match call {
            RpcCall::MethodCall(call) => {
                trace!(target: "rpc", id = ?call.id , method = ?call.method,  "handling call");
                Some(self.on_method_call(call, project_config).await)
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
    async fn on_method_call(
        &self,
        call: RpcMethodCall,
        project_config: &ProjectConfig,
    ) -> RpcResponse {
        let chain_id = self.chain_config.chain.id().to_string();
        let RpcMethodCall { method, id, .. } = call;

        let start_time = std::time::Instant::now();

        let raw_call = serde_json::json!({
            "id": id,
            "jsonrpc": "2.0", // TODO: is this part necessary? maybe we can remove the jsonrpc field?
            "method": method.clone(),
            "params": call.params
        });

        let chain_handler_response = self.on_request(raw_call).await;

        debug!(
          chain_id = chain_id,
          rpc_method = ?method,
          response_success = ?chain_handler_response.response_result,
          response_source = ?chain_handler_response.response_source,
          gateway_project = ?project_config.name,
          "RPC response ready",
        );

        let source: Cow<'static, str> = chain_handler_response.response_source.into(); // TODO: is this the right way to do this?
        let success = match chain_handler_response.response_result {
            ResponseResult::Success(_) => "true",
            ResponseResult::Error(_) => "false",
        };

        counter!("rpc_responses_total",
          "chain_id" => chain_id.clone(),
          "rpc_method" => method.clone(),
          "response_success" => success,
          "response_source" => source.clone(),
          "gateway_project" => project_config.name.clone(), // TODO: this should come from the span
        )
        .increment(1);

        let response_result = chain_handler_response.response_result.clone();

        let duration = start_time.elapsed();
        histogram!("method_call_latency_seconds",
          "chain_id" => chain_id.clone(),
          "rpc_method" => method.clone(),
          "response_success" => success,
          "response_source" => source.clone(),
          "gateway_project" => project_config.name.clone(),
        )
        .record(duration.as_secs_f64());

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
        raw_call: serde_json::Value,
        req: Result<EthRequest, serde_json::Error>,
    ) -> ChainHandlerResponse {
        let coalescing_key = serde_json::to_string(&raw_call).unwrap(); // TODO: is this the right way to do this?

        let (outer_fut, coalesced) = {
            let request_pool = self.request_pool.clone();
            let raw_call = raw_call.clone();
            let cache = self.cache.clone();
            let in_flight_requests = self.in_flight_requests.clone();
            match self.in_flight_requests.entry(coalescing_key.clone()) {
                dashmap::Entry::Occupied(e) => {
                    trace!(?coalescing_key, "returning existing future");
                    (e.get().clone(), true)
                }
                dashmap::Entry::Vacant(e) => {
                    // TODO: consider reusing the cache key here.
                    let inner_fut = cache_then_upstream(request_pool, cache, raw_call, req)
                        .boxed()
                        .shared();

                    let timeout = Duration::from_millis(500); // TODO: make this configurable

                    let inner_fut_for_removal = inner_fut.clone();
                    let coalescing_key_for_removal = coalescing_key.clone();

                    // TODO: consider capping the dashmap size

                    tokio::spawn(async move {
                        // TODO: can just use a tokio::time::timeout here
                        let did_complete = tokio::select!(
                            _ = inner_fut_for_removal => true,
                            _ = tokio::time::sleep(timeout) => false
                        );

                        trace!(
                            ?coalescing_key_for_removal,
                            did_complete = ?did_complete,
                            "removing coalesced request future"
                        );
                        in_flight_requests.remove(&coalescing_key_for_removal);
                    });

                    trace!(?coalescing_key, "storing coalesced request future");
                    e.insert(inner_fut.clone());
                    (inner_fut, false)
                }
            }
        };

        // Await the future
        let result = outer_fut.await;

        if coalesced {
            debug!(?coalescing_key, "coalescing complete");

            return ChainHandlerResponse {
                response_source: ChainHandlerResponseSource::Coalesced,
                response_result: result.response_result,
            };
        }

        result
    }

    #[instrument(skip(self, raw_call))]
    async fn on_request(&self, raw_call: serde_json::Value) -> ChainHandlerResponse {
        let req = serde_json::from_value::<EthRequest>(raw_call.clone());

        let canned_response = match &req {
            Ok(req) => self.try_canned_response(req).await,
            Err(_) => None,
        };

        if let Some(response_result) = canned_response {
            // TODO: may want to cache canned responses if they are expensive to generate
            return ChainHandlerResponse {
                response_source: ChainHandlerResponseSource::Canned,
                response_result,
            };
        }

        if self.request_coalescing_config.enabled {
            self.handle_request_with_coalescing(raw_call, req).await
        } else {
            cache_then_upstream(self.request_pool.clone(), self.cache.clone(), raw_call, req).await
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

async fn forward_to_upstream(
    request_pool: Arc<ChainRequestPool>,
    raw_call: serde_json::Value,
) -> ChainHandlerResponse {
    let response = match request_pool.forward_request(&raw_call).await {
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
    };

    return response;
}

async fn cache_then_upstream(
    request_pool: Arc<ChainRequestPool>,
    cache: Option<Arc<Box<dyn RpcCache>>>,
    raw_call: serde_json::Value,
    req: Result<EthRequest, serde_json::Error>,
) -> ChainHandlerResponse {
    let req = match req {
        Ok(req) => req,
        Err(err) => {
            warn!(
                ?err,
                "Failed to parse eth request. Forwarding to upstream without caching."
            );
            return forward_to_upstream(request_pool, raw_call).await;
        }
    };

    if let Some(response_result) = try_cache_read(&cache, &req).await {
        return ChainHandlerResponse {
            response_source: ChainHandlerResponseSource::Cached,
            response_result,
        };
    }

    let response = forward_to_upstream(request_pool, raw_call).await;

    if matches!(
        response.response_source,
        ChainHandlerResponseSource::Upstream
    ) {
        try_cache_write(&cache, &req, &response.response_result).await;
    }

    response
}
