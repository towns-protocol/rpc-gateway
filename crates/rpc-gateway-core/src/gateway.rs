use crate::{config::Config, load_balancer::HealthCheckManager};
use alloy_json_rpc::{Request, Response, ResponsePayload};
use futures::future::join_all;
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

        // TODO: make sure this chains hashmap is not empty
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
    pub fn start_upstream_health_check_loops(&self) {
        if !self.config.upstream_health_checks.enabled {
            warn!("Upstream health checks are disabled. Not starting health check loops.");
            return;
        }

        debug!("Starting upstream health check loops");
        for handler in self.handlers.values() {
            // TODO: use a task manager here for graceful shutdown
            debug!(
                "Starting upstream health check loop for chain: {}",
                handler.chain_config.chain
            );

            let health_check_manager = handler
                .request_pool
                .load_balancer
                .get_health_check_manager();

            HealthCheckManager::start_upstream_health_check_loop(health_check_manager);
        }
    }

    #[instrument(skip(self))]
    pub async fn run_upstream_health_checks(&self) {
        let futures = self.handlers.values().map(|handler| {
            let manager = handler
                .request_pool
                .load_balancer
                .get_health_check_manager();

            async move {
                manager.run_health_checks().await;
            }
        });

        join_all(futures).await;
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
