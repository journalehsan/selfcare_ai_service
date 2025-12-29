use anyhow::Result;
use serde_json::json;
use tokio::sync::RwLock;
use std::sync::Arc;

use crate::config::{AiConfig, OpenRouterSettings};
use crate::models::{ChatRequest, ChatResponse};
use crate::models::AIModel;
use crate::services::{ModelService, SearchService};

#[derive(Clone)]
pub struct AIService {
    ai_model: Arc<RwLock<AIModel>>,
    model_service: ModelService,
    search_service: SearchService,
    openrouter: OpenRouterSettings,
    ai_config: AiConfig,
}

impl AIService {
    pub fn new(
        ai_model: Arc<RwLock<AIModel>>,
        ai_config: AiConfig,
        openrouter: OpenRouterSettings,
    ) -> Self {
        Self {
            ai_model,
            model_service: ModelService::default(),
            search_service: SearchService::default(),
            openrouter,
            ai_config,
        }
    }

    pub async fn analyze_complexity(&self, req: &ChatRequest) -> crate::services::Complexity {
        self.model_service.analyze_complexity(req)
    }

    pub async fn local_model_generate(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let conversation_id = req.conversation_id.unwrap_or_else(uuid::Uuid::new_v4);
        let mut model = self.ai_model.write().await;
        let temperature = req.temperature.unwrap_or(self.ai_config.temperature);
        let max_tokens = req.max_tokens.unwrap_or(self.ai_config.max_tokens);
        let response = model
            .chat_with_params(&req.message, Some(conversation_id.to_string()), temperature, max_tokens)
            .await?;
        Ok(ChatResponse::new(response, conversation_id))
    }

    pub async fn enrich_and_generate(
        &self,
        req: &ChatRequest,
        search_results: &[crate::services::SearchResult],
    ) -> Result<ChatResponse> {
        if search_results.is_empty() {
            return self.local_model_generate(req).await;
        }

        let enrichment = json!({
            "sources": search_results
                .iter()
                .map(|result| {
                    json!({
                        "title": result.title,
                        "url": result.url,
                        "snippet": result.snippet
                    })
                })
                .collect::<Vec<_>>()
        });

        let enriched_message = format!(
            "{}\n\nAdditional context (sources): {}",
            req.message,
            enrichment
        );

        let enriched_req = ChatRequest {
            message: enriched_message,
            conversation_id: req.conversation_id,
            model: req.model.clone(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            cache_bypass: req.cache_bypass,
            stream: req.stream,
        };

        self.local_model_generate(&enriched_req).await
    }

    pub async fn cloud_model_generate(
        &self,
        req: &ChatRequest,
        search_results: &[crate::services::SearchResult],
    ) -> Result<ChatResponse> {
        if self.openrouter.api_key.trim().is_empty() {
            return self.enrich_and_generate(req, search_results).await;
        }

        let _ = search_results;
        let model = req
            .model
            .clone()
            .unwrap_or_else(|| self.openrouter.default_model.clone());
        let temperature = req.temperature.unwrap_or(self.ai_config.temperature);
        let max_tokens = req.max_tokens.unwrap_or(self.ai_config.max_tokens) as u32;

        let response = reqwest::Client::new()
            .post(format!("{}/chat/completions", self.openrouter.base_url))
            .bearer_auth(&self.openrouter.api_key)
            .json(&json!({
                "model": model,
                "messages": [{"role": "user", "content": req.message}],
                "temperature": temperature,
                "max_tokens": max_tokens,
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        let content = response
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .unwrap_or("No response from OpenRouter");

        let conversation_id = req.conversation_id.unwrap_or_else(uuid::Uuid::new_v4);
        Ok(ChatResponse::new(content.to_string(), conversation_id))
    }

    pub async fn search(&self, query: &str) -> Result<Vec<crate::services::SearchResult>> {
        self.search_service.search(query).await
    }
}
