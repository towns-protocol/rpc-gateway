use crate::config::Config;
use alloy_json_rpc::{Request, Response, ResponsePayload};
use serde_json::Value;
use std::{collections::HashMap, time::Duration};
use tokio::time::sleep;
use tracing::{debug, error, info, instrument};

use crate::chain_handler::ChainHandler;

#[derive(Debug, Clone)]
pub struct Gateway {
    handlers: HashMap<u64, ChainHandler>,
}

impl Gateway {
    pub fn new(config: Config) -> Self {
        info!(config = ?config, "Creating new Gateway");
        let mut handlers = HashMap::new();

        for (chain_id, chain_config) in config.chains {
            let handler = ChainHandler::new(
                chain_config,
                config.error_handling.clone(),
                config.load_balancing.clone(),
                config.cache.clone(),
            );
            handlers.insert(chain_id, handler);
        }

        Self { handlers }
    }

    #[instrument(skip(self))]
    pub fn liveness_probe(&self) -> bool {
        // TODO: should this fail if even a single chain is not working?
        self.handlers
            .values()
            .all(|handler| handler.liveness_probe())
    }

    #[instrument(skip(self))]
    pub fn readiness_probe(&self) -> bool {
        self.liveness_probe()
    }

    #[instrument(skip(self))]
    pub fn start_health_check_loops(&self) {
        for handler in self.handlers.values() {
            handler.start_health_check_loop();
        }
    }

    #[instrument(skip(self, request), fields(chain_id = %chain_id))]
    pub async fn forward_request(
        &self,
        chain_id: u64,
        request: Request<Value>,
    ) -> Result<Response<Value>, Box<dyn std::error::Error>> {
        debug!(chain_id = %chain_id, "Forwarding request");

        let handler = self.handlers.get(&chain_id).ok_or_else(|| {
            error!(chain_id = %chain_id, "Chain not found in configuration");
            format!("Chain {} not found", chain_id)
        })?;

        let response = handler.handle_request(request).await?;

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
