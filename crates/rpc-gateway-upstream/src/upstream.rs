use std::{error::Error, time::Duration};

use alloy_chains::Chain;
use alloy_primitives::U64;
use bytes::Bytes;
use rand::Rng;
use reqwest::Client;
use rpc_gateway_config::UpstreamConfig;
use rpc_gateway_rpc::{
    error::ErrorCode,
    response::{ResponseResult, RpcResponse},
};
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug)]
pub struct Upstream {
    pub config: UpstreamConfig,
    pub current_weight: f64,
    pub chain: Chain,
    client: Client,
}

#[derive(Debug)]
pub enum UpstreamError {
    RequestError,
    ResponseError,
    JsonError,
}

use std::sync::LazyLock;

static CHAIN_ID_REQUEST: LazyLock<Bytes> = LazyLock::new(|| {
    serde_json::to_string(&serde_json::json!({
      "jsonrpc": "2.0",
      "method": "eth_chainId",
      "params": [],
      "id": 1
    }))
    .unwrap()
    .into()
});

impl Upstream {
    pub fn new(config: UpstreamConfig, chain: Chain) -> Self {
        Self {
            current_weight: config.weight as f64,
            config,
            chain,
            client: Client::builder()
                .timeout(Duration::from_secs(2)) // TODO: make this configurable
                .build()
                .unwrap(),
        }
    }

    pub fn apply_weight_decay(&mut self, decay: f64) {
        self.current_weight *= decay;
    }

    pub fn reset_weight(&mut self) {
        self.current_weight = self.config.weight as f64;
    }

    #[instrument(skip(self))]
    pub async fn readiness_probe(&self) -> bool {
        let response = match self.forward_once(&CHAIN_ID_REQUEST).await {
            Ok(response) => response,
            Err(_) => return false,
        };

        let success_result = match response.result {
            ResponseResult::Success(result) => result,
            ResponseResult::Error(_) => return false,
        };

        let chain_id: U64 = match serde_json::from_value(success_result) {
            Ok(chain_id) => chain_id,
            Err(_) => {
                error!(upstream = ?self, "Could not parse chain id in readiness probe");
                return false;
            }
        };

        let self_chain_id: U64 = U64::from(self.chain.id());

        if self_chain_id == chain_id {
            debug!(upstream = ?self, "Readiness probe passed");
            return true;
        } else {
            error!(upstream = ?self, expected_chain_id = %self_chain_id, actual_chain_id = %chain_id, "Readiness probe failed. Chain id mismatch");
            return false;
        }
    }

    // TODO: do the lazy_request trick but for the response now
    #[instrument(skip(self))]
    pub async fn forward_once(&self, raw_call: &Bytes) -> Result<RpcResponse, UpstreamError> {
        // TODO: try parsing the response as an alloy_json_rpc::Response
        // TODO: make sure the upstream errors can be represented as an RpcError.
        // TODO: otherwise, consider just checking if the response is a success or error, and returning it as a Json Value.

        let raw_response = self
            .client
            .post(self.config.url.as_str())
            .body(raw_call.clone())
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| {
                error!(?e, error_source = ?e.source(), "upstream request error");
                UpstreamError::RequestError
            })?;

        let status = raw_response.status();

        if !status.is_success() {
            error!(status = ?status, "upstream response error");
            return Err(UpstreamError::ResponseError);
        }

        // TODO: rebuild your own RpcResponse type. need to be able to access the .result field.
        let rpc_response = raw_response.bytes().await.map_err(|e| {
            error!(?e, status = ?status, error_source = ?e.source(), "upstream response error");
            UpstreamError::ResponseError
        })?;

        let rpc_response = serde_json::from_slice::<RpcResponse>(&rpc_response).map_err(|e| {
            error!(?e, status = ?status, error_source = ?e.source(), bytes_response = ?rpc_response, "upstream response json error");
            UpstreamError::JsonError
        })?;

        match &rpc_response.result {
            ResponseResult::Success(_) => {}
            ResponseResult::Error(e)
                if e.code == ErrorCode::ExecutionError
                    || e.code == ErrorCode::TransactionRejected =>
            {
                debug!(
                  err_code = ?e.code,
                  err_message = ?e.message,
                  err_data = ?e.data,
                  "upstream returned error, but it's expected"
                );
            }
            ResponseResult::Error(err) => {
                // TODO: start a new counter for upstream errors, and label by status code and url
                error!(
                    err_code = ?err.code,
                    err_message = ?err.message,
                    err_data = ?err.data,
                    "upstream returned unexpected error"
                );
            }
        };
        return Ok(rpc_response);
    }

    // # TODO: standardize error handling
    #[instrument(skip(self))]
    pub async fn forward_with_retry(
        &self,
        raw_call: &Bytes,
        max_retries: u32,
        retry_delay: Duration,
        jitter: bool,
    ) -> Result<RpcResponse, UpstreamError> {
        let mut last_error = None;
        let mut current_retry = 0;

        while current_retry <= max_retries {
            match self.forward_once(raw_call).await {
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
        Err(last_error.unwrap().into())
    }
}
