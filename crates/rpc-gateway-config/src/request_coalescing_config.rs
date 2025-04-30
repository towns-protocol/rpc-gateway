use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestCoalescingConfig {
    #[serde(default = "default_request_coalescing_enabled")]
    pub enabled: bool,

    #[serde(default)]
    pub method_filter: RequestCoalescingMethods,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestCoalescingMethods {
    Whitelist(HashSet<String>),
    Blacklist(HashSet<String>),
    All,
}

fn default_request_coalescing_enabled() -> bool {
    true
}

// Define intermediate struct only for deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "methods")]
enum RequestCoalescingMethodsDef {
    Whitelist(Vec<String>),
    Blacklist(Vec<String>),
    All,
}

impl<'de> Deserialize<'de> for RequestCoalescingMethods {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let def = RequestCoalescingMethodsDef::deserialize(deserializer)?;
        Ok(match def {
            RequestCoalescingMethodsDef::Whitelist(vec) => {
                RequestCoalescingMethods::Whitelist(vec.into_iter().collect())
            }
            RequestCoalescingMethodsDef::Blacklist(vec) => {
                RequestCoalescingMethods::Blacklist(vec.into_iter().collect())
            }
            RequestCoalescingMethodsDef::All => RequestCoalescingMethods::All,
        })
    }
}

impl Serialize for RequestCoalescingMethods {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            RequestCoalescingMethods::Whitelist(set) => {
                RequestCoalescingMethodsDef::Whitelist(set.iter().cloned().collect())
            }
            RequestCoalescingMethods::Blacklist(set) => {
                RequestCoalescingMethodsDef::Blacklist(set.iter().cloned().collect())
            }
            RequestCoalescingMethods::All => RequestCoalescingMethodsDef::All,
        }
        .serialize(serializer)
    }
}

impl Default for RequestCoalescingMethods {
    fn default() -> Self {
        RequestCoalescingMethods::All
    }
}

impl RequestCoalescingConfig {
    /// Checks if a given method should be coalesced based on the configuration
    pub fn should_coalesce(&self, method: &str) -> bool {
        if !self.enabled {
            return false;
        }

        match &self.method_filter {
            RequestCoalescingMethods::Whitelist(methods) => methods.contains(&method.to_string()),
            RequestCoalescingMethods::Blacklist(methods) => !methods.contains(&method.to_string()),
            RequestCoalescingMethods::All => true,
        }
    }
}

impl Default for RequestCoalescingConfig {
    fn default() -> Self {
        Self {
            enabled: default_request_coalescing_enabled(),
            method_filter: RequestCoalescingMethods::All,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_coalescing_methods_deserialize() {
        let config_str = r#"
enabled: true
method_filter:
  type: "whitelist"
  methods:
    - eth_blockNumber
"#;

        let config: RequestCoalescingConfig = serde_yaml::from_str(config_str).unwrap();
        assert_eq!(config.enabled, true);
        assert_eq!(
            config.method_filter,
            RequestCoalescingMethods::Whitelist(
                vec!["eth_blockNumber".to_string()].into_iter().collect()
            )
        );
    }

    #[test]
    fn test_request_coalescing_methods_deserialize_blacklist() {
        let config_str = r#"
enabled: true
method_filter:
  type: "blacklist"
  methods:
    - eth_sendRawTransaction
    - eth_sendTransaction
"#;

        let config: RequestCoalescingConfig = serde_yaml::from_str(config_str).unwrap();
        assert_eq!(config.enabled, true);
        assert_eq!(
            config.method_filter,
            RequestCoalescingMethods::Blacklist(
                vec![
                    "eth_sendRawTransaction".to_string(),
                    "eth_sendTransaction".to_string()
                ]
                .into_iter()
                .collect()
            )
        );
    }

    #[test]
    fn test_request_coalescing_methods_deserialize_all() {
        let config_str = r#"
enabled: true
method_filter:
  type: "all"
"#;

        let config: RequestCoalescingConfig = serde_yaml::from_str(config_str).unwrap();
        assert_eq!(config.enabled, true);
        assert_eq!(config.method_filter, RequestCoalescingMethods::All);
    }

    #[test]
    fn test_request_coalescing_methods_deserialize_disabled() {
        let config_str = r#"
enabled: false
"#;

        let config: RequestCoalescingConfig = serde_yaml::from_str(config_str).unwrap();
        assert_eq!(config.enabled, false);
        assert_eq!(config.method_filter, RequestCoalescingMethods::All);
    }

    #[test]
    fn test_should_coalesce_whitelist() {
        let config = RequestCoalescingConfig {
            enabled: true,
            method_filter: RequestCoalescingMethods::Whitelist(
                vec!["eth_blockNumber".to_string()].into_iter().collect(),
            ),
        };
        assert!(config.should_coalesce("eth_blockNumber"));
        assert!(!config.should_coalesce("eth_getBalance"));
    }

    #[test]
    fn test_should_coalesce_blacklist() {
        let config = RequestCoalescingConfig {
            enabled: true,
            method_filter: RequestCoalescingMethods::Blacklist(
                vec!["eth_sendRawTransaction".to_string()]
                    .into_iter()
                    .collect(),
            ),
        };
        assert!(!config.should_coalesce("eth_sendRawTransaction"));
        assert!(config.should_coalesce("eth_getBalance"));
    }

    #[test]
    fn test_should_coalesce_all() {
        let config = RequestCoalescingConfig {
            enabled: true,
            method_filter: RequestCoalescingMethods::All,
        };
        assert!(config.should_coalesce("eth_anyMethod"));
    }

    #[test]
    fn test_should_coalesce_disabled() {
        let config = RequestCoalescingConfig {
            enabled: false,
            method_filter: RequestCoalescingMethods::All,
        };
        assert!(!config.should_coalesce("eth_anyMethod"));

        let config = RequestCoalescingConfig {
            enabled: false,
            method_filter: RequestCoalescingMethods::Whitelist(
                vec!["eth_blockNumber".to_string()].into_iter().collect(),
            ),
        };
        assert!(!config.should_coalesce("eth_blockNumber"));
    }
}
