use crate::config::Config;
use alloy_json_rpc::{Request, Response, ResponsePayload};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, info, instrument, warn};

use crate::chain_handler::ChainHandler;

#[derive(Debug, Clone)]
pub struct Gateway {
    handlers: HashMap<u64, ChainHandler>,
    config: Config,
}

impl Gateway {
    pub fn new(config: Config) -> Self {
        info!(config = ?config, "Creating new Gateway");
        let mut handlers = HashMap::new();

        for (chain_id, chain_config) in &config.chains {
            let handler = ChainHandler::new(
                chain_config.clone(),
                config.error_handling.clone(),
                config.load_balancing.clone(),
                config.upstream_health_checks.clone(),
                config.cache.clone(),
            );
            handlers.insert(chain_id.clone(), handler);
        }

        Self { handlers, config }
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
        if !self.config.upstream_health_checks.enabled {
            warn!("Health checks are disabled");
            return;
        }

        debug!("Starting health check loops");
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
