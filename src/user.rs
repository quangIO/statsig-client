use bon::bon;
use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default)]
pub struct User {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(length(
        min = 1,
        max = 100,
        message = "userID must be between 1 and 100 characters"
    ))]
    #[serde(rename = "userID")]
    pub user_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(length(min = 7, max = 45, message = "Invalid IP address format"))]
    pub ip: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "userAgent")]
    pub user_agent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(length(min = 2, max = 2, message = "Country must be a 2-letter ISO code"))]
    pub country: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "appVersion")]
    pub app_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<HashMap<String, serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "privateAttributes")]
    pub private_attributes: Option<HashMap<String, serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "customIDs")]
    pub custom_ids: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "statsigEnvironment")]
    pub statsig_environment: Option<StatsigEnvironment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StatsigEnvironment {
    #[serde(rename = "tier")]
    #[validate(custom(function = "validate_tier"))]
    pub tier: EnvironmentTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentTier {
    Production,
    Staging,
    Development,
}

fn validate_tier(tier: &EnvironmentTier) -> Result<(), validator::ValidationError> {
    match tier {
        EnvironmentTier::Production | EnvironmentTier::Staging | EnvironmentTier::Development => {
            Ok(())
        }
    }
}

#[bon]
impl User {
    #[builder]
    pub fn new(
        #[builder(into)] user_id: Option<String>,
        #[builder(into)] email: Option<String>,
        #[builder(into)] ip: Option<String>,
        #[builder(into)] user_agent: Option<String>,
        #[builder(into)] country: Option<String>,
        #[builder(into)] locale: Option<String>,
        #[builder(into)] app_version: Option<String>,
        #[builder(with = |iter: impl IntoIterator<Item = (impl Into<String>, serde_json::Value)>| {
            iter.into_iter().map(|(k, v)| (k.into(), v)).collect()
        })]
        custom: Option<HashMap<String, serde_json::Value>>,
        #[builder(with = |iter: impl IntoIterator<Item = (impl Into<String>, serde_json::Value)>| {
            iter.into_iter().map(|(k, v)| (k.into(), v)).collect()
        })]
        private_attributes: Option<HashMap<String, serde_json::Value>>,
        #[builder(with = |iter: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>| {
            iter.into_iter().map(|(k, v)| (k.into(), v.into())).collect()
        })]
        custom_ids: Option<HashMap<String, String>>,
        statsig_environment: Option<StatsigEnvironment>,
    ) -> crate::error::Result<Self> {
        let user = Self {
            user_id,
            email,
            ip,
            user_agent,
            country,
            locale,
            app_version,
            custom,
            private_attributes,
            custom_ids,
            statsig_environment,
        };

        user.validate().map_err(|e| {
            crate::error::StatsigError::user_validation(format!("User validation failed: {e}"))
        })?;

        Ok(user)
    }
}

impl User {
    pub fn validate_user(&self) -> crate::error::Result<()> {
        self.validate().map_err(|e| {
            crate::error::StatsigError::user_validation(format!("User validation failed: {e}"))
        })
    }

    /// Creates a new user with just a user ID
    ///
    /// This is a convenience method for the most common use case
    pub fn with_user_id(user_id: impl Into<String>) -> UserBuilder<user_builder::SetUserId> {
        Self::builder().user_id(user_id)
    }

    pub fn get_primary_id(&self) -> Option<&str> {
        self.user_id
            .as_deref()
            .or(self.email.as_deref())
            .or_else(|| {
                self.custom_ids
                    .as_ref()
                    .and_then(|ids| ids.values().next().map(|s| s.as_str()))
            })
    }

    /// Get user ID (alias for userID for consistency)
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// Generates a consistent hash for the user used in cache keys and batch grouping
    ///
    /// This hash includes all user identifiers to ensure consistent cache behavior
    /// across different user representations with the same logical identity.
    pub fn hash_for_cache(&self) -> String {
        let mut hasher = Sha256::new();

        if let Some(user_id) = &self.user_id {
            hasher.update(user_id.as_bytes());
        }
        if let Some(email) = &self.email {
            hasher.update(email.as_bytes());
        }
        if let Some(custom_ids) = &self.custom_ids {
            for (key, value) in custom_ids {
                hasher.update(key.as_bytes());
                hasher.update(value.as_bytes());
            }
        }

        hex::encode(hasher.finalize())
    }
}

impl Default for StatsigEnvironment {
    fn default() -> Self {
        Self {
            tier: EnvironmentTier::Development,
        }
    }
}
