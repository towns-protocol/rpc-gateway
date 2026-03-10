use crate::{lazy_request::PreservedRequest, load_balancer, request_pool::ChainRequestPool};
use arc_swap::ArcSwap;
use futures::{
    FutureExt,
    future::{self, join_all},
};
use metrics::{counter, gauge};
use nonempty::NonEmpty;
use rpc_gateway_config::{ChainConfig, Config, ProjectConfig};
use rpc_gateway_rpc::{
    error::RpcError,
    response::{Response, RpcResponse},
};
use rpc_gateway_upstream::upstream::Upstream;
use std::path::PathBuf;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::chain_handler::ChainHandler;

#[derive(Debug)]
pub struct GatewayRequest {
    pub project_config: ProjectConfig,
    pub key: Option<String>,
    pub chain_id: u64,
    pub req: PreservedRequest,
}

impl GatewayRequest {
    pub fn new(
        project_config: ProjectConfig,
        key: Option<String>,
        chain_id: u64,
        req: PreservedRequest,
    ) -> Self {
        Self {
            project_config,
            key,
            chain_id,
            req,
        }
    }
}

/// Errors that can occur during configuration reload.
#[derive(Debug, Error)]
pub enum ReloadError {
    #[error("No config path provided for reload")]
    NoConfigPath,
    #[error("Failed to load config: {0}")]
    ConfigError(String),
}

impl From<Box<dyn std::error::Error>> for ReloadError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        ReloadError::ConfigError(err.to_string())
    }
}

/// The main gateway that routes requests to chain handlers.
///
/// The gateway supports dynamic configuration reloading. When the configuration
/// file changes, call [`Gateway::reload_config`] to apply the new configuration
/// without restarting the service.
pub struct Gateway {
    handlers: ArcSwap<HashMap<u64, Arc<ChainHandler>>>,
    config: ArcSwap<Config>,
    config_path: Option<PathBuf>,
    /// Mutex to serialize config reloads, preventing interleaved stores that could
    /// leave handlers and config on different generations.
    reload_mutex: Mutex<()>,
}

impl Gateway {
    /// Creates a new gateway with the given configuration.
    ///
    /// If `config_path` is provided, the gateway supports dynamic config reloading
    /// via [`Gateway::reload_config`].
    pub async fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let handlers = Self::build_handlers(&config).await;

        // Emit initial upstream weight metrics
        emit_upstream_weight_metrics(&config);

        Self {
            handlers: ArcSwap::from_pointee(handlers),
            config: ArcSwap::from_pointee(config),
            config_path,
            reload_mutex: Mutex::new(()),
        }
    }

    /// Builds chain handlers from the configuration.
    async fn build_handlers(config: &Config) -> HashMap<u64, Arc<ChainHandler>> {
        let mut handlers = HashMap::new();

        for (chain_id, chain_config) in &config.chains {
            let handler = Self::build_chain_handler(chain_config, config).await;
            handlers.insert(*chain_id, Arc::new(handler));
        }

        handlers
    }

    /// Builds a single chain handler from chain and global config.
    async fn build_chain_handler(chain_config: &ChainConfig, config: &Config) -> ChainHandler {
        let cache = rpc_gateway_cache::cache::from_config(&config.cache, chain_config).await;
        let upstreams = NonEmpty::from_vec(
            chain_config
                .upstreams
                .iter()
                .map(|upstream_config| {
                    Arc::new(Upstream::new(upstream_config.clone(), chain_config.chain))
                })
                .collect::<Vec<_>>(),
        )
        .expect("Chain config must have at least one upstream");

        let load_balancer = load_balancer::from_config(
            config.load_balancing.clone(),
            config.upstream_health_checks.clone(),
            upstreams,
        );

        let request_pool = ChainRequestPool::new(config.error_handling.clone(), load_balancer);

        ChainHandler::new(
            chain_config,
            &config.request_coalescing,
            &config.canned_responses,
            request_pool,
            cache,
        )
    }

    /// Reloads the configuration from the stored config path.
    ///
    /// This method:
    /// 1. Parses the new configuration file
    /// 2. For existing chains: rebuilds handlers with new config
    /// 3. For new chains: creates new handlers
    /// 4. For removed chains: removes them from the handlers map
    ///
    /// In-flight requests continue using the old configuration until they complete,
    /// thanks to Arc reference counting.
    ///
    /// Concurrent calls to this method are serialized to prevent interleaved stores
    /// that could leave handlers and config on different generations.
    ///
    /// # Errors
    ///
    /// Returns an error if no config path was provided or if the config file
    /// cannot be parsed.
    pub async fn reload_config(&self) -> Result<(), ReloadError> {
        // Serialize reloads to prevent interleaved stores
        let _guard = self.reload_mutex.lock().await;

        let path = self.config_path.as_ref().ok_or(ReloadError::NoConfigPath)?;

        info!(config_path = %path.display(), "Reloading configuration");

        let new_config = Config::from_yaml_path_buf(path)?;

        self.apply_config(new_config).await;

        info!("Configuration reloaded successfully");
        counter!("config_reload_total", "status" => "success").increment(1);

        Ok(())
    }

    /// Applies a new configuration to the gateway.
    ///
    /// This is the core reload logic that updates handlers based on config changes.
    /// For changed chains, we build a completely new handler and swap the Arc atomically
    /// to ensure requests never see inconsistent state (e.g., new config with old pool).
    async fn apply_config(&self, new_config: Config) {
        let old_handlers = self.handlers.load();
        let old_config = self.config.load();
        let mut new_handlers = HashMap::new();

        // Compute once - doesn't depend on individual chains
        let global_changed = !global_configs_equal(&old_config, &new_config);

        for (chain_id, chain_config) in &new_config.chains {
            if let Some(_existing_handler) = old_handlers.get(chain_id) {
                // Check if this chain's config actually changed
                let old_chain_config = old_config.chains.get(chain_id);
                let chain_changed = old_chain_config
                    .map(|old| !configs_equal(old, chain_config))
                    .unwrap_or(true);

                if chain_changed || global_changed {
                    // Config changed - build a completely new handler atomically
                    // This ensures requests never see inconsistent state (e.g., new config with old pool)
                    debug!(chain_id = %chain_id, "Rebuilding chain handler for config change");
                    let handler = Self::build_chain_handler(chain_config, &new_config).await;
                    new_handlers.insert(*chain_id, Arc::new(handler));
                } else {
                    // No changes - reuse existing handler
                    new_handlers.insert(*chain_id, Arc::clone(_existing_handler));
                }
            } else {
                // New chain - create a new handler
                info!(chain_id = %chain_id, "Adding new chain handler");
                let handler = Self::build_chain_handler(chain_config, &new_config).await;
                new_handlers.insert(*chain_id, Arc::new(handler));
            }
        }

        // Log removed chains
        for chain_id in old_handlers.keys() {
            if !new_config.chains.contains_key(chain_id) {
                info!(chain_id = %chain_id, "Removing chain handler");
            }
        }

        self.handlers.store(Arc::new(new_handlers));
        self.config.store(Arc::new(new_config.clone()));

        // Update upstream weight metrics after config reload
        emit_upstream_weight_metrics(&new_config);
    }

    /// Returns a reference to the current configuration.
    pub fn config(&self) -> Arc<Config> {
        self.config.load_full()
    }

    /// Starts the health check loop that periodically checks all chain handlers.
    ///
    /// This method runs indefinitely until cancelled. Unlike per-handler loops,
    /// this dynamically observes the current set of handlers, so newly added
    /// chains get health-checked and removed chains stop being checked.
    ///
    /// The loop re-reads the config on each iteration, so changes to
    /// `upstream_health_checks.enabled` and `upstream_health_checks.interval`
    /// take effect without restart.
    pub async fn start_upstream_health_check_loops(&self) {
        debug!("Starting dynamic health check loop");

        loop {
            // Re-read config each iteration to pick up hot-reload changes
            let config = self.config.load();
            let interval = config.upstream_health_checks.interval;

            if !config.upstream_health_checks.enabled {
                // Health checks disabled - sleep and check again later
                debug!(
                    sleep_secs = interval.as_secs(),
                    "Health checks disabled, sleeping"
                );
                tokio::time::sleep(interval).await;
                continue;
            }

            self.run_upstream_health_checks_once().await;
            debug!(
                sleep_secs = interval.as_secs(),
                "Health check loop completed"
            );
            tokio::time::sleep(interval).await;
        }
    }

    /// Runs a single health check for all upstreams across all chains.
    pub async fn run_upstream_health_checks_once(&self) {
        let handlers = self.handlers.load();
        let futures = handlers.values().map(|handler| {
            let request_pool = handler.get_request_pool();
            let manager = request_pool.load_balancer.get_health_check_manager();

            async move {
                manager.run_health_checks_once().await;
            }
        });

        join_all(futures).await;
    }

    /// Handles an incoming gateway request.
    pub async fn handle_request(&self, gateway_request: GatewayRequest) -> Option<Response> {
        let is_authorized = gateway_request.project_config.key == gateway_request.key;

        let handlers = self.handlers.load();
        let chain_handler = match handlers.get(&gateway_request.chain_id) {
            Some(chain_handler) => Arc::clone(chain_handler),
            None => {
                let error = Response::error(RpcError::internal_error_with("Chain not supported"));
                return Some(error);
            }
        };
        // Drop handlers reference early to avoid holding it during async work
        drop(handlers);

        let project_config = &gateway_request.project_config;

        if !is_authorized {
            warn!("Unauthorized request");
            let error = Response::error(RpcError::internal_error_with("Unauthorized"));
            return Some(error);
        }

        match gateway_request.req {
            PreservedRequest::Single(call) => chain_handler
                .handle_call(call, project_config)
                .await
                .map(Response::Single),
            PreservedRequest::Batch(calls) => {
                let project_config = project_config.clone();
                let futures = calls.into_iter().map(|call| {
                    let handler = Arc::clone(&chain_handler);
                    let config = project_config.clone();
                    async move { handler.handle_call(call, &config).await }
                });
                future::join_all(futures).map(responses_as_batch).await
            }
        }
    }
}

/// Checks if two chain configs are equal (for reload comparison).
///
/// Compares all fields that affect handler behavior: block_time (cache TTL),
/// and upstream configuration (URLs, weights, timeouts, names).
fn configs_equal(a: &ChainConfig, b: &ChainConfig) -> bool {
    // Compare block_time (affects cache TTL calculations)
    if a.block_time != b.block_time {
        return false;
    }

    // Compare upstream configuration
    if a.upstreams.len() != b.upstreams.len() {
        return false;
    }

    for (ua, ub) in a.upstreams.iter().zip(b.upstreams.iter()) {
        if ua.url != ub.url
            || ua.weight != ub.weight
            || ua.timeout != ub.timeout
            || ua.name != ub.name
        {
            return false;
        }
    }

    true
}

/// Checks if global configs that affect chain handlers are equal.
///
/// Compares only the config sections that affect ChainHandler behavior:
/// - load_balancing: affects how requests are routed to upstreams
/// - error_handling: affects retry/failover behavior
/// - cache: affects response caching
/// - request_coalescing: affects request deduplication
/// - canned_responses: affects which responses are generated locally
/// - upstream_health_checks: affects health check behavior
///
/// Note: Changes to server, cors, metrics, logging, or projects do NOT
/// require rebuilding chain handlers.
fn global_configs_equal(a: &Config, b: &Config) -> bool {
    a.load_balancing == b.load_balancing
        && a.error_handling == b.error_handling
        && a.cache == b.cache
        && a.request_coalescing == b.request_coalescing
        && a.canned_responses == b.canned_responses
        && a.upstream_health_checks == b.upstream_health_checks
}

/// Processes batch call responses into a single batch response.
fn responses_as_batch(outs: Vec<Option<RpcResponse>>) -> Option<Response> {
    let batch: Vec<_> = outs.into_iter().flatten().collect();
    (!batch.is_empty()).then_some(Response::Batch(batch))
}

/// Emits gauge metrics for all configured upstream weights.
///
/// This allows observing the configured weight distribution (e.g., 10/90 split)
/// alongside the actual traffic distribution from request counters.
fn emit_upstream_weight_metrics(config: &Config) {
    for (chain_id, chain_config) in &config.chains {
        let chain_id_str = chain_id.to_string();
        for upstream in &chain_config.upstreams {
            gauge!(
                "upstream_configured_weight",
                "chain_id" => chain_id_str.clone(),
                "upstream" => upstream.name.clone(),
            )
            .set(upstream.weight as f64);
        }
    }
}
