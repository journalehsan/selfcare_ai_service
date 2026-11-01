use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::AiConfig;
use crate::models::{ChatRequest, ChatResponse, Generation, InferenceBackend, TaskComplexity};
use crate::utils::prompts::{generate_log_analysis_prompt, generate_script_prompt};

#[derive(Clone)]
pub struct AIService {
    ai_model: Arc<RwLock<dyn InferenceBackend>>,
    ai_config: AiConfig,
}

impl AIService {
    pub fn new(ai_model: Arc<RwLock<dyn InferenceBackend>>, ai_config: AiConfig) -> Self {
        Self {
            ai_model,
            ai_config,
        }
    }

    pub async fn evaluate_task(&self, req: &ChatRequest) -> Result<TaskComplexity> {
        let model = self.ai_model.clone();
        let prompt = req.message.clone();
        let system_prompt = req
            .effective_system_prompt(&self.ai_config.system_prompt)
            .to_string();
        tokio::task::spawn_blocking(move || {
            model
                .blocking_write()
                .evaluate_task(&prompt, &system_prompt)
        })
        .await
        .context("Task evaluation worker failed")
    }

    pub async fn generate(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let conversation_id = req.conversation_id.unwrap_or_else(uuid::Uuid::new_v4);
        let output = self
            .generate_prompt(
                req.message.clone(),
                req.effective_system_prompt(&self.ai_config.system_prompt)
                    .to_string(),
                req.temperature.unwrap_or(self.ai_config.temperature),
                req.max_tokens.unwrap_or(self.ai_config.max_tokens),
                req.model.clone(),
            )
            .await?;
        Ok(ChatResponse {
            response: output.text,
            conversation_id,
            timestamp: chrono::Utc::now(),
            model: output.model,
            fallback_used: output.fallback_used,
            finish_reason: output.finish_reason,
            cache_hit: false,
            cache_source: None,
        })
    }

    pub async fn analyze_logs(&self, logs: &str, context: Option<String>) -> Result<String> {
        let prompt = generate_log_analysis_prompt(logs, context);
        Ok(self
            .generate_prompt(
                prompt,
                self.ai_config.system_prompt.clone(),
                self.ai_config.temperature,
                self.ai_config.max_tokens,
                None,
            )
            .await?
            .text)
    }

    pub async fn generate_script(
        &self,
        requirement: &str,
        environment: &str,
        language: &str,
    ) -> Result<String> {
        let prompt = generate_script_prompt(requirement, environment, language);
        Ok(self
            .generate_prompt(
                prompt,
                self.ai_config.system_prompt.clone(),
                self.ai_config.temperature,
                self.ai_config.max_tokens,
                None,
            )
            .await?
            .text)
    }

    async fn generate_prompt(
        &self,
        prompt: String,
        system_prompt: String,
        temperature: f32,
        max_tokens: usize,
        requested_model: Option<String>,
    ) -> Result<Generation> {
        let model = self.ai_model.clone();
        // CPU inference runs outside Actix/Tokio worker threads. The write lock
        // serializes generations, keeping KV state and VM memory usage bounded.
        tokio::task::spawn_blocking(move || {
            model.blocking_write().generate(
                &prompt,
                &system_prompt,
                temperature,
                max_tokens,
                requested_model.as_deref(),
            )
        })
        .await
        .context("Local inference worker failed")?
    }
}
