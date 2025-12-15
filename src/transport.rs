use crate::{
    api::{ConfigEvaluationResult, GateEvaluationResult, StatsigMetadata},
    config::StatsigClientConfig,
    error::{Result, StatsigError},
    events::{LogEventResponse, StatsigEvent},
    response::ApiResponseHandler,
    user::User,
};
use backoff::backoff::Backoff;
use http::Extensions;
use httpdate::parse_http_date;
use reqwest::Request;
use reqwest::Response;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Middleware, Next};
use reqwest_retry::{
    RetryDecision, RetryPolicy, RetryTransientMiddleware, Retryable, RetryableStrategy,
};
use serde::Serialize;
use std::time::{Duration, SystemTime};
use thiserror::Error;

#[derive(Debug, Error)]
enum TransportMiddlewareError {
    #[error("Request object is not cloneable. Are you passing a streaming body?")]
    UncloneableRequest,
}

#[derive(Debug, Clone)]
pub struct StatsigTransport {
    client: ClientWithMiddleware,
    base_url: String,
    events_base_url: String,
    api_key: String,
    exposure_logging_disabled: bool,
}

impl StatsigTransport {
    pub fn new(config: &StatsigClientConfig) -> Result<Self> {
        let inner = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent(format!("{}/{}", config.sdk_type, config.sdk_version))
            .build()
            .map_err(|e| {
                StatsigError::configuration(format!("Failed to create HTTP client: {}", e))
            })?;

        let retry_policy = BackoffRetryPolicy::new(config.retry_attempts, config.retry_delay);
        let retry_transient = RetryTransientMiddleware::new_with_policy_and_strategy(
            retry_policy,
            No429RetryStrategy,
        );

        let client = ClientBuilder::new(inner)
            .with(RateLimitRetryMiddleware::new(
                config.retry_attempts,
                config.retry_delay,
            ))
            .with(retry_transient)
            .build();

        Ok(Self {
            client,
            base_url: config.base_url.clone(),
            events_base_url: config.events_base_url.clone(),
            api_key: config.api_key.clone(),
            exposure_logging_disabled: config.exposure_logging_disabled,
        })
    }

    async fn post_sdk<T: Serialize>(&self, path: &str, body: &T) -> Result<Response> {
        let response = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .header("statsig-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;

        Ok(response)
    }

    async fn post_events<T: Serialize>(&self, path: &str, body: &T) -> Result<Response> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string();

        let response = self
            .client
            .post(format!("{}{}", self.events_base_url, path))
            .header("statsig-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .header("STATSIG-CLIENT-TIME", now_ms)
            .json(body)
            .send()
            .await?;

        Ok(response)
    }

    pub async fn check_gates(
        &self,
        gate_names: Vec<String>,
        user: &User,
    ) -> Result<Vec<GateEvaluationResult>> {
        #[derive(Serialize)]
        struct CheckGateRequest<'a> {
            #[serde(rename = "gateNames")]
            gate_names: Vec<String>,
            user: &'a User,
            #[serde(rename = "statsigMetadata")]
            statsig_metadata: StatsigMetadata,
        }

        let request_body = CheckGateRequest {
            gate_names,
            user,
            statsig_metadata: StatsigMetadata::default()
                .with_exposure_logging_disabled(self.exposure_logging_disabled),
        };

        let response = self.post_sdk("/v1/check_gate", &request_body).await?;

        ApiResponseHandler::handle_gate_response(response).await
    }

    pub async fn get_config(
        &self,
        config_name: &str,
        user: &User,
    ) -> Result<ConfigEvaluationResult> {
        #[derive(Serialize)]
        struct GetConfigRequest<'a> {
            #[serde(rename = "configName")]
            config_name: String,
            user: &'a User,
            #[serde(rename = "statsigMetadata")]
            statsig_metadata: StatsigMetadata,
        }

        let request_body = GetConfigRequest {
            config_name: config_name.to_string(),
            user,
            statsig_metadata: StatsigMetadata::default()
                .with_exposure_logging_disabled(self.exposure_logging_disabled),
        };

        let response = self.post_sdk("/v1/get_config", &request_body).await?;

        ApiResponseHandler::handle_config_response(response).await
    }

    pub async fn log_events(
        &self,
        user: &User,
        events: &[StatsigEvent],
    ) -> Result<LogEventResponse> {
        #[derive(Serialize)]
        struct LogEventRequest<'a> {
            events: &'a [StatsigEvent],
            #[serde(skip_serializing_if = "Option::is_none")]
            user: Option<&'a User>,
            #[serde(rename = "statsigMetadata", skip_serializing_if = "Option::is_none")]
            statsig_metadata: Option<StatsigMetadata>,
        }

        let request_body = LogEventRequest {
            events,
            user: Some(user),
            statsig_metadata: Some(
                StatsigMetadata::default()
                    .with_exposure_logging_disabled(self.exposure_logging_disabled),
            ),
        };

        let response = self.post_events("/v1/log_event", &request_body).await?;

        ApiResponseHandler::handle(response).await
    }
}

#[derive(Debug, Clone)]
struct RateLimitRetryMiddleware {
    max_retries: u32,
    fallback_delay: Duration,
}

impl RateLimitRetryMiddleware {
    fn new(max_retries: u32, fallback_delay: Duration) -> Self {
        Self {
            max_retries,
            fallback_delay,
        }
    }
}

#[async_trait::async_trait]
impl Middleware for RateLimitRetryMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest::Response> {
        let mut n_past_retries: u32 = 0;
        loop {
            let duplicate_request = req.try_clone().ok_or_else(|| {
                reqwest_middleware::Error::middleware(TransportMiddlewareError::UncloneableRequest)
            })?;

            let response = next.clone().run(duplicate_request, extensions).await?;
            if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Ok(response);
            }

            if n_past_retries >= self.max_retries {
                return Ok(response);
            }

            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after)
                .unwrap_or(self.fallback_delay);

            tokio::time::sleep(retry_after).await;
            n_past_retries += 1;
        }
    }
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let when = parse_http_date(value).ok()?;
    let now = SystemTime::now();
    when.duration_since(now).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_retry_after_seconds() {
        assert_eq!(parse_retry_after("2"), Some(Duration::from_secs(2)));
    }

    #[test]
    fn parse_retry_after_http_date() {
        let when = SystemTime::now() + Duration::from_secs(2);
        let header = httpdate::fmt_http_date(when);
        let delay = parse_retry_after(&header).unwrap();
        assert!(delay > Duration::from_millis(0));
        assert!(delay <= Duration::from_secs(2));
    }
}

#[derive(Debug, Clone, Copy)]
struct No429RetryStrategy;

impl RetryableStrategy for No429RetryStrategy {
    fn handle(
        &self,
        res: &std::result::Result<reqwest::Response, reqwest_middleware::Error>,
    ) -> Option<Retryable> {
        match res {
            Ok(success) => {
                let status = success.status();
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    Some(Retryable::Fatal)
                } else if status.is_server_error() || status == reqwest::StatusCode::REQUEST_TIMEOUT
                {
                    Some(Retryable::Transient)
                } else if status.is_success() {
                    None
                } else {
                    Some(Retryable::Fatal)
                }
            }
            Err(error) => reqwest_retry::default_on_request_failure(error),
        }
    }
}

#[derive(Debug, Clone)]
struct BackoffRetryPolicy {
    max_retries: u32,
    initial_interval: Duration,
}

impl BackoffRetryPolicy {
    fn new(max_retries: u32, initial_interval: Duration) -> Self {
        Self {
            max_retries,
            initial_interval,
        }
    }
}

impl RetryPolicy for BackoffRetryPolicy {
    fn should_retry(&self, request_start_time: SystemTime, n_past_retries: u32) -> RetryDecision {
        if n_past_retries >= self.max_retries {
            return RetryDecision::DoNotRetry;
        }

        let _ = request_start_time;

        let mut backoff = backoff::ExponentialBackoffBuilder::new()
            .with_initial_interval(self.initial_interval)
            .with_randomization_factor(0.5)
            .with_multiplier(2.0)
            .with_max_interval(Duration::from_secs(60))
            .with_max_elapsed_time(None)
            .build();
        backoff.reset();

        let mut delay = self.initial_interval;
        for _ in 0..=n_past_retries {
            delay = backoff.next_backoff().unwrap_or(self.initial_interval);
        }

        RetryDecision::Retry {
            execute_after: SystemTime::now() + delay,
        }
    }
}
