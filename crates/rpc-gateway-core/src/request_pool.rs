use crate::load_balancer::LoadBalancer;
use bytes::Bytes;
use rpc_gateway_config::ErrorHandlingConfig;
use rpc_gateway_rpc::response::RpcResponse;
use rpc_gateway_upstream::upstream::UpstreamError;
use std::sync::Arc;
use tracing::{debug, error, instrument, warn};

/// Result of forwarding a request to an upstream.
pub struct ForwardResult {
    /// The RPC response from the upstream.
    pub response: RpcResponse,
    /// Name of the upstream that handled the request.
    pub upstream_name: String,
    /// Whether the request was handled by a backup upstream (failover occurred).
    pub failed_over: bool,
}

// TODO: maybe request coalescing should be done here?

/// Manages request forwarding to upstreams for a specific chain.
///
/// Handles upstream selection via the load balancer and implements failover
/// logic when upstreams fail.
#[derive(Debug, Clone)]
pub struct ChainRequestPool {
    error_handling: Arc<ErrorHandlingConfig>,
    /// The load balancer used to select upstreams for requests.
    pub load_balancer: Arc<dyn LoadBalancer>,
}

/// Errors that can occur when forwarding requests through the pool.
#[derive(Debug)]
pub enum RequestPoolError {
    /// No healthy upstreams are available to handle the request.
    NoUpstreamsAvailable,
    /// An error occurred while communicating with an upstream.
    UpstreamError(UpstreamError),
    /// All upstreams in the failover chain failed to handle the request.
    AllUpstreamsFailed,
}

impl ChainRequestPool {
    /// Creates a new request pool with the given error handling config and load balancer.
    pub fn new(error_handling: ErrorHandlingConfig, load_balancer: Arc<dyn LoadBalancer>) -> Self {
        Self {
            error_handling: Arc::new(error_handling),
            load_balancer,
        }
    }

    /// Forwards a raw RPC request to an available upstream.
    ///
    /// Attempts to forward the request to upstreams in order of priority (as determined
    /// by the load balancer). Each upstream is given its full retry budget (based on the
    /// error_handling config) before failing over to the next upstream. Returns the response
    /// from the first successful upstream, along with metadata about whether failover occurred.
    #[instrument(skip(self, raw_call))]
    pub async fn forward_request(&self, raw_call: Bytes) -> Result<ForwardResult, RequestPoolError> {
        let upstreams = self.load_balancer.select_upstreams();
        if upstreams.is_empty() {
            error!("no upstreams available");
            return Err(RequestPoolError::NoUpstreamsAvailable);
        }

        let mut last_error: Option<UpstreamError> = None;
        let mut attempted_failover = false;

        // Try each upstream in order until one succeeds
        for (index, upstream) in upstreams.iter().enumerate() {
            let is_failover = index > 0;

            if is_failover {
                attempted_failover = true;
                debug!(
                    upstream = %upstream.name(),
                    "Failing over to backup upstream"
                );
            }

            let result = match self.error_handling.as_ref() {
                ErrorHandlingConfig::Retry {
                    max_retries,
                    retry_delay,
                    jitter,
                } => {
                    upstream
                        .forward_with_retry(&raw_call, *max_retries, *retry_delay, *jitter)
                        .await
                }
                ErrorHandlingConfig::FailFast => upstream.forward_once(&raw_call).await,
                ErrorHandlingConfig::CircuitBreaker { .. } => {
                    unimplemented!(
                        "CircuitBreaker error handling is not yet implemented. \
                         Use 'fail_fast' or 'retry' instead."
                    )
                }
            };

            match result {
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
                    last_error = Some(e);
                    continue;
                }
            }
        }

        // Return appropriate error based on whether failover was attempted
        if attempted_failover {
            error!("All upstreams in failover chain failed");
            Err(RequestPoolError::AllUpstreamsFailed)
        } else {
            // Single upstream case: return the actual error
            error!("Primary upstream failed");
            Err(RequestPoolError::UpstreamError(
                last_error.expect("last_error should be set if we reached here"),
            ))
        }
    }
}
