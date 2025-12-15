#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(clippy::needless_return)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::new_without_default)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::drain_collect)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::collapsible_if)]

//! Statsig Client for Rust
//!
//! A type-safe, async client for interacting with Statsig's feature gates and dynamic configs.
//!
//! # Example
//!
//! ```rust, no_run
//! use statsig_client::{StatsigClient, User};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = StatsigClient::new("your-api-key").await?;
//!     
//!     let user = User::builder()
//!         .user_id("user-123")
//!         .email("user@example.com")
//!         .build()?;
//!     
//!     let gate_result = client.check_gate("my-feature-gate", &user).await?;
//!     println!("Gate passes: {}", gate_result);
//!     
//!     Ok(())
//! }
//! ```

pub mod api;
pub mod batch;
pub mod cache_metrics;
pub mod config;
pub mod error;
pub mod events;
pub mod response;
mod transport;
pub mod user;

use std::collections::HashMap;
use std::hash::Hash;

use moka::future::Cache;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

pub use api::{ConfigEvaluationResult, GateEvaluationResult, StatsigMetadata};
pub use batch::{BatchProcessor, BatchRequest};
pub use cache_metrics::{CacheMetrics, CacheMetricsSummary};
pub use config::StatsigClientConfig;
pub use error::{Result, StatsigError};
pub use events::{
    ExposureEventMetadata, LogEventResponse, StatsigEvent, StatsigEventTime, StatsigEventValue,
};
pub use response::ApiResponseHandler;
pub use user::{EnvironmentTier, StatsigEnvironment, User, UserBuilder};

/// A high-performance, async client for Statsig feature flags and dynamic configs.
///
/// # Architecture
///
/// The client uses a multi-layered architecture:
/// - **API Layer**: Handles HTTP communication with Statsig servers
/// - **Cache Layer**: Provides intelligent caching with TTL support
/// - **Batch Layer**: Optimizes multiple requests into single API calls
///
/// # Performance Characteristics
///
/// - Cache hit latency: ~1ms
/// - API call latency: ~100ms (network dependent)
/// - Batch processing: Reduces API calls by up to 90%
#[derive(Debug)]
pub struct StatsigClient {
    config: StatsigClientConfig,
    transport: transport::StatsigTransport,
    cache: Cache<CacheKey, CachedEvaluation>,
    cache_metrics: CacheMetrics,
    batch_sender: mpsc::Sender<BatchRequest>,
    _shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    entity_type: EntityType,
    entity_name: String,
    user_hash: String,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum EntityType {
    Gate,
    Config,
}

#[derive(Debug, Clone)]
struct CachedEvaluation {
    result: EvaluationResult,
    timestamp: std::time::Instant,
}

#[derive(Debug, Clone)]
enum EvaluationResult {
    Gate(GateEvaluationResult),
    Config(ConfigEvaluationResult),
}

impl StatsigClient {
    /// Create a new Statsig client with the given API key
    ///
    /// # Arguments
    /// * `api_key` - Your Statsig server API key
    ///
    /// # Returns
    /// A configured StatsigClient ready for use
    ///
    /// # Errors
    /// Returns an error if the API key is invalid or HTTP client creation fails
    pub async fn new(api_key: impl Into<String>) -> Result<Self> {
        let config = StatsigClientConfig::new(api_key)?;
        Self::with_config(config).await
    }

    /// Create a new Statsig client with custom configuration
    ///
    /// # Arguments
    /// * `config` - Custom configuration for the client
    ///
    /// # Returns
    /// A configured StatsigClient with custom settings
    ///
    /// # Errors
    /// Returns an error if configuration validation fails
    pub async fn with_config(config: StatsigClientConfig) -> Result<Self> {
        config.validate()?;

        let transport = transport::StatsigTransport::new(&config)?;

        let cache = Cache::builder()
            .time_to_live(config.cache_ttl)
            .max_capacity(config.cache_max_capacity)
            .build();

        let (batch_sender, batch_receiver) = mpsc::channel(1000);
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        let batch_processor = BatchProcessor::new(batch_receiver, shutdown_tx.subscribe());
        let _handle = tokio::spawn(batch_processor.run(transport.clone(), config.clone()));

        Ok(Self {
            config,
            transport,
            cache,
            cache_metrics: CacheMetrics::new(),
            batch_sender,
            _shutdown_tx: shutdown_tx,
        })
    }

    pub async fn log_event(&self, event_name: impl Into<String>, user: &User) -> Result<bool> {
        let event = StatsigEvent::builder()
            .event_name(event_name.into())
            .time(StatsigEventTime::UnixMillis(now_ms()))
            .build();

        Ok(self.log_events(vec![event], user).await?.success)
    }

    pub async fn log_events(
        &self,
        events: Vec<StatsigEvent>,
        user: &User,
    ) -> Result<LogEventResponse> {
        if events.is_empty() {
            return Err(StatsigError::validation(
                "events must contain at least 1 item",
            ));
        }

        user.validate_user()
            .map_err(|e| e.with_context("User validation failed"))?;

        self.transport.log_events(user, &events).await
    }

    /// Check if a single feature gate passes for a user
    ///
    /// This method first checks the cache for a recent evaluation, falling back
    /// to the Statsig API if needed. Results are automatically cached for
    /// the configured TTL duration.
    ///
    /// # Arguments
    ///
    /// * `gate_name` - The name of the feature gate to check (2-100 characters)
    /// * `user` - The user to evaluate the gate for
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the gate passes, `Ok(false)` if it doesn't, or an error
    /// if the evaluation fails.
    ///
    /// # Errors
    ///
    /// - `StatsigError::Validation` if the gate name or user is invalid
    /// - `StatsigError::Network` if the API request fails
    /// - `StatsigError::Api` if the server returns an error response
    ///
    /// # Performance
    ///
    /// - Cache hit: ~1ms
    /// - Cache miss: ~100ms (network dependent)
    pub async fn check_gate(&self, gate_name: impl Into<String>, user: &User) -> Result<bool> {
        let gate_name = gate_name.into();
        let results = self.check_gates(vec![gate_name], user).await?;
        Ok(results.into_values().next().unwrap_or(false))
    }

    /// Check multiple feature gates for a user
    ///
    /// This method efficiently checks multiple gates in a single API call when
    /// cache misses occur, significantly reducing network overhead.
    ///
    /// # Arguments
    ///
    /// * `gate_names` - List of gate names to check
    /// * `user` - The user to evaluate the gates for
    ///
    /// # Returns
    /// A HashMap mapping gate names to their boolean results
    ///
    /// # Errors
    /// Same as `check_gate`
    pub async fn check_gates(
        &self,
        gate_names: Vec<String>,
        user: &User,
    ) -> Result<HashMap<String, bool>> {
        if gate_names.is_empty() {
            return Ok(HashMap::new());
        }

        for gate_name in &gate_names {
            validate_entity_name("gate", gate_name)?;
        }

        user.validate_user()
            .map_err(|e| e.with_context("User validation failed"))?;

        let mut results = HashMap::new();
        let mut missing_gates = Vec::new();

        // Check cache first
        for gate_name in &gate_names {
            let cache_key = self.create_cache_key(EntityType::Gate, gate_name, user);
            if let Some(cached) = self.cache.get(&cache_key).await {
                self.cache_metrics.record_hit();
                if let EvaluationResult::Gate(gate_result) = cached.result {
                    results.insert(gate_name.clone(), gate_result.value);
                }
            } else {
                self.cache_metrics.record_miss();
                missing_gates.push(gate_name.clone());
            }
        }

        if missing_gates.is_empty() {
            return Ok(results);
        }

        // Fetch missing gates from API
        let gate_results = self.fetch_gates_batch(missing_gates, user).await?;

        for gate_result in gate_results {
            let cache_key = self.create_cache_key(EntityType::Gate, &gate_result.name, user);
            let cached = CachedEvaluation {
                result: EvaluationResult::Gate(gate_result.clone()),
                timestamp: std::time::Instant::now(),
            };
            self.cache_metrics.record_insert();
            self.cache.insert(cache_key, cached).await;
            results.insert(gate_result.name, gate_result.value);
        }

        Ok(results)
    }

    /// Get a single dynamic config for a user
    ///
    /// Retrieves a dynamic config (or experiment) value for the given user, with caching
    /// for improved performance. Statsig uses the same endpoint for both dynamic configs
    /// and experiments; the backend determines which based on the name.
    ///
    /// # Arguments
    ///
    /// * `config_name` - The name of the config to retrieve
    /// * `user` - The user to get the config for
    ///
    /// # Returns
    /// The config value as a JSON Value, or null if not found
    ///
    /// # Errors
    /// Similar to `check_gate`, with validation and network errors
    pub async fn get_config(&self, config_name: impl Into<String>, user: &User) -> Result<Value> {
        let config_name = config_name.into();
        let results = self.get_configs(vec![config_name], user).await?;
        Ok(results.into_values().next().unwrap_or(Value::Null))
    }

    /// Get a single dynamic config (or experiment) evaluation for a user
    ///
    /// Returns the full evaluation payload including `rule_id`, `group_name`, and `group`.
    pub async fn get_config_evaluation(
        &self,
        config_name: impl Into<String>,
        user: &User,
    ) -> Result<ConfigEvaluationResult> {
        let config_name = config_name.into();
        let mut results = self
            .get_config_evaluations(vec![config_name.clone()], user)
            .await?;
        results
            .remove(&config_name)
            .ok_or_else(|| StatsigError::internal("Missing config evaluation in response"))
    }

    /// Get multiple dynamic configs for a user
    ///
    /// Efficiently retrieves multiple configuration objects (or experiments) in parallel when
    /// cache misses occur.
    ///
    /// # Arguments
    ///
    /// * `config_names` - List of config names to retrieve
    /// * `user` - The user to get configs for
    ///
    /// # Returns
    /// A HashMap mapping config names to their JSON values
    ///
    /// # Errors
    /// Similar to `check_gate`
    pub async fn get_configs(
        &self,
        config_names: Vec<String>,
        user: &User,
    ) -> Result<HashMap<String, Value>> {
        let evaluations = self.get_config_evaluations(config_names, user).await?;
        Ok(evaluations.into_iter().map(|(k, v)| (k, v.value)).collect())
    }

    /// Get multiple dynamic config (or experiment) evaluations for a user
    ///
    /// Returns full evaluation payloads including `rule_id`, `group_name`, and `group`.
    pub async fn get_config_evaluations(
        &self,
        config_names: Vec<String>,
        user: &User,
    ) -> Result<HashMap<String, ConfigEvaluationResult>> {
        if config_names.is_empty() {
            return Ok(HashMap::new());
        }

        for config_name in &config_names {
            validate_entity_name("config", config_name)?;
        }

        user.validate_user()
            .map_err(|e| e.with_context("User validation failed"))?;

        let mut results = HashMap::new();
        let mut missing_configs = Vec::new();

        // Check cache first
        for config_name in &config_names {
            let cache_key = self.create_cache_key(EntityType::Config, config_name, user);
            if let Some(cached) = self.cache.get(&cache_key).await {
                self.cache_metrics.record_hit();
                if let EvaluationResult::Config(config_result) = cached.result {
                    results.insert(config_name.clone(), config_result);
                }
            } else {
                self.cache_metrics.record_miss();
                missing_configs.push(config_name.clone());
            }
        }

        if missing_configs.is_empty() {
            return Ok(results);
        }

        // Fetch missing configs from API
        let config_results = self.fetch_configs_batch(missing_configs, user).await?;

        for config_result in config_results {
            let cache_key = self.create_cache_key(EntityType::Config, &config_result.name, user);
            let cached = CachedEvaluation {
                result: EvaluationResult::Config(config_result.clone()),
                timestamp: std::time::Instant::now(),
            };
            self.cache_metrics.record_insert();
            self.cache.insert(cache_key, cached).await;
            results.insert(config_result.name.clone(), config_result);
        }

        Ok(results)
    }

    fn create_cache_key(
        &self,
        entity_type: EntityType,
        entity_name: &str,
        user: &User,
    ) -> CacheKey {
        let user_hash = user.hash_for_cache();
        CacheKey {
            entity_type,
            entity_name: entity_name.to_string(),
            user_hash,
        }
    }

    async fn fetch_gates_batch(
        &self,
        gate_names: Vec<String>,
        user: &User,
    ) -> Result<Vec<GateEvaluationResult>> {
        let (response_tx, response_rx) = oneshot::channel();

        let request = BatchRequest::CheckGates {
            gate_names,
            user: user.clone(),
            response_tx,
        };

        self.batch_sender
            .send(request)
            .await
            .map_err(|_| StatsigError::batch_processor("Batch processor channel closed"))?;

        response_rx
            .await
            .map_err(|_| StatsigError::batch_processor("Batch processor response channel closed"))?
    }

    async fn fetch_configs_batch(
        &self,
        config_names: Vec<String>,
        user: &User,
    ) -> Result<Vec<ConfigEvaluationResult>> {
        let (response_tx, response_rx) = oneshot::channel();

        let request = BatchRequest::GetConfigs {
            config_names,
            user: user.clone(),
            response_tx,
        };

        self.batch_sender
            .send(request)
            .await
            .map_err(|_| StatsigError::batch_processor("Batch processor channel closed"))?;

        response_rx
            .await
            .map_err(|_| StatsigError::batch_processor("Batch processor response channel closed"))?
    }

    /// Get cache performance metrics
    ///
    /// Returns a snapshot of cache performance metrics including hit ratio,
    /// total requests, and other useful statistics for monitoring.
    ///
    /// # Returns
    /// A summary of cache metrics
    pub fn cache_metrics(&self) -> CacheMetricsSummary {
        self.cache_metrics.summary()
    }

    /// Reset cache metrics
    ///
    /// Resets all cache performance counters to zero. Useful for
    /// periodic monitoring or testing scenarios.
    pub fn reset_cache_metrics(&self) {
        self.cache_metrics.reset();
    }
}

fn validate_entity_name(kind: &str, name: &str) -> Result<()> {
    let len = name.chars().count();
    if !(2..=100).contains(&len) {
        return Err(StatsigError::validation(format!(
            "{} name must be between 2 and 100 characters",
            kind
        )));
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let _client = StatsigClient::new("test_key").await.unwrap();
        // Basic test
    }

    #[tokio::test]
    async fn test_user_builder() {
        let user = User::builder()
            .user_id("test_user")
            .email("test@example.com")
            .country("US")
            .build()
            .unwrap();

        assert_eq!(user.user_id, Some("test_user".to_string()));
        assert_eq!(user.email, Some("test@example.com".to_string()));
        assert_eq!(user.country, Some("US".to_string()));
    }

    #[tokio::test]
    #[ignore = "Network integration test (requires Statsig API access)"]
    async fn test_demo_gate() {
        let _client = StatsigClient::new("client-PxavfBEvcE6M449BEtJyQe883t2StBbxwFCMpAuBnI")
            .await
            .unwrap();
        let user = User::builder().user_id("test_user").build().unwrap();
        let result = _client.check_gate("demo-gate", &user).await;
        println!("Demo gate result: {:?}", result);
        assert!(result.is_ok());
    }
}
