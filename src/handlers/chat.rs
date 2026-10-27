use actix_web::{web, HttpRequest, HttpResponse, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;
use validator::Validate;

use crate::config::AiConfig;
use crate::models::{ChatRequest, ChatResponse, ErrorResponse};
use crate::utils::cache_key;
use crate::AppState;

pub async fn chat(
    state: web::Data<AppState>,
    http_req: HttpRequest,
    req: web::Json<ChatRequest>,
) -> Result<HttpResponse> {
    // Validate request
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse::with_details(
            "Invalid request",
            format!("Validation error: {}", e),
        )));
    }

    let conversation_id = req.conversation_id.unwrap_or_else(Uuid::new_v4);
    let model_name = req.model.clone().unwrap_or_else(|| "auto".to_string());
    if model_name != "auto"
        && model_name != state.config.ai.model_name
        && model_name != state.config.ai.fallback_model_name
    {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse::new(
            "Unknown local model; use auto, MODEL_NAME, or FALLBACK_MODEL_NAME",
        )));
    }
    let cache_key = chat_cache_key(&req, &state.config.ai);

    let cache_bypass = req.cache_bypass.unwrap_or(false);
    let wants_stream = req.stream.unwrap_or(false)
        || http_req
            .headers()
            .get(actix_web::http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|value| {
                value.contains("text/event-stream")
                    || value.contains("application/x-ndjson")
                    || value.contains("application/jsonl")
            })
            .unwrap_or(false);
    let use_cache = !cache_bypass && rand::random::<f32>() < state.config.cache.cache_probability;

    if use_cache {
        if let Some((cached, source)) = state.cache_service.get(&cache_key).await {
            if let Ok(mut cached_response) = serde_json::from_value::<ChatResponse>(cached) {
                cached_response.cache_hit = true;
                cached_response.cache_source = Some(source.as_str().to_string());
                cached_response.conversation_id = conversation_id;
                cached_response.timestamp = chrono::Utc::now();
                if wants_stream {
                    return Ok(stream_text_response(
                        &http_req,
                        cached_response.response.clone(),
                        cached_response.model.clone(),
                        true,
                        cached_response.cache_source.clone(),
                        conversation_id,
                        cached_response.fallback_used,
                        cached_response.finish_reason.clone(),
                    ));
                }
                return respond_chat(http_req, cached_response);
            }
        }
    }

    let response = state.ai_service.generate(&req).await;

    match response {
        Ok(mut chat_response) => {
            chat_response.conversation_id = conversation_id;
            chat_response.cache_hit = false;
            chat_response.cache_source = None;
            let value = serde_json::to_value(&chat_response)
                .unwrap_or_else(|_| serde_json::json!({ "response": chat_response.response }));
            if use_cache && chat_response.finish_reason == "stop" {
                let _ = state.cache_service.set(&cache_key, &value).await;
            }
            if wants_stream {
                return Ok(stream_text_response(
                    &http_req,
                    chat_response.response.clone(),
                    chat_response.model.clone(),
                    false,
                    None,
                    conversation_id,
                    chat_response.fallback_used,
                    chat_response.finish_reason.clone(),
                ));
            }
            respond_chat(http_req, chat_response)
        }
        Err(e) => {
            tracing::error!("Chat error: {:?}", e);
            Ok(
                HttpResponse::InternalServerError().json(ErrorResponse::with_details(
                    "Failed to process chat request",
                    e.to_string(),
                )),
            )
        }
    }
}

fn chat_cache_key(req: &ChatRequest, config: &AiConfig) -> String {
    cache_key(&[
        "generic-tiny-qwen-v2",
        req.effective_system_prompt(&config.system_prompt),
        &req.message,
        req.model.as_deref().unwrap_or("auto"),
        &config.model_name,
        &config.model_path,
        &config.fallback_model_name,
        &config.fallback_model_path,
        &config.fallback_tokenizer_path,
        &config.context_length.to_string(),
        &config.fallback_context_length.to_string(),
        &config.tiny_max_tokens.to_string(),
        &config.top_p.to_string(),
        &req.temperature.unwrap_or(config.temperature).to_string(),
        &req.max_tokens.unwrap_or(config.max_tokens).to_string(),
    ])
}

fn respond_chat(http_req: HttpRequest, chat_response: ChatResponse) -> Result<HttpResponse> {
    let accept = http_req
        .headers()
        .get(actix_web::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if accept.contains("text/plain") {
        return Ok(HttpResponse::Ok()
            .content_type("text/plain; charset=utf-8")
            .body(chat_response.response));
    }

    Ok(HttpResponse::Ok().json(chat_response))
}

fn stream_text_response(
    _http_req: &HttpRequest,
    response: String,
    model_name: String,
    cache_hit: bool,
    cache_source: Option<String>,
    conversation_id: Uuid,
    fallback_used: bool,
    finish_reason: String,
) -> HttpResponse {
    let (tx, rx) = mpsc::channel::<Bytes>(32);
    tokio::spawn(async move {
        let words: Vec<&str> = response.split_whitespace().collect();
        for (index, word) in words.iter().enumerate() {
            let token = if index == 0 {
                (*word).to_string()
            } else {
                format!(" {}", word)
            };
            let payload = serde_json::json!({
                "model": model_name,
                "created_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
                "response": token,
                "done": false
            });
            let line = format!("{}\n", payload);
            if tx.send(Bytes::from(line)).await.is_err() {
                return;
            }
            sleep(Duration::from_millis(60)).await;
        }

        let done_payload = serde_json::json!({
            "model": model_name,
            "created_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            "response": "",
            "done": true,
            "cache_hit": cache_hit,
            "cache_source": cache_source,
            "conversation_id": conversation_id,
            "fallback_used": fallback_used,
            "finish_reason": finish_reason,
        });
        let line = format!("{}\n", done_payload);
        let _ = tx.send(Bytes::from(line)).await;
    });

    let stream = ReceiverStream::new(rx).map(Ok::<Bytes, std::io::Error>);
    HttpResponse::Ok()
        .insert_header((
            actix_web::http::header::CONTENT_TYPE,
            "application/x-ndjson",
        ))
        .streaming(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::StatusCode, test, App};
    use std::sync::{atomic::AtomicBool, Arc};
    use std::time::Instant;
    use tokio::sync::RwLock;

    use crate::config::Config;
    use crate::models::AIModel;
    use crate::services::{AIService, CacheService};

    #[actix_web::test]
    async fn cache_hits_skip_inference_and_are_scoped_to_effective_system_prompt() {
        let mut config = Config::default();
        config.ai.system_prompt = "Translate into Persian.".into();
        config.cache.redis_url.clear();
        config.cache.sqlite_path.clear();
        let cache_service = CacheService::new(config.cache.clone()).await.unwrap();
        let req: ChatRequest =
            serde_json::from_value(serde_json::json!({"message": "hi"})).unwrap();
        let cached = ChatResponse {
            response: "سلام".into(),
            conversation_id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            model: config.ai.fallback_model_name.clone(),
            fallback_used: true,
            finish_reason: "stop".into(),
            cache_hit: false,
            cache_source: None,
        };
        cache_service
            .set(
                &chat_cache_key(&req, &config.ai),
                &serde_json::to_value(cached).unwrap(),
            )
            .await
            .unwrap();

        // No loaded models: success here proves the cache bypasses CPU inference.
        let ai_model = Arc::new(RwLock::new(AIModel::new(config.ai.clone())));
        let state = AppState {
            ai_service: AIService::new(ai_model.clone(), config.ai.clone()),
            ai_model,
            models_ready: Arc::new(AtomicBool::new(false)),
            cache_service,
            config: config.clone(),
            start_time: Instant::now(),
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .route("/api/chat", web::post().to(chat)),
        )
        .await;

        for prompt in [None, Some("Translate into Persian.")] {
            let id = Uuid::new_v4();
            let request = test::TestRequest::post().uri("/api/chat")
                .set_json(serde_json::json!({"message": "hi", "system_prompt": prompt, "conversation_id": id}))
                .to_request();
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::OK);
            let body: ChatResponse = test::read_body_json(response).await;
            assert_eq!(body.response, "سلام");
            assert!(body.cache_hit);
            assert_eq!(body.cache_source.as_deref(), Some("memory"));
            assert_eq!(body.conversation_id, id);
        }

        for body in [
            serde_json::json!({"message": "hi", "system_prompt": "Answer in English."}),
            serde_json::json!({"message": "hi", "system_prompt": ""}),
            serde_json::json!({"message": "hello"}),
            serde_json::json!({"message": "hi", "cache_bypass": true}),
        ] {
            let request = test::TestRequest::post()
                .uri("/api/chat")
                .set_json(body)
                .to_request();
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        }

        let mut changed_default = config.ai.clone();
        changed_default.system_prompt = "Answer in English.".into();
        assert_ne!(
            chat_cache_key(&req, &config.ai),
            chat_cache_key(&req, &changed_default)
        );
    }
}
