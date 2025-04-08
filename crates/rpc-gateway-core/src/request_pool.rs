use crate::config::{
    ChainConfig, ErrorHandlingConfig, LoadBalancingStrategy, UpstreamHealthChecksConfig,
};
use crate::load_balancer::{LoadBalancer, create_load_balancer};
use crate::upstream::Upstream;
use alloy_json_rpc::{Request, Response};
use nonempty::NonEmpty;
use rand::Rng;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug, Clone)]
pub struct ChainRequestPool {
    chain_config: Arc<ChainConfig>,
    error_handling: Arc<ErrorHandlingConfig>,
    load_balancer: Arc<dyn LoadBalancer>,
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
            chain_config: Arc::new(chain_config),
            error_handling: Arc::new(error_handling),
            load_balancer: create_load_balancer(
                load_balancing_strategy,
                upstream_health_checks_config,
                upstreams,
            ),
        }
    }

    #[instrument(skip(self))]
    pub fn start_health_check_loop(&self) {
        self.load_balancer.start_health_check_loop();
    }

    #[instrument(skip(self))]
    pub fn liveness_probe(&self) -> bool {
        self.load_balancer.liveness_probe()
    }

    #[instrument(skip(self, request))]
    pub async fn forward_request(
        &self,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
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
                self.forward_with_retry(request, *max_retries, *retry_delay, *jitter)
                    .await
            }
            ErrorHandlingConfig::FailFast { .. } => {
                debug!("Using fail-fast strategy");
                self.forward_once(request).await
            }
            ErrorHandlingConfig::CircuitBreaker { .. } => {
                warn!(
                    "Circuit breaker strategy not yet implemented, falling back to single attempt"
                );
                self.forward_once(request).await
            }
        }
    }

    #[instrument(skip(self, request))]
    async fn forward_with_retry(
        &self,
        request: Request<Value>,
        max_retries: u32,
        retry_delay: Duration,
        jitter: bool,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        let mut last_error = None;
        let mut current_retry = 0;

        while current_retry <= max_retries {
            match self.forward_once(request.clone()).await {
                Ok(response) => {
                    info!(
                        retry_count = %current_retry,
                        "Successfully forwarded request"
                    );
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);
                    if current_retry < max_retries {
                        let delay = if jitter {
                            let mut rng = rand::rng();
                            retry_delay + Duration::from_millis(rng.random_range(0..1000))
                        } else {
                            retry_delay
                        };
                        warn!(
                            delay = ?delay,
                            attempt = %current_retry + 1,
                            max_retries = %max_retries,
                            "Request failed, retrying"
                        );
                        tokio::time::sleep(delay).await;
                    }
                    current_retry += 1;
                }
            }
        }

        error!("All retry attempts failed");
        Err(last_error.unwrap_or_else(|| "Unknown error".into()))
    }

    #[instrument(skip(self, request))]
    async fn forward_once(
        &self,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        match self.load_balancer.select_upstream() {
            Some(upstream) => {
                debug!(upstream = ?upstream, "Selected upstream");
                let response = upstream
                    .client
                    .post(upstream.config.url.as_str())
                    .json(&request)
                    .send()
                    .await?
                    .json::<Response<Value>>()
                    .await?;

                Ok(response)
            }
            None => {
                error!("No upstreams available");
                Err("No upstreams available".into())
            }
        }
    }
}
