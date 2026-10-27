use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub conversation_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub fallback_used: bool,
    pub finish_reason: String,
    pub cache_hit: bool,
    pub cache_source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogAnalysisResponse {
    pub analysis: String,
    pub issues: Vec<String>,
    pub recommendations: Vec<String>,
    pub severity: String,
    pub confidence: f32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ScriptResponse {
    pub script: String,
    pub language: String,
    pub environment: String,
    pub explanation: String,
    pub safety_warnings: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub model_loaded: bool,
    pub uptime_seconds: u64,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_details(error: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: Some(details.into()),
            timestamp: Utc::now(),
        }
    }
}
