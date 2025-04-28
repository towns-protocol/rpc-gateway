use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CannedResponseConfig {
    #[serde(default = "default_canned_responses_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub methods: CannedResponseMethods,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CannedResponseMethods {
    #[serde(default = "default_web3_client_version_enabled")]
    pub web3_client_version: bool,
    #[serde(default = "default_eth_chain_id_enabled")]
    pub eth_chain_id: bool,
}

fn default_canned_responses_enabled() -> bool {
    true
}

fn default_web3_client_version_enabled() -> bool {
    true
}

fn default_eth_chain_id_enabled() -> bool {
    true
}

impl Default for CannedResponseConfig {
    fn default() -> Self {
        Self {
            enabled: default_canned_responses_enabled(),
            methods: CannedResponseMethods::default(),
        }
    }
}

impl Default for CannedResponseMethods {
    fn default() -> Self {
        Self {
            web3_client_version: default_web3_client_version_enabled(),
            eth_chain_id: default_eth_chain_id_enabled(),
        }
    }
}
