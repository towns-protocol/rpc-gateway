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
        info!("Creating new Gateway with config: {:?}", config);
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
        debug!("Forwarding request for chain {}", chain_id);

        let pool = self.pools.get(&chain_id).ok_or_else(|| {
            error!("Chain {} not found in configuration", chain_id);
            format!("Chain {} not found", chain_id)
        })?;

        let response = pool.forward_request(request).await?;

        // Log the response based on whether it's an error or result
        match &response.payload {
            ResponsePayload::Success(result) => {
                info!(
                    "Received successful response for chain {}: {:?}",
                    chain_id, result
                );
            }
            ResponsePayload::Failure(error) => {
                error!(
                    "Received error response for chain {}: {:?}",
                    chain_id, error
                );
            }
        }

        Ok(response)
    }
}
