use crate::load_balancer::LoadBalancer;
use bytes::Bytes;
use rpc_gateway_config::ErrorHandlingConfig;
use rpc_gateway_rpc::response::RpcResponse;
use rpc_gateway_upstream::upstream::UpstreamError;
use std::sync::Arc;
use tracing::{debug, error, instrument, warn};

pub struct ForwardResult {
    pub response: RpcResponse,
    pub upstream_name: String,
    pub failed_over: bool,
}

// TODO: maybe request coalescing should be done here?

#[derive(Debug, Clone)]
pub struct ChainRequestPool {
    error_handling: Arc<ErrorHandlingConfig>,
    pub load_balancer: Arc<dyn LoadBalancer>,
}

#[derive(Debug)]
pub enum RequestPoolError {
    NoUpstreamsAvailable,
    UpstreamError(UpstreamError),
    AllUpstreamsFailed,
}

impl ChainRequestPool {
    pub fn new(error_handling: ErrorHandlingConfig, load_balancer: Arc<dyn LoadBalancer>) -> Self {
        Self {
            error_handling: Arc::new(error_handling),
            load_balancer,
        }
    }

    #[instrument(skip(self))]
    pub async fn forward_request(&self, raw_call: Bytes) -> Result<ForwardResult, RequestPoolError> {
        let upstreams = self.load_balancer.select_upstreams();
        if upstreams.is_empty() {
            error!("no upstreams available");
            return Err(RequestPoolError::NoUpstreamsAvailable);
        }

        let primary_name = upstreams[0].name();

        // Try each upstream in order until one succeeds
        for upstream in &upstreams {
            let is_failover = upstream.name() != primary_name;

            if is_failover {
                debug!(
                    upstream = %upstream.name(),
                    "Failing over to backup upstream"
                );
            }

            match upstream.forward_once(&raw_call).await {
                Ok(response) => {
                    return Ok(ForwardResult {
                        response,
                        upstream_name: upstream.name().to_string(),
                        failed_over: is_failover,
                    });
                }
                Err(e) => {
                    warn!(
                        upstream = %upstream.name(),
                        error = ?e,
                        "Upstream failed, trying next"
                    );
                    continue;
                }
            }
        }

        // All upstreams failed
        error!("All upstreams in failover chain failed");
        Err(RequestPoolError::AllUpstreamsFailed)
    }
}
