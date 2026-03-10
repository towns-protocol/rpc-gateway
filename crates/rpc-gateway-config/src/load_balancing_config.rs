use serde::{Deserialize, Serialize};

/// Strategy for selecting upstreams when forwarding requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum LoadBalancingStrategy {
    /// Uses only the single upstream with the highest weight.
    PrimaryOnly,
    /// Distributes requests across upstreams in round-robin fashion. (Not yet implemented)
    RoundRobin,
    /// Distributes requests based on upstream weights. (Not yet implemented)
    WeightedOrder,
    /// Tries upstreams by weight (highest first), failing over on connection errors, non-2xx HTTP status, or invalid JSON.
    Failover,
}

impl Default for LoadBalancingStrategy {
    fn default() -> Self {
        LoadBalancingStrategy::PrimaryOnly
    }
}
