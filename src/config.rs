use bon::Builder;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Builder)]
pub struct StatsigClientConfig {
    #[builder(into)]
    pub api_key: String,
    #[builder(default = "https://api.statsig.com".to_string())]
    pub base_url: String,
    #[builder(default = "https://events.statsigapi.net".to_string())]
    pub events_base_url: String,
    #[builder(default = Duration::from_secs(30))]
    pub timeout: Duration,
    #[builder(default = 3)]
    pub retry_attempts: u32,
    #[builder(default = Duration::from_millis(1000))]
    pub retry_delay: Duration,
    #[builder(default = Duration::from_secs(300))]
    pub cache_ttl: Duration,
    #[builder(default = 10000)]
    pub cache_max_capacity: u64,
    #[builder(default = 10)]
    pub batch_size: usize,
    #[builder(default = Duration::from_millis(100))]
    pub batch_flush_interval: Duration,
    #[builder(default = false)]
    pub offline_fallback: bool,
    #[builder(default = false)]
    pub exposure_logging_disabled: bool,
    #[builder(default = "rust-client".to_string())]
    pub sdk_type: String,
    #[builder(default = env!("CARGO_PKG_VERSION").to_string())]
    pub sdk_version: String,
}

impl StatsigClientConfig {
    pub fn new(api_key: impl Into<String>) -> crate::error::Result<Self> {
        let api_key = api_key.into();

        if api_key.is_empty() {
            return Err(crate::error::StatsigError::configuration(
                "API key cannot be empty",
            ));
        }

        Ok(Self::builder().api_key(api_key).build())
    }

    pub fn validate(&self) -> crate::error::Result<()> {
        if self.api_key.is_empty() {
            return Err(crate::error::StatsigError::configuration(
                "API key cannot be empty",
            ));
        }

        if self.base_url.is_empty() {
            return Err(crate::error::StatsigError::configuration(
                "Base URL cannot be empty",
            ));
        }

        if self.timeout.as_secs() == 0 {
            return Err(crate::error::StatsigError::configuration(
                "Timeout must be greater than 0",
            ));
        }

        if self.retry_attempts == 0 {
            return Err(crate::error::StatsigError::configuration(
                "Retry attempts must be greater than 0",
            ));
        }

        if self.batch_size == 0 {
            return Err(crate::error::StatsigError::configuration(
                "Batch size must be greater than 0",
            ));
        }

        if self.cache_ttl.as_secs() == 0 {
            return Err(crate::error::StatsigError::configuration(
                "Cache TTL must be greater than 0",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub sdk_type: String,
    pub sdk_version: String,
    pub language: String,
    pub language_version: String,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            sdk_type: "rust-client".to_string(),
            sdk_version: env!("CARGO_PKG_VERSION").to_string(),
            language: "rust".to_string(),
            language_version: "unknown".to_string(),
        }
    }
}
