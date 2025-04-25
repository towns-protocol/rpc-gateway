use crate::{
    cache::{self, RpcCache},
    config::{Config, ProjectConfig},
    load_balancer,
    request_pool::ChainRequestPool,
    upstream::Upstream,
};
use anvil_rpc::{
    error::RpcError,
    request::Request,
    response::{Response, RpcResponse},
};
use futures::{
    FutureExt,
    future::{self, join_all},
};
use nonempty::NonEmpty;
use std::{collections::HashMap, sync::Arc};
use tracing::{debug, info, warn};

use crate::chain_handler::ChainHandler;

#[derive(Debug)]
pub struct Gateway {
    handlers: HashMap<u64, ChainHandler>,
    pub config: Config, // TODO: make this private
}

impl Gateway {
    pub fn new(config: Config) -> Self {
        info!(config = ?config, "Creating new Gateway");
        let mut handlers = HashMap::new();

        // TODO: make sure this chains hashmap is not empty
        for (chain_id, chain_config) in &config.chains {
            let cache = cache::from_config(&config.cache, chain_config);
            let upstreams = NonEmpty::from_vec(
                chain_config
                    .upstreams
                    .iter()
                    .map(|config| Arc::new(Upstream::new(config.clone(), chain_config.chain)))
                    .collect::<Vec<_>>(),
            )
            .expect("Chain config must have at least one upstream");
            let load_balancer = load_balancer::from_config(
                config.load_balancing.clone(),
                config.upstream_health_checks.clone(),
                upstreams,
            );
            let request_pool = ChainRequestPool::new(config.error_handling.clone(), load_balancer);
            let handler = ChainHandler::new(
                chain_config,
                &config.request_coalescing,
                &config.canned_responses,
                request_pool,
                cache,
            );
            handlers.insert(chain_id.clone(), handler);
        }

        Self { handlers, config }
    }

    // TODO: this should be called by the task manager. it should be async.
    pub async fn start_upstream_health_check_loops(&self) {
        if !self.config.upstream_health_checks.enabled {
            warn!("Upstream health checks are disabled. Not starting health check loops.");
            return;
        }

        debug!("Starting upstream health check loops");

        let health_check_futures: Vec<_> = self
            .handlers
            .values()
            .map(|handler| {
                let manager = handler
                    .request_pool
                    .load_balancer
                    .get_health_check_manager();
                async move {
                    manager.start_upstream_health_check_loop().await;
                }
            })
            .collect();

        join_all(health_check_futures).await;
    }

    pub async fn run_upstream_health_checks_once(&self) {
        let futures = self.handlers.values().map(|handler| {
            let manager = handler
                .request_pool
                .load_balancer
                .get_health_check_manager();

            async move {
                manager.run_health_checks_once().await;
            }
        });

        join_all(futures).await;
    }

    pub async fn handle_request(
        &self,
        project_config: &ProjectConfig,
        key: Option<String>,
        chain_id: u64,
        req: Request,
    ) -> Option<Response> {
        let is_authorized = project_config.key == key;

        let chain_handler = match self.handlers.get(&chain_id) {
            Some(chain_handler) => chain_handler,
            None => {
                let error = Response::error(RpcError::internal_error_with("Chain not supported"));
                return Some(error);
            }
        };

        match (req, is_authorized) {
            (Request::Single(call), true) => chain_handler
                .handle_call(call, project_config)
                .await
                .map(Response::Single),
            (Request::Batch(calls), true) => {
                future::join_all(
                    calls
                        .into_iter()
                        .map(move |call| chain_handler.handle_call(call, project_config)),
                )
                .map(responses_as_batch)
                .await
            }
            (_, false) => {
                warn!("Unauthorized request");
                let error = Response::error(RpcError::internal_error_with("Unauthorized"));
                return Some(error);
            }
        }
    }
}

/// processes batch calls
fn responses_as_batch(outs: Vec<Option<RpcResponse>>) -> Option<Response> {
    let batch: Vec<_> = outs.into_iter().flatten().collect();
    (!batch.is_empty()).then_some(Response::Batch(batch))
}
