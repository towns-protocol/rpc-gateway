use anvil_core::eth::EthRequest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReqRes {
    pub req: EthRequest,
    pub res: serde_json::Value,
}
