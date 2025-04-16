use crate::config::{
    ChainConfig, ErrorHandlingConfig, LoadBalancingStrategy, UpstreamHealthChecksConfig,
};
use crate::load_balancer::{LoadBalancer, create_load_balancer};
use crate::upstream::Upstream;
use anvil_rpc::response::RpcResponse;
use nonempty::NonEmpty;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, instrument, warn};

#[derive(Debug, Clone)]
pub struct ChainRequestPool {
    error_handling: Arc<ErrorHandlingConfig>,
    pub load_balancer: Arc<dyn LoadBalancer>,
}

pub enum RequestPoolError {
    NoUpstreamsAvailable,
    UpstreamError(Box<dyn std::error::Error>),
}

impl ChainRequestPool {
    pub fn new(
        chain_config: ChainConfig,
        error_handling: ErrorHandlingConfig,
        load_balancing_strategy: LoadBalancingStrategy,
        upstream_health_checks_config: UpstreamHealthChecksConfig,
    ) -> Self {
        debug!(
            chain = ?chain_config.chain,
            "Creating new ChainRequestPool"
        );

        let upstreams = NonEmpty::from_vec(
            chain_config
                .upstreams
                .iter()
                .map(|config| Arc::new(Upstream::new(config.clone(), chain_config.chain)))
                .collect::<Vec<_>>(),
        )
        .expect("Chain config must have at least one upstream");

        debug!(upstreams = ?upstreams, "Created upstreams");

        Self {
            error_handling: Arc::new(error_handling),
            load_balancer: create_load_balancer(
                load_balancing_strategy,
                upstream_health_checks_config,
                upstreams,
            ),
        }
    }

    #[instrument(skip(self))]
    pub async fn forward_request(&self, raw_call: &Value) -> Result<RpcResponse, RequestPoolError> {
        let upstream = match self.load_balancer.select_upstream() {
            Some(upstream) => upstream,
            None => {
                return Err(RequestPoolError::NoUpstreamsAvailable);
            }
        };
        match &*self.error_handling {
            ErrorHandlingConfig::Retry {
                max_retries,
                retry_delay,
                jitter,
            } => {
                debug!(
                    max_retries = %max_retries,
                    retry_delay = ?retry_delay,
                    jitter = %jitter,
                    "Using retry strategy"
                );
                upstream
                    .forward_with_retry(raw_call, *max_retries, *retry_delay, *jitter)
                    .await
                    .map_err(|err| {
                        error!("Error forwarding request: {}", err);
                        RequestPoolError::UpstreamError(err)
                    })
            }
            ErrorHandlingConfig::FailFast { .. } => {
                debug!("Using fail-fast strategy");
                upstream.forward_once(raw_call).await.map_err(|err| {
                    error!("Error forwarding request: {}", err);
                    RequestPoolError::UpstreamError(err)
                })
            }
            ErrorHandlingConfig::CircuitBreaker { .. } => {
                warn!(
                    "Circuit breaker strategy not yet implemented, falling back to single attempt"
                );
                upstream.forward_once(raw_call).await.map_err(|err| {
                    error!("Error forwarding request: {}", err);
                    RequestPoolError::UpstreamError(err)
                })
            }
        }
    }
}
