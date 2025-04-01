use alloy_json_rpc::{Request, Response, ResponsePayload};
use rpc_gateway_config::Config;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, info, instrument};

use crate::request_pool::ChainRequestPool;

#[derive(Debug)]
pub struct Gateway {
    pools: HashMap<u64, ChainRequestPool>,
}

impl Gateway {
    pub fn new(config: Config) -> Self {
        info!(config = ?config, "Creating new Gateway");
        let mut pools = HashMap::new();

        for (chain_id, chain_config) in config.chains {
            let pool = ChainRequestPool::new(
                chain_config,
                config.error_handling.clone(),
                config.load_balancing.clone(),
            );
            pools.insert(chain_id, pool);
        }

        Self { pools }
    }

    #[instrument(skip(self, request), fields(chain_id = %chain_id))]
    pub async fn forward_request(
        &self,
        chain_id: u64,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        debug!(chain_id = %chain_id, "Forwarding request");

        let pool = self.pools.get(&chain_id).ok_or_else(|| {
            error!(chain_id = %chain_id, "Chain not found in configuration");
            format!("Chain {} not found", chain_id)
        })?;

        let response = pool.forward_request(request).await?;

        // Log the response based on whether it's an error or result
        match &response.payload {
            ResponsePayload::Success(result) => {
                info!(
                    chain_id = %chain_id,
                    result = ?result,
                    "Received successful response"
                );
            }
            ResponsePayload::Failure(error) => {
                error!(
                    chain_id = %chain_id,
                    error = ?error,
                    "Received error response"
                );
            }
        }

        Ok(response)
    }
}
