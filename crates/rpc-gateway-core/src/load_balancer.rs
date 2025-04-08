use std::{fmt, sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use futures::future::join_all;
use nonempty::NonEmpty;
use tokio::{task, time::sleep};

use crate::{config::LoadBalancingStrategy, upstream::Upstream};

/// Tracks upstream health and exposes the healthy set.
#[derive(Debug)]
struct HealthCheckManager {
    all_upstreams: NonEmpty<Arc<Upstream>>,
    healthy_upstreams: ArcSwap<Vec<Arc<Upstream>>>,
}

impl HealthCheckManager {
    pub fn new(all_upstreams: NonEmpty<Arc<Upstream>>) -> Self {
        Self {
            all_upstreams,
            healthy_upstreams: ArcSwap::from_pointee(vec![]),
        }
    }

    /// Runs readiness probes in parallel and updates healthy set.
    pub async fn run_health_checks(&self) {
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

    /// Returns a snapshot of currently healthy upstreams.
    pub fn healthy_upstreams(&self) -> Arc<Vec<Arc<Upstream>>> {
        self.healthy_upstreams.load_full()
    }
}

/// A basic load balancer interface.
pub trait LoadBalancer: fmt::Debug + Send + Sync {
    fn select_upstream(&self) -> Option<Arc<Upstream>>;
    fn start_health_check_loop(&self);
    fn liveness_probe(&self) -> bool;
}

/// Balancer that always selects a single primary upstream.
#[derive(Debug, Clone)]
pub struct PrimaryOnlyLoadBalancer {
    health_check_manager: Arc<HealthCheckManager>,
}

impl PrimaryOnlyLoadBalancer {
    pub fn new(all_upstreams: NonEmpty<Arc<Upstream>>) -> Self {
        let primary = all_upstreams
            .iter()
            .max_by_key(|u| u.config.weight)
            .cloned()
            .expect("NonEmpty should have at least one upstream");

        let manager = Arc::new(HealthCheckManager::new(NonEmpty::new(primary)));

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

    // TODO: use a task manager here for graceful shutdown
    fn start_health_check_loop(&self) {
        let manager = Arc::clone(&self.health_check_manager);

        task::spawn(async move {
            loop {
                manager.run_health_checks().await;
                sleep(Duration::from_secs(60)).await; // TODO: make this configurable
            }
        });
    }

    fn liveness_probe(&self) -> bool {
        self.select_upstream().is_some()
    }
}

pub fn create_load_balancer(
    load_balancing_strategy: LoadBalancingStrategy,
    all_upstreams: NonEmpty<Arc<Upstream>>,
) -> Arc<dyn LoadBalancer> {
    match load_balancing_strategy {
        LoadBalancingStrategy::PrimaryOnly => Arc::new(PrimaryOnlyLoadBalancer::new(all_upstreams)),
        LoadBalancingStrategy::RoundRobin => todo!(),
        LoadBalancingStrategy::WeightedOrder => todo!(),
    }
}
