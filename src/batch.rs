//! Batch processing module for optimizing API requests
//!
//! This module handles batching multiple gate and config requests into single API calls
//! to reduce network overhead and improve performance.

use crate::{
    api::{ConfigEvaluationResult, GateEvaluationResult},
    config::StatsigClientConfig,
    error::Result,
    transport::StatsigTransport,
    user::User,
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

/// Represents different types of batch requests
#[derive(Debug)]
pub enum BatchRequest {
    CheckGates {
        gate_names: Vec<String>,
        user: User,
        response_tx: oneshot::Sender<Result<Vec<GateEvaluationResult>>>,
    },
    GetConfigs {
        config_names: Vec<String>,
        user: User,
        response_tx: oneshot::Sender<Result<Vec<ConfigEvaluationResult>>>,
    },
}

/// Handles batch processing of API requests
pub struct BatchProcessor {
    receiver: mpsc::Receiver<BatchRequest>,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
}

impl BatchProcessor {
    /// Creates a new batch processor
    pub fn new(
        receiver: mpsc::Receiver<BatchRequest>,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self {
            receiver,
            shutdown_rx,
        }
    }

    /// Runs the batch processor loop
    pub async fn run(mut self, transport: StatsigTransport, config: StatsigClientConfig) {
        let mut interval = tokio::time::interval(config.batch_flush_interval);
        let mut gate_requests = Vec::new();
        let mut config_requests = Vec::new();

        loop {
            tokio::select! {
                Some(request) = self.receiver.recv() => {
                    match request {
                        BatchRequest::CheckGates { .. } => gate_requests.push(request),
                        BatchRequest::GetConfigs { .. } => config_requests.push(request),
                    }

                    // Process if batch size reached
                    if gate_requests.len() >= config.batch_size || config_requests.len() >= config.batch_size {
                        Self::process_gate_batch(&transport, &mut gate_requests).await;
                        Self::process_config_batch(&transport, &mut config_requests).await;
                    }
                }
                _ = interval.tick() => {
                    if !gate_requests.is_empty() {
                        Self::process_gate_batch(&transport, &mut gate_requests).await;
                    }
                    if !config_requests.is_empty() {
                        Self::process_config_batch(&transport, &mut config_requests).await;
                    }
                }
                _ = self.shutdown_rx.recv() => {
                    info!("Batch processor shutting down");
                    break;
                }
            }
        }
    }

    /// Processes a batch of gate requests
    async fn process_gate_batch(transport: &StatsigTransport, requests: &mut Vec<BatchRequest>) {
        if requests.is_empty() {
            return;
        }

        let batch = std::mem::take(requests);

        // Group by user for efficiency
        let mut user_groups: HashMap<String, Vec<_>> = HashMap::new();
        for request in batch {
            if let BatchRequest::CheckGates { user, .. } = &request {
                let user_hash = Self::hash_user_for_batch(user);
                user_groups.entry(user_hash).or_default().push(request);
            }
        }

        for (_user_hash, group_requests) in user_groups {
            if let Some(first_request) = group_requests.first() {
                if let BatchRequest::CheckGates { user, .. } = first_request {
                    let all_gate_names: Vec<String> = group_requests
                        .iter()
                        .filter_map(|req| {
                            if let BatchRequest::CheckGates { gate_names, .. } = req {
                                Some(gate_names.clone())
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .collect();

                    match transport.check_gates(all_gate_names, user).await {
                        Ok(results) => {
                            // Distribute results back to requesters
                            for request in group_requests {
                                if let BatchRequest::CheckGates {
                                    gate_names,
                                    response_tx,
                                    ..
                                } = request
                                {
                                    let filtered_results: Vec<GateEvaluationResult> = results
                                        .iter()
                                        .filter(|result| gate_names.contains(&result.name))
                                        .cloned()
                                        .collect();
                                    let _ = response_tx.send(Ok(filtered_results));
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to fetch gates from API: {:?}", e);
                            // Send error to all requesters
                            for request in group_requests {
                                if let BatchRequest::CheckGates { response_tx, .. } = request {
                                    let _ = response_tx.send(Err(e.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Processes a batch of config requests
    async fn process_config_batch(transport: &StatsigTransport, requests: &mut Vec<BatchRequest>) {
        if requests.is_empty() {
            return;
        }

        let batch = std::mem::take(requests);

        // Process each config request individually for now (could be optimized)
        for request in batch {
            if let BatchRequest::GetConfigs {
                config_names,
                user,
                response_tx,
            } = request
            {
                let results = Self::fetch_configs_from_api(transport, &config_names, &user).await;
                let _ = response_tx.send(results);
            }
        }
    }

    /// Fetches configs from the Statsig API
    async fn fetch_configs_from_api(
        transport: &StatsigTransport,
        config_names: &[String],
        user: &User,
    ) -> Result<Vec<ConfigEvaluationResult>> {
        let mut results = Vec::new();

        for config_name in config_names {
            let config_result = transport.get_config(config_name, user).await?;
            results.push(config_result);
        }

        Ok(results)
    }

    /// Hashes user for batch grouping
    fn hash_user_for_batch(user: &User) -> String {
        user.hash_for_cache()
    }
}
