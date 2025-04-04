use crate::config::{ChainConfig, ErrorHandlingConfig, LoadBalancingConfig, UpstreamConfig};
use alloy_chains::Chain;
use alloy_json_rpc::{Id, Request, Response, ResponsePayload};
use alloy_primitives::{ChainId, U64};
use alloy_rpc_types::pubsub::Params;
use rand::Rng;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug, Clone)]
struct Upstream {
    config: UpstreamConfig,
    current_weight: f64,
    chain: Chain,
}

impl Upstream {
    fn new(config: UpstreamConfig, chain: Chain) -> Self {
        Self {
            current_weight: config.weight as f64,
            config,
            chain,
        }
    }

    fn apply_weight_decay(&mut self, decay: f64) {
        self.current_weight *= decay;
    }

    fn reset_weight(&mut self) {
        self.current_weight = self.config.weight as f64;
    }

    // TODO: make this more efficient
    #[instrument(skip(self, client))]
    pub async fn readiness_probe(&self, client: &Client) -> bool {
        debug!(
            url = %self.config.url,
            expected_chain_id = %self.chain.id(),
            "Checking upstream readiness"
        );

        for attempt in 0..3 {
            debug!(
                url = %self.config.url,
                attempt = attempt + 1,
                "Attempting readiness check"
            );

            let request = Request::new("eth_chainId", Id::Number(1), Params::None);

            let response = match client
                .post(self.config.url.as_str())
                .json(&request)
                .send()
                .await
            {
                Ok(response) => response,
                Err(e) => {
                    warn!(
                        url = %self.config.url,
                        attempt = attempt + 1,
                        error = %e,
                        "Failed to send request to upstream"
                    );
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    return false;
                }
            };

            let rpc_response = match response.json::<Response<Value>>().await {
                Ok(response) => response,
                Err(e) => {
                    warn!(
                        url = %self.config.url,
                        attempt = attempt + 1,
                        error = %e,
                        "Failed to parse response from upstream"
                    );
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    return false;
                }
            };

            let payload = match rpc_response.payload {
                ResponsePayload::Success(payload) => payload,
                ResponsePayload::Failure(error) => {
                    warn!(
                        url = %self.config.url,
                        attempt = attempt + 1,
                        error = ?error,
                        "Upstream returned error response"
                    );
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    return false;
                }
            };

            let chain_id: U64 = match serde_json::from_value(payload) {
                Ok(chain_id) => chain_id,
                Err(e) => {
                    warn!(
                        url = %self.config.url,
                        attempt = attempt + 1,
                        error = %e,
                        "Failed to parse chain ID from response"
                    );
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    return false;
                }
            };

            let chain_id: ChainId = chain_id.to();

            if chain_id == self.chain.id() {
                info!(
                    url = %self.config.url,
                    chain_id = %chain_id,
                    attempt = attempt + 1,
                    "Upstream is ready"
                );
                return true;
            } else {
                warn!(
                    url = %self.config.url,
                    attempt = attempt + 1,
                    expected_chain_id = %self.chain.id(),
                    received_chain_id = %chain_id,
                    "Upstream returned incorrect chain ID"
                );
                if attempt < 2 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                return false;
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
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
            chain = ?chain_config.chain,
            "Creating new ChainRequestPool"
        );

        let upstreams = chain_config
            .upstreams
            .iter()
            .map(|config| Upstream::new(config.clone(), chain_config.chain))
            .collect();

        Self {
            chain_config: Arc::new(chain_config),
            client: Client::new(),
            upstreams: Arc::new(Mutex::new(upstreams)),
            error_handling: Arc::new(error_handling),
            load_balancing: Arc::new(load_balancing),
        }
    }

    #[instrument(skip(self))]
    pub async fn readiness_probe(&self) {
        // TODO: this should exit the process if no upstreams are healthy
        let mut upstreams = self.upstreams.lock().await;
        let mut i = 0;

        while i < upstreams.len() {
            let upstream = &upstreams[i];
            if !upstream.readiness_probe(&self.client).await {
                warn!(
                    url = %upstream.config.url,
                    "Removing unhealthy upstream"
                );
                upstreams.remove(i);
            } else {
                i += 1;
            }
        }

        if upstreams.is_empty() {
            error!(
                chain = ?self.chain_config.chain,
                "No healthy upstreams remaining"
            );
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
        let upstream = match &*self.load_balancing {
            LoadBalancingConfig::RoundRobin => {
                debug!("Using round-robin load balancing");
                self.select_upstream_round_robin().await?
            }
            LoadBalancingConfig::WeightedRoundRobin { weight_decay } => {
                debug!(
                    weight_decay = %weight_decay,
                    "Using weighted round-robin load balancing"
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

        debug!(upstream = ?upstream, "Selected upstream");
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

        if selected_index >= upstreams.len() {
            error!("No upstreams available");
            return Err("No upstreams available".into());
        }

        debug!(
            selected_index = %selected_index,
            max_weight = %max_weight,
            "Selected upstream"
        );

        // Apply weight decay
        upstreams[selected_index].apply_weight_decay(weight_decay);
        debug!(
            new_weight = %upstreams[selected_index].current_weight,
            "Applied weight decay"
        );

        Ok(upstreams[selected_index].clone())
    }
}
