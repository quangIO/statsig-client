use bon::Builder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEventResponse {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureEventMetadata {
    pub gate: String,
    #[serde(rename = "gateValue")]
    pub gate_value: String,
    #[serde(rename = "ruleID")]
    pub rule_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StatsigEventValue {
    String(String),
    Number(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StatsigEventTime {
    UnixMillis(i64),
    IsoDateTime(String),
}

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
pub struct StatsigEvent {
    #[serde(rename = "eventName")]
    #[builder(into)]
    pub event_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<StatsigEventValue>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<StatsigEventTime>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<crate::user::User>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,

    #[serde(rename = "secondaryExposures", skip_serializing_if = "Option::is_none")]
    pub secondary_exposures: Option<Vec<ExposureEventMetadata>>,

    #[serde(rename = "statsigMetadata", skip_serializing_if = "Option::is_none")]
    pub statsig_metadata: Option<crate::api::StatsigMetadata>,
}
