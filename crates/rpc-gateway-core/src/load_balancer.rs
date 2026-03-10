use std::{fmt, sync::Arc};

use arc_swap::ArcSwap;
use futures::future::join_all;
use nonempty::NonEmpty;
use rpc_gateway_config::{LoadBalancingStrategy, UpstreamHealthChecksConfig};
use rpc_gateway_upstream::upstream::Upstream;
use tokio::time::sleep;
use tracing::debug;

/// Tracks upstream health and exposes the healthy set.
#[derive(Debug)]
pub struct HealthCheckManager {
    all_upstreams: NonEmpty<Arc<Upstream>>,
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
            all_upstreams,
            config,
        }
    }

    /// Runs readiness probes in parallel and updates healthy set.
    pub async fn run_health_checks_once(&self) {
        let futures = self.all_upstreams.iter().map(|upstream| {
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
        LoadBalancingStrategy::WeightedOrder => todo!(),
    }
}
