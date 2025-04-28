use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum LoadBalancingStrategy {
    PrimaryOnly,
    RoundRobin,
    WeightedOrder,
}

impl Default for LoadBalancingStrategy {
    fn default() -> Self {
        LoadBalancingStrategy::PrimaryOnly
    }
}
