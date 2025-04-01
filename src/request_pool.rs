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

#[derive(Debug)]
pub struct RequestPool {
    config: Arc<Config>,
    client: Client,
    upstream_weights: Arc<Mutex<Vec<f64>>>,
}

impl RequestPool {
    pub fn new(config: Config) -> Self {
        info!("Creating new RequestPool with config: {:?}", config);
        Self {
            config: Arc::new(config),
            client: Client::new(),
            upstream_weights: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[instrument(skip(self, request), fields(chain_id = %chain_id))]
    pub async fn forward_request(
        &self,
        chain_id: u64,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        debug!("Forwarding request for chain {}", chain_id);

        let chain_config = self.config.chains.get(&chain_id).ok_or_else(|| {
            error!("Chain {} not found in configuration", chain_id);
            format!("Chain {} not found", chain_id)
        })?;

        match &self.config.error_handling {
            ErrorHandlingConfig::Retry {
                max_retries,
                retry_delay,
                jitter,
            } => {
                debug!(
                    "Using retry strategy with max_retries={}, retry_delay={:?}, jitter={}",
                    max_retries, retry_delay, jitter
                );
                self.forward_with_retry(chain_config, request, *max_retries, *retry_delay, *jitter)
                    .await
            }
            ErrorHandlingConfig::FailFast { .. } => {
                debug!("Using fail-fast strategy");
                self.forward_once(chain_config, request).await
            }
            ErrorHandlingConfig::CircuitBreaker { .. } => {
                warn!(
                    "Circuit breaker strategy not yet implemented, falling back to single attempt"
                );
                self.forward_once(chain_config, request).await
            }
        }
    }

    #[instrument(skip(self, chain_config, request))]
    async fn forward_with_retry(
        &self,
        chain_config: &ChainConfig,
        request: Request<Value>,
        max_retries: u32,
        retry_delay: Duration,
        jitter: bool,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        let mut last_error = None;
        let mut current_retry = 0;

        while current_retry <= max_retries {
            match self.forward_once(chain_config, request.clone()).await {
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

    #[instrument(skip(self, chain_config, request))]
    async fn forward_once(
        &self,
        chain_config: &ChainConfig,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        let upstream = match &self.config.load_balancing {
            LoadBalancingConfig::RoundRobin => {
                debug!("Using round-robin load balancing");
                self.select_upstream_round_robin(chain_config).await?
            }
            LoadBalancingConfig::WeightedRoundRobin { weight_decay } => {
                debug!(
                    "Using weighted round-robin load balancing with decay={}",
                    weight_decay
                );
                self.select_upstream_weighted_round_robin(chain_config, *weight_decay)
                    .await?
            }
            LoadBalancingConfig::LeastConnections { .. } => {
                warn!(
                    "Least connections strategy not yet implemented, falling back to round-robin"
                );
                self.select_upstream_round_robin(chain_config).await?
            }
        };

        debug!("Selected upstream: {:?}", upstream);
        let response = self
            .client
            .post(upstream.url.as_str())
            .json(&request)
            .send()
            .await?
            .json::<Response<Value>>()
            .await?;

        Ok(response)
    }

    async fn select_upstream_round_robin<'a>(
        &self,
        chain_config: &'a ChainConfig,
    ) -> Result<&'a UpstreamConfig, Box<dyn std::error::Error>> {
        // TODO: Implement round robin selection
        debug!("Selecting first upstream (round-robin not implemented)");
        Ok(&chain_config.upstreams[0])
    }

    async fn select_upstream_weighted_round_robin<'a>(
        &self,
        chain_config: &'a ChainConfig,
        weight_decay: f64,
    ) -> Result<&'a UpstreamConfig, Box<dyn std::error::Error>> {
        let mut weights = self.upstream_weights.lock().await;
        if weights.is_empty() {
            debug!("Initializing upstream weights");
            *weights = chain_config
                .upstreams
                .iter()
                .map(|u| u.weight as f64)
                .collect();
        }

        // Find the upstream with the highest weight
        let mut max_weight = f64::NEG_INFINITY;
        let mut selected_index = 0;
        for (i, &weight) in weights.iter().enumerate() {
            if weight > max_weight {
                max_weight = weight;
                selected_index = i;
            }
        }

        debug!(
            "Selected upstream {} with weight {}",
            selected_index, max_weight
        );

        // Apply weight decay
        weights[selected_index] *= weight_decay;
        debug!(
            "Applied weight decay, new weight: {}",
            weights[selected_index]
        );

        Ok(&chain_config.upstreams[selected_index])
    }

    #[allow(dead_code)]
    async fn select_upstream_random<'a>(
        &self,
        chain_config: &'a ChainConfig,
    ) -> Result<&'a UpstreamConfig, Box<dyn std::error::Error>> {
        let mut rng = rand::rng();
        let index = rng.random_range(0..chain_config.upstreams.len());
        debug!("Randomly selected upstream {}", index);
        Ok(&chain_config.upstreams[index])
    }
}
