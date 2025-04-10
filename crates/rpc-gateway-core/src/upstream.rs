use crate::config::UpstreamConfig;
use alloy_chains::Chain;
use alloy_json_rpc::{Id, Request, Response, ResponsePayload};
use alloy_primitives::{ChainId, U64};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug, Clone)]
pub struct Upstream {
    pub config: UpstreamConfig,
    pub current_weight: f64,
    pub chain: Chain,
    pub client: Client,
}

impl Upstream {
    pub fn new(config: UpstreamConfig, chain: Chain) -> Self {
        Self {
            current_weight: config.weight as f64,
            config,
            chain,
            client: Client::new(),
        }
    }

    pub fn apply_weight_decay(&mut self, decay: f64) {
        self.current_weight *= decay;
    }

    pub fn reset_weight(&mut self) {
        self.current_weight = self.config.weight as f64;
    }

    // TODO: make this more efficient
    #[instrument(skip(self))]
    pub async fn readiness_probe(&self) -> bool {
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

            let request = Request::new("eth_chainId", Id::Number(1), Value::Null);

            let response = match self
                .client
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
