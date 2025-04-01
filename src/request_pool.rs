use alloy_json_rpc::{Request, Response};
use rand::Rng;
use reqwest::Client;
use rpc_gateway_config::{
    ChainConfig, Config, ErrorHandlingConfig, LoadBalancingConfig, UpstreamConfig,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug, Clone)]
struct Upstream {
    config: UpstreamConfig,
    current_weight: f64,
    active_connections: u32,
}

impl Upstream {
    fn new(config: UpstreamConfig) -> Self {
        Self {
            current_weight: config.weight as f64,
            active_connections: 0,
            config,
        }
    }

    fn apply_weight_decay(&mut self, decay: f64) {
        self.current_weight *= decay;
    }

    fn reset_weight(&mut self) {
        self.current_weight = self.config.weight as f64;
    }
}

#[derive(Debug)]
pub struct ChainRequestPool {
    chain_config: Arc<ChainConfig>,
    client: Client,
    upstreams: Arc<Mutex<Vec<Upstream>>>,
    error_handling: Arc<ErrorHandlingConfig>,
    load_balancing: Arc<LoadBalancingConfig>,
}

impl ChainRequestPool {
    pub fn new(
        chain_config: ChainConfig,
        error_handling: ErrorHandlingConfig,
        load_balancing: LoadBalancingConfig,
    ) -> Self {
        info!(
            "Creating new ChainRequestPool for chain: {:?}",
            chain_config.chain
        );

        let upstreams = chain_config
            .upstreams
            .iter()
            .map(|config| Upstream::new(config.clone()))
            .collect();

        Self {
            chain_config: Arc::new(chain_config),
            client: Client::new(),
            upstreams: Arc::new(Mutex::new(upstreams)),
            error_handling: Arc::new(error_handling),
            load_balancing: Arc::new(load_balancing),
        }
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
                    "Using retry strategy with max_retries={}, retry_delay={:?}, jitter={}",
                    max_retries, retry_delay, jitter
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
                        "Successfully forwarded request after {} retries",
                        current_retry
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
                            "Request failed, retrying in {:?} (attempt {}/{})",
                            delay,
                            current_retry + 1,
                            max_retries
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
        let upstream = match &*self.load_balancing {
            LoadBalancingConfig::RoundRobin => {
                debug!("Using round-robin load balancing");
                self.select_upstream_round_robin().await?
            }
            LoadBalancingConfig::WeightedRoundRobin { weight_decay } => {
                debug!(
                    "Using weighted round-robin load balancing with decay={}",
                    weight_decay
                );
                self.select_upstream_weighted_round_robin(*weight_decay)
                    .await?
            }
            LoadBalancingConfig::LeastConnections { .. } => {
                warn!(
                    "Least connections strategy not yet implemented, falling back to round-robin"
                );
                self.select_upstream_round_robin().await?
            }
        };

        debug!("Selected upstream: {:?}", upstream);
        let response = self
            .client
            .post(upstream.config.url.as_str())
            .json(&request)
            .send()
            .await?
            .json::<Response<Value>>()
            .await?;

        Ok(response)
    }

    async fn select_upstream_round_robin(&self) -> Result<Upstream, Box<dyn std::error::Error>> {
        // TODO: Implement round robin selection
        debug!("Selecting first upstream (round-robin not implemented)");
        let upstreams = self.upstreams.lock().await;
        Ok(upstreams[0].clone())
    }

    async fn select_upstream_weighted_round_robin(
        &self,
        weight_decay: f64,
    ) -> Result<Upstream, Box<dyn std::error::Error>> {
        let mut upstreams = self.upstreams.lock().await;

        // Find the upstream with the highest weight
        let mut max_weight = f64::NEG_INFINITY;
        let mut selected_index = 0;
        for (i, upstream) in upstreams.iter().enumerate() {
            if upstream.current_weight > max_weight {
                max_weight = upstream.current_weight;
                selected_index = i;
            }
        }

        debug!(
            "Selected upstream {} with weight {}",
            selected_index, max_weight
        );

        // Apply weight decay
        upstreams[selected_index].apply_weight_decay(weight_decay);
        debug!(
            "Applied weight decay, new weight: {}",
            upstreams[selected_index].current_weight
        );

        Ok(upstreams[selected_index].clone())
    }
}
