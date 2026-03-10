use serde::{Deserialize, Serialize};

/// Strategy for selecting upstreams when forwarding requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum LoadBalancingStrategy {
    /// Uses only the single upstream with the highest weight.
    PrimaryOnly,
    /// Distributes requests across upstreams in round-robin fashion. (Not yet implemented)
    RoundRobin,
    /// Distributes requests proportionally based on upstream weights.
    ///
    /// For example, with weights [10, 90], approximately 10% of traffic goes to the first
    /// upstream and 90% to the second. If the selected upstream fails, requests fail over
    /// to other healthy upstreams in the order specified by `fallback_order`.
    WeightedOrder {
        /// Order of upstream names to try when the initially selected upstream fails.
        /// If empty, falls back to other upstreams in descending weight order.
        #[serde(default)]
        fallback_order: Vec<String>,
    },
    /// Tries upstreams by weight (highest first), failing over on connection errors, non-2xx HTTP status, or invalid JSON.
    Failover,
}

impl Default for LoadBalancingStrategy {
    fn default() -> Self {
        LoadBalancingStrategy::PrimaryOnly
    }
}
