use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatRequest {
    #[validate(length(min = 1, max = 100_000))]
    pub message: String,
    /// Overrides the service default; an empty string omits the application system prompt.
    #[validate(length(max = 100_000))]
    pub system_prompt: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub model: Option<String>,
    #[validate(range(min = 0.0, max = 2.0))]
    pub temperature: Option<f32>,
    #[validate(range(min = 1, max = 8192))]
    pub max_tokens: Option<usize>,
    pub cache_bypass: Option<bool>,
    pub stream: Option<bool>,
}

impl ChatRequest {
    pub fn effective_system_prompt<'a>(&'a self, default: &'a str) -> &'a str {
        self.system_prompt.as_deref().unwrap_or(default)
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct LogAnalysisRequest {
    #[validate(length(min = 1, max = 100_000))]
    pub logs: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Linux,
    Windows,
    MacOS,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptLanguage {
    Bash,
    Python,
    Powershell,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ScriptGenerationRequest {
    #[validate(length(min = 1, max = 100_000))]
    pub requirement: String,
    #[serde(alias = "type")]
    pub environment: Environment,
    pub language: ScriptLanguage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_override_and_validation() {
        for body in [
            serde_json::json!({"message": "hi"}),
            serde_json::json!({"message": "hi", "system_prompt": null}),
        ] {
            let req: ChatRequest = serde_json::from_value(body).unwrap();
            assert!(req.validate().is_ok());
            assert_eq!(req.effective_system_prompt("global"), "global");
        }
        for system in ["", "Translate into Persian."] {
            let req: ChatRequest = serde_json::from_value(serde_json::json!({
                "message": "hi", "system_prompt": system
            }))
            .unwrap();
            assert!(req.validate().is_ok());
            assert_eq!(req.effective_system_prompt("global"), system);
        }
        let req: ChatRequest = serde_json::from_value(serde_json::json!({
            "message": "hi", "system_prompt": "x".repeat(100_001)
        }))
        .unwrap();
        assert!(req.validate().is_err());
    }
}
