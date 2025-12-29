use actix_web::{web, HttpRequest, HttpResponse, Result, Responder};
use actix_web_lab::sse;
use uuid::Uuid;
use validator::Validate;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::models::{ChatRequest, ChatResponse, ErrorResponse};
use crate::services::Complexity;
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
    let model_name = req
        .model
        .clone()
        .unwrap_or_else(|| state.config.ai.model_name.clone());
    let temperature = req.temperature.unwrap_or(state.config.ai.temperature);
    let max_tokens = req.max_tokens.unwrap_or(state.config.ai.max_tokens);

    let cache_key = cache_key(&[
        &req.message,
        &model_name,
        &temperature.to_string(),
        &max_tokens.to_string(),
    ]);

    let cache_bypass = req.cache_bypass.unwrap_or(false);
    let wants_stream = req.stream.unwrap_or(false)
        || http_req
            .headers()
            .get(actix_web::http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|value| value.contains("text/event-stream"))
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
                        true,
                        cached_response.cache_source.clone(),
                    ));
                }
                return respond_chat(http_req, cached_response);
            }
        }
    }

    let complexity = state.ai_service.analyze_complexity(&req).await;
    let response = match complexity {
        Complexity::Low => state.ai_service.local_model_generate(&req).await,
        Complexity::Medium => {
            let search_results = state.ai_service.search(&req.message).await;
            match search_results {
                Ok(results) => state.ai_service.enrich_and_generate(&req, &results).await,
                Err(err) => Err(err),
            }
        }
        Complexity::High => {
            let search_results = state.ai_service.search(&req.message).await;
            match search_results {
                Ok(results) => state.ai_service.cloud_model_generate(&req, &results).await,
                Err(err) => Err(err),
            }
        }
    };

    match response {
        Ok(mut chat_response) => {
            chat_response.conversation_id = conversation_id;
            chat_response.cache_hit = false;
            chat_response.cache_source = None;
            let value = serde_json::to_value(&chat_response).unwrap_or_else(|_| {
                serde_json::json!({ "response": chat_response.response })
            });
            if use_cache {
                let _ = state.cache_service.set(&cache_key, &value).await;
            }
            if wants_stream {
                return Ok(stream_text_response(
                    &http_req,
                    chat_response.response.clone(),
                    false,
                    None,
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
    http_req: &HttpRequest,
    response: String,
    cache_hit: bool,
    cache_source: Option<String>,
) -> HttpResponse {
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move {
        let words: Vec<&str> = response.split_whitespace().collect();
        for chunk in words.chunks(3) {
            let data = chunk.join(" ");
            let event = sse::Event::Data(sse::Data::new(data).event("token"));
            if tx.send(event).await.is_err() {
                return;
            }
            sleep(Duration::from_millis(60)).await;
        }

        let meta = serde_json::json!({
            "cache_hit": cache_hit,
            "cache_source": cache_source,
        });
        let meta_event = match sse::Data::new_json(meta) {
            Ok(data) => sse::Event::Data(data.event("meta")),
            Err(_) => sse::Event::Data(sse::Data::new("")),
        };
        let _ = tx.send(meta_event).await;
        let _ = tx.send(sse::Event::Data(sse::Data::new("").event("done"))).await;
    });

    sse::Sse::from_infallible_receiver(rx)
        .with_keep_alive(Duration::from_secs(3))
        .respond_to(http_req)
}
