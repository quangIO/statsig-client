use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsigMetadata {
    #[serde(rename = "sdkType")]
    pub sdk_type: String,

    #[serde(rename = "sdkVersion")]
    pub sdk_version: String,

    #[serde(rename = "exposureLoggingDisabled")]
    pub exposure_logging_disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateEvaluationResult {
    pub name: String,
    pub value: bool,
    #[serde(rename = "rule_id")]
    pub rule_id: Option<String>,
    #[serde(rename = "group_name")]
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEvaluationResult {
    pub name: String,
    pub value: serde_json::Value,
    #[serde(rename = "rule_id")]
    pub rule_id: Option<String>,
    #[serde(rename = "group_name")]
    pub group_name: Option<String>,
    pub group: Option<String>,
}

impl StatsigMetadata {
    pub fn new(sdk_type: impl Into<String>, sdk_version: impl Into<String>) -> Self {
        Self {
            sdk_type: sdk_type.into(),
            sdk_version: sdk_version.into(),
            exposure_logging_disabled: false,
        }
    }

    pub fn with_exposure_logging_disabled(mut self, disabled: bool) -> Self {
        self.exposure_logging_disabled = disabled;
        self
    }
}

impl Default for StatsigMetadata {
    fn default() -> Self {
        Self {
            sdk_type: "rust-client".to_string(),
            sdk_version: env!("CARGO_PKG_VERSION").to_string(),
            exposure_logging_disabled: false,
        }
    }
}
