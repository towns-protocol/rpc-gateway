use alloy_chains::Chain;
use duration_str::deserialize_duration;
use nonempty::NonEmpty;
use serde::{Deserialize, Deserializer, Serialize};
use std::time::Duration;
use url::Url;

use crate::UpstreamConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    #[serde(skip)]
    pub chain: Chain,
    #[serde(
        deserialize_with = "deserialize_nonempty_upstreams",
        serialize_with = "serialize_nonempty_upstreams"
    )]
    pub upstreams: NonEmpty<UpstreamConfig>,

    #[serde(default, deserialize_with = "deserialize_option_duration")]
    pub block_time: Option<Duration>,
}

fn deserialize_option_duration<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    // This will deserialize the value into an Option<String>
    // If the value is null, we get None; otherwise, we parse it
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let duration = deserialize_duration(serde::de::IntoDeserializer::into_deserializer(s))?;
            Ok(Some(duration))
        }
        None => Ok(None),
    }
}

// TODO: audit the default values for everything
impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain: Chain::from_id(1),
            upstreams: NonEmpty::new(UpstreamConfig {
                url: Url::parse("http://example.com").unwrap(),
                timeout: Duration::from_secs(10),
                weight: 1,
            }),
            block_time: None,
        }
    }
}

fn deserialize_nonempty_upstreams<'de, D>(
    deserializer: D,
) -> Result<NonEmpty<UpstreamConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let vec: Vec<UpstreamConfig> = Vec::deserialize(deserializer)?;
    NonEmpty::from_vec(vec).ok_or_else(|| serde::de::Error::custom("upstreams cannot be empty"))
}

fn serialize_nonempty_upstreams<S>(
    upstreams: &NonEmpty<UpstreamConfig>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let vec: Vec<_> = upstreams.iter().cloned().collect();
    vec.serialize(serializer)
}
