use std::{fmt, sync::Arc};

use arc_swap::ArcSwap;
use futures::future::join_all;
use nonempty::NonEmpty;
use rpc_gateway_config::{LoadBalancingStrategy, UpstreamHealthChecksConfig};
use tokio::time::sleep;
use tracing::debug;

use crate::upstream::Upstream;

/// Tracks upstream health and exposes the healthy set.
#[derive(Debug)]
pub struct HealthCheckManager {
    all_upstreams: NonEmpty<Arc<Upstream>>,
    config: UpstreamHealthChecksConfig,
    healthy_upstreams: ArcSwap<Vec<Arc<Upstream>>>,
}

impl HealthCheckManager {
    pub fn new(all_upstreams: NonEmpty<Arc<Upstream>>, config: UpstreamHealthChecksConfig) -> Self {
        Self {
            healthy_upstreams: ArcSwap::from_pointee(vec![]),
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

    pub async fn start_upstream_health_check_loop(&self) {
        let sleep_duration = self.config.interval;

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
    fn select_upstream(&self) -> Option<Arc<Upstream>>;
    fn get_health_check_manager(&self) -> Arc<HealthCheckManager>;
}

/// Balancer that always selects a single primary upstream.
#[derive(Debug, Clone)]
pub struct PrimaryOnlyLoadBalancer {
    health_check_manager: Arc<HealthCheckManager>,
}

impl PrimaryOnlyLoadBalancer {
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
        let upstreams = self.health_check_manager.healthy_upstreams();
        upstreams.first().cloned()
    }

    fn get_health_check_manager(&self) -> Arc<HealthCheckManager> {
        Arc::clone(&self.health_check_manager)
    }
}

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
        LoadBalancingStrategy::RoundRobin => todo!(),
        LoadBalancingStrategy::WeightedOrder => todo!(),
    }
}
