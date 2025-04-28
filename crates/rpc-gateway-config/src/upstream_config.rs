use std::time::Duration;

use duration_str::deserialize_duration;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    #[serde(with = "url_serde")]
    pub url: Url,
    #[serde(default = "default_timeout", deserialize_with = "validate_timeout")]
    pub timeout: Duration,
    #[serde(default = "default_weight")]
    #[serde(deserialize_with = "validate_weight")]
    pub weight: u32,
}

fn default_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_weight() -> u32 {
    1
}

fn validate_timeout<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let duration = deserialize_duration(deserializer)?;
    if duration.is_zero() {
        return Err(serde::de::Error::custom("timeout cannot be zero"));
    }
    Ok(duration)
}

fn validate_weight<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    if value == 0 {
        return Err(serde::de::Error::custom("weight cannot be zero"));
    }
    Ok(value)
}

pub trait UrlProcessor {
    fn process_url(&self, url_str: &str) -> Result<String, String>;
}

#[derive(Debug, Clone)]
pub struct EnvVarUrlProcessor;

impl UrlProcessor for EnvVarUrlProcessor {
    fn process_url(&self, url_str: &str) -> Result<String, String> {
        if url_str.starts_with('$') {
            let var_name = url_str.trim_start_matches('$');
            std::env::var(var_name)
                .map_err(|e| format!("Environment variable '{}' not found: {}", var_name, e))
        } else {
            Ok(url_str.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct DefaultUrlProcessor;

impl UrlProcessor for DefaultUrlProcessor {
    fn process_url(&self, url_str: &str) -> Result<String, String> {
        EnvVarUrlProcessor.process_url(url_str)
    }
}

mod url_serde {
    use super::*;
    use serde::{Deserializer, Serializer};
    use std::str::FromStr;

    pub fn serialize<S>(url: &Url, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(url.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Url, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_with_processor(deserializer, &DefaultUrlProcessor)
    }

    pub fn deserialize_with_processor<'de, D, P>(
        deserializer: D,
        processor: &P,
    ) -> Result<Url, D::Error>
    where
        D: Deserializer<'de>,
        P: UrlProcessor,
    {
        let s = String::deserialize(deserializer)?;
        let processed_url = processor
            .process_url(&s)
            .map_err(serde::de::Error::custom)?;
        let mut url = Url::from_str(&processed_url).map_err(serde::de::Error::custom)?;
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        Ok(url)
    }
}
