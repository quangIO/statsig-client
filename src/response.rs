//! Unified API response handling for Statsig client
//!
//! This module provides consistent response handling across all API endpoints,
//! with proper error mapping and response parsing.

use crate::{
    api::{ConfigEvaluationResult, GateEvaluationResult},
    error::{Result, StatsigError},
};
use reqwest::Response;
use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Handles API responses with consistent error mapping and parsing
pub struct ApiResponseHandler;

impl ApiResponseHandler {
    /// Handles a generic API response
    pub async fn handle<T: DeserializeOwned>(response: Response) -> Result<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.text().await?;
            Self::parse_json(&body)
                .map_err(|e| e.with_context(&format!("Response body: {}", Self::truncate(&body))))
        } else {
            Err(Self::error_from_response(status, response).await)
        }
    }

    /// Handles gate-specific API responses with custom parsing
    pub async fn handle_gate_response(response: Response) -> Result<Vec<GateEvaluationResult>> {
        let status = response.status();

        if status.is_success() {
            let body = response.text().await?;

            #[derive(Deserialize)]
            struct GateEvaluationResultWire {
                #[serde(default)]
                name: Option<String>,
                value: bool,
                #[serde(rename = "rule_id")]
                rule_id: Option<String>,
                #[serde(rename = "group_name")]
                group_name: Option<String>,
            }

            let map: std::collections::HashMap<String, GateEvaluationResultWire> =
                Self::parse_json(&body).map_err(|e| {
                    e.with_context(&format!("Response body: {}", Self::truncate(&body)))
                })?;

            Ok(map
                .into_iter()
                .map(|(gate_name, wire)| GateEvaluationResult {
                    name: wire.name.unwrap_or(gate_name),
                    value: wire.value,
                    rule_id: wire.rule_id,
                    group_name: wire.group_name,
                })
                .collect())
        } else {
            Err(Self::error_from_response(status, response).await)
        }
    }

    /// Handles config-specific API responses
    pub async fn handle_config_response(response: Response) -> Result<ConfigEvaluationResult> {
        Self::handle(response).await
    }

    fn parse_json<T: DeserializeOwned>(body: &str) -> Result<T> {
        serde_json::from_str(body)
            .map_err(|e| StatsigError::serialization(format!("Failed to parse response JSON: {e}")))
    }

    fn truncate(body: &str) -> String {
        const LIMIT: usize = 2_000;
        if body.len() <= LIMIT {
            body.to_string()
        } else {
            format!("{}...(truncated)", &body[..LIMIT])
        }
    }

    async fn error_from_response(status: reqwest::StatusCode, response: Response) -> StatsigError {
        let headers = response.headers().clone();
        let body = match response.text().await {
            Ok(body) => body,
            Err(err) => return StatsigError::from(err),
        };

        match status.as_u16() {
            401 => StatsigError::Unauthorized,
            429 => {
                let retry_after_seconds = headers
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                StatsigError::rate_limited(retry_after_seconds)
            }
            _ => StatsigError::api(status.as_u16(), Self::truncate(&body)),
        }
    }
}
