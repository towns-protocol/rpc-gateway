use std::{fmt, sync::Arc};

use arc_swap::ArcSwap;
use futures::future::join_all;
use nonempty::NonEmpty;
use rand::Rng;
use rpc_gateway_config::{LoadBalancingStrategy, UpstreamHealthChecksConfig};
use rpc_gateway_upstream::upstream::Upstream;
use tokio::time::sleep;
use tracing::debug;

/// Tracks upstream health and exposes the healthy set.
///
/// Supports dynamic upstream updates via [`HealthCheckManager::update_upstreams`]
/// for configuration reloading.
#[derive(Debug)]
pub struct HealthCheckManager {
    all_upstreams: ArcSwap<NonEmpty<Arc<Upstream>>>,
    config: UpstreamHealthChecksConfig,
    healthy_upstreams: ArcSwap<Vec<Arc<Upstream>>>,
}

impl HealthCheckManager {
    /// Creates a new health check manager for the given upstreams.
    ///
    /// Initially assumes all upstreams are healthy to allow requests to succeed
    /// before the first health check completes (or when health checks are disabled).
    pub fn new(all_upstreams: NonEmpty<Arc<Upstream>>, config: UpstreamHealthChecksConfig) -> Self {
        let initial_healthy: Vec<_> = all_upstreams.iter().cloned().collect();
        Self {
            healthy_upstreams: ArcSwap::from_pointee(initial_healthy),
            all_upstreams: ArcSwap::from_pointee(all_upstreams),
            config,
        }
    }

    /// Updates the upstream list with new configuration.
    ///
    /// This is called during config reload to update upstream URLs, weights, etc.
    /// The healthy upstreams set is immediately updated to include all new upstreams,
    /// assuming they are healthy until the next health check runs.
    pub fn update_upstreams(&self, new_upstreams: NonEmpty<Arc<Upstream>>) {
        debug!(
            upstream_count = new_upstreams.len(),
            "Updating upstreams for health check manager"
        );

        // Store all_upstreams first (the superset), then healthy_upstreams (the subset)
        // This ensures conceptual consistency during the update
        let initial_healthy: Vec<_> = new_upstreams.iter().cloned().collect();
        self.all_upstreams.store(Arc::new(new_upstreams));
        self.healthy_upstreams.store(Arc::new(initial_healthy));
    }

    /// Runs readiness probes in parallel and updates healthy set.
    pub async fn run_health_checks_once(&self) {
        let all_upstreams = self.all_upstreams.load();
        let futures = all_upstreams.iter().map(|upstream| {
            let upstream = Arc::clone(upstream);
            async move {
                let is_healthy = upstream.readiness_probe().await;
                (upstream, is_healthy)
            }
        });

        let healthy = join_all(futures)
            .await
            .into_iter()
            .filter_map(|(upstream, is_healthy)| is_healthy.then_some(upstream))
            .collect();

        self.healthy_upstreams.store(Arc::new(healthy));
    }

    /// Starts the background health check loop that periodically probes all upstreams.
    pub async fn start_upstream_health_check_loop(&self) {
        let sleep_duration = self.config.interval;

        // Run first health check immediately to populate healthy upstreams
        self.run_health_checks_once().await;

        // TODO: consider adding the chain here to help with debugging
        loop {
            sleep(sleep_duration).await;
            self.run_health_checks_once().await;
            debug!(
                "Health checks loop sleeping for {} seconds",
                sleep_duration.as_secs()
            );
        }
    }

    /// Returns a snapshot of currently healthy upstreams.
    pub fn healthy_upstreams(&self) -> Arc<Vec<Arc<Upstream>>> {
        self.healthy_upstreams.load_full()
    }
}

/// A basic load balancer interface.
pub trait LoadBalancer: fmt::Debug + Send + Sync {
    /// Returns a single upstream (the first/primary one).
    fn select_upstream(&self) -> Option<Arc<Upstream>>;
    /// Returns all healthy upstreams in order for failover scenarios.
    fn select_upstreams(&self) -> Vec<Arc<Upstream>>;
    /// Returns the health check manager for this load balancer.
    fn get_health_check_manager(&self) -> Arc<HealthCheckManager>;
}

/// Balancer that always selects a single primary upstream.
#[derive(Debug, Clone)]
pub struct PrimaryOnlyLoadBalancer {
    health_check_manager: Arc<HealthCheckManager>,
}

impl PrimaryOnlyLoadBalancer {
    /// Creates a new primary-only load balancer that uses only the highest-weight upstream.
    pub fn new(
        all_upstreams: NonEmpty<Arc<Upstream>>,
        health_checks_config: UpstreamHealthChecksConfig,
    ) -> Self {
        let primary = all_upstreams
            .iter()
            .max_by_key(|u| u.config.weight)
            .cloned()
            .expect("NonEmpty should have at least one upstream");

        let manager = Arc::new(HealthCheckManager::new(
            NonEmpty::new(primary),
            health_checks_config,
        ));

        Self {
            health_check_manager: manager,
        }
    }
}

impl LoadBalancer for PrimaryOnlyLoadBalancer {
    fn select_upstream(&self) -> Option<Arc<Upstream>> {
        self.select_upstreams().into_iter().next()
    }

    fn select_upstreams(&self) -> Vec<Arc<Upstream>> {
        // No sorting needed: PrimaryOnlyLoadBalancer is initialized with only
        // a single upstream (the highest-weight one), so there's at most one element.
        self.health_check_manager.healthy_upstreams().to_vec()
    }

    fn get_health_check_manager(&self) -> Arc<HealthCheckManager> {
        Arc::clone(&self.health_check_manager)
    }
}

/// Balancer that tries upstreams by weight (highest first), failing over to the next on error.
#[derive(Debug, Clone)]
pub struct FailoverLoadBalancer {
    health_check_manager: Arc<HealthCheckManager>,
}

impl FailoverLoadBalancer {
    /// Creates a new failover load balancer that sorts upstreams by weight (highest first).
    pub fn new(
        all_upstreams: NonEmpty<Arc<Upstream>>,
        health_checks_config: UpstreamHealthChecksConfig,
    ) -> Self {
        // Sort upstreams by weight (highest first) for failover priority
        let mut sorted: Vec<_> = all_upstreams.into_iter().collect();
        sorted.sort_by(|a, b| b.config.weight.cmp(&a.config.weight));
        let sorted_upstreams =
            NonEmpty::from_vec(sorted).expect("NonEmpty should have at least one upstream");

        let manager = Arc::new(HealthCheckManager::new(
            sorted_upstreams,
            health_checks_config,
        ));
        Self {
            health_check_manager: manager,
        }
    }
}

impl LoadBalancer for FailoverLoadBalancer {
    fn select_upstream(&self) -> Option<Arc<Upstream>> {
        self.select_upstreams().into_iter().next()
    }

    fn select_upstreams(&self) -> Vec<Arc<Upstream>> {
        // Return healthy upstreams sorted by weight (highest first)
        // Re-sort to ensure deterministic ordering regardless of health check completion order
        let mut upstreams = self.health_check_manager.healthy_upstreams().to_vec();
        upstreams.sort_by(|a, b| b.config.weight.cmp(&a.config.weight));
        upstreams
    }

    fn get_health_check_manager(&self) -> Arc<HealthCheckManager> {
        Arc::clone(&self.health_check_manager)
    }
}

/// Balancer that distributes traffic proportionally based on upstream weights.
///
/// Uses weighted random selection to route traffic according to configured weights.
/// When the selected upstream fails, falls back to other upstreams in the specified
/// fallback order, or by descending weight if no fallback order is configured.
#[derive(Debug, Clone)]
pub struct WeightedOrderLoadBalancer {
    health_check_manager: Arc<HealthCheckManager>,
    /// Ordered list of upstream names for failover (empty means use weight order)
    fallback_order: Vec<String>,
}

impl WeightedOrderLoadBalancer {
    /// Creates a new weighted order load balancer.
    ///
    /// Traffic is distributed proportionally based on upstream weights.
    /// When the selected upstream fails, falls back to upstreams in `fallback_order`,
    /// or by descending weight if `fallback_order` is empty.
    pub fn new(
        all_upstreams: NonEmpty<Arc<Upstream>>,
        health_checks_config: UpstreamHealthChecksConfig,
        fallback_order: Vec<String>,
    ) -> Self {
        let manager = Arc::new(HealthCheckManager::new(all_upstreams, health_checks_config));
        Self {
            health_check_manager: manager,
            fallback_order,
        }
    }

    /// Selects an upstream using weighted random selection.
    fn weighted_random_select(&self, upstreams: &[Arc<Upstream>]) -> Option<Arc<Upstream>> {
        if upstreams.is_empty() {
            return None;
        }

        let total_weight: u32 = upstreams.iter().map(|u| u.config.weight).sum();
        if total_weight == 0 {
            return upstreams.first().cloned();
        }

        let mut rng = rand::rng();
        let random_value = rng.random_range(0..total_weight);

        let mut cumulative_weight = 0u32;
        for upstream in upstreams {
            cumulative_weight += upstream.config.weight;
            if random_value < cumulative_weight {
                return Some(Arc::clone(upstream));
            }
        }

        // Fallback to first upstream (shouldn't happen with valid weights)
        upstreams.first().cloned()
    }

    /// Orders upstreams for failover based on fallback_order config.
    /// Places the selected upstream first, then orders remaining upstreams
    /// according to fallback_order (or by descending weight if not configured).
    fn order_for_failover(
        &self,
        selected: &Arc<Upstream>,
        all_healthy: &[Arc<Upstream>],
    ) -> Vec<Arc<Upstream>> {
        let mut result = vec![Arc::clone(selected)];

        // Get remaining upstreams (excluding the selected one)
        let remaining: Vec<_> = all_healthy
            .iter()
            .filter(|u| u.name() != selected.name())
            .cloned()
            .collect();

        if self.fallback_order.is_empty() {
            // No explicit fallback order: use descending weight
            let mut sorted = remaining;
            sorted.sort_by(|a, b| b.config.weight.cmp(&a.config.weight));
            result.extend(sorted);
        } else {
            // Use configured fallback order
            for name in &self.fallback_order {
                if let Some(upstream) = remaining.iter().find(|u| u.name() == name) {
                    result.push(Arc::clone(upstream));
                }
            }
            // Add any remaining upstreams not in fallback_order (sorted by weight)
            let mut unordered: Vec<_> = remaining
                .iter()
                .filter(|u| !self.fallback_order.contains(&u.name().to_string()))
                .cloned()
                .collect();
            unordered.sort_by(|a, b| b.config.weight.cmp(&a.config.weight));
            result.extend(unordered);
        }

        result
    }
}

impl LoadBalancer for WeightedOrderLoadBalancer {
    fn select_upstream(&self) -> Option<Arc<Upstream>> {
        let healthy = self.health_check_manager.healthy_upstreams();
        self.weighted_random_select(&healthy)
    }

    fn select_upstreams(&self) -> Vec<Arc<Upstream>> {
        let healthy = self.health_check_manager.healthy_upstreams();
        if healthy.is_empty() {
            return vec![];
        }

        // Select primary upstream via weighted random, then order remaining for failover
        if let Some(selected) = self.weighted_random_select(&healthy) {
            self.order_for_failover(&selected, &healthy)
        } else {
            vec![]
        }
    }

    fn get_health_check_manager(&self) -> Arc<HealthCheckManager> {
        Arc::clone(&self.health_check_manager)
    }
}

/// Creates a load balancer based on the configured strategy.
///
/// Returns the appropriate load balancer implementation for the given strategy.
pub fn from_config(
    load_balancing_strategy: LoadBalancingStrategy,
    upstream_health_checks_config: UpstreamHealthChecksConfig,
    all_upstreams: NonEmpty<Arc<Upstream>>,
) -> Arc<dyn LoadBalancer> {
    match load_balancing_strategy {
        LoadBalancingStrategy::PrimaryOnly => Arc::new(PrimaryOnlyLoadBalancer::new(
            all_upstreams,
            upstream_health_checks_config,
        )),
        LoadBalancingStrategy::Failover => Arc::new(FailoverLoadBalancer::new(
            all_upstreams,
            upstream_health_checks_config,
        )),
        LoadBalancingStrategy::RoundRobin => todo!(),
        LoadBalancingStrategy::WeightedOrder { fallback_order } => {
            Arc::new(WeightedOrderLoadBalancer::new(
                all_upstreams,
                upstream_health_checks_config,
                fallback_order,
            ))
        }
    }
}
