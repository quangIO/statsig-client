use thiserror::Error;

pub type Result<T> = std::result::Result<T, StatsigError>;

#[derive(Error, Debug, Clone)]
pub enum StatsigError {
    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid configuration: {0}")]
    Configuration(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Batch processor error: {0}")]
    BatchProcessor(String),

    #[error("Rate limited: retry after {retry_after_seconds} seconds")]
    RateLimited { retry_after_seconds: u64 },

    #[error("Unauthorized: invalid API key")]
    Unauthorized,

    #[error("User validation error: {0}")]
    UserValidation(String),

    #[error("Feature gate not found: {0}")]
    GateNotFound(String),

    #[error("Dynamic config not found: {0}")]
    ConfigNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl StatsigError {
    pub fn api(status: u16, message: impl Into<String>) -> Self {
        Self::Api {
            status,
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration(message.into())
    }

    pub fn cache(message: impl Into<String>) -> Self {
        Self::Cache(message.into())
    }

    pub fn batch_processor(message: impl Into<String>) -> Self {
        Self::BatchProcessor(message.into())
    }

    pub fn rate_limited(retry_after_seconds: u64) -> Self {
        Self::RateLimited {
            retry_after_seconds,
        }
    }

    pub fn user_validation(message: impl Into<String>) -> Self {
        Self::UserValidation(message.into())
    }

    pub fn gate_not_found(name: impl Into<String>) -> Self {
        Self::GateNotFound(name.into())
    }

    pub fn config_not_found(name: impl Into<String>) -> Self {
        Self::ConfigNotFound(name.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network(message.into())
    }

    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization(message.into())
    }

    /// Adds context to an error for better debugging and error reporting
    pub fn with_context(self, context: &str) -> Self {
        match self {
            Self::Api { status, message } => Self::Api {
                status,
                message: format!("{}: {}", context, message),
            },
            Self::Network(message) => Self::Network(format!("{}: {}", context, message)),
            Self::Serialization(message) => {
                Self::Serialization(format!("{}: {}", context, message))
            }
            Self::Validation(message) => Self::Validation(format!("{}: {}", context, message)),
            Self::Configuration(message) => {
                Self::Configuration(format!("{}: {}", context, message))
            }
            Self::Cache(message) => Self::Cache(format!("{}: {}", context, message)),
            Self::BatchProcessor(message) => {
                Self::BatchProcessor(format!("{}: {}", context, message))
            }
            Self::UserValidation(message) => {
                Self::UserValidation(format!("{}: {}", context, message))
            }
            Self::GateNotFound(name) => Self::GateNotFound(format!("{}: {}", context, name)),
            Self::ConfigNotFound(name) => Self::ConfigNotFound(format!("{}: {}", context, name)),
            Self::Internal(message) => Self::Internal(format!("{}: {}", context, message)),
            // These variants don't need context
            Self::RateLimited { .. } | Self::Unauthorized => self,
        }
    }
}

impl From<reqwest::Error> for StatsigError {
    fn from(err: reqwest::Error) -> Self {
        Self::Network(err.to_string())
    }
}

impl From<reqwest_middleware::Error> for StatsigError {
    fn from(err: reqwest_middleware::Error) -> Self {
        Self::Network(err.to_string())
    }
}

impl StatsigError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::RateLimited { .. } => true,
            Self::Api { status, .. } => matches!(status, 429 | 500..=599),
            _ => false,
        }
    }

    pub fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            Self::RateLimited {
                retry_after_seconds,
            } => Some(*retry_after_seconds),
            Self::Api { status, .. } if *status == 429 => Some(60),
            _ => None,
        }
    }
}
