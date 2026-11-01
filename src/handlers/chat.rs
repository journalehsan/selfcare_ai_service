use actix_web::{web, HttpRequest, HttpResponse, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;
use validator::Validate;

use crate::config::AiConfig;
use crate::models::{ChatRequest, ChatResponse, ErrorResponse, ModelSelection, TaskComplexity};
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
    let selection = match ModelSelection::parse(req.model.as_deref(), &state.config.ai) {
        Ok(selection) => selection,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse::new(
                "Unknown local model; use auto, qwen, MODEL_NAME, or FALLBACK_MODEL_NAME",
            )))
        }
    };
    let complexity = if selection == ModelSelection::Qwen {
        TaskComplexity::Complex
    } else {
        state
            .ai_service
            .evaluate_task(&req)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!("Task evaluation unavailable; using Qwen: {error:#}");
                TaskComplexity::Complex
            })
    };
    let ai = &state.config.ai;

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
        let mut candidates = Vec::new();
        if complexity == TaskComplexity::Simple {
            candidates.push(ai.model_name.as_str());
        }
        candidates.push(ai.fallback_model_name.as_str());
        for model in candidates {
            let key = chat_cache_key(&req, ai, model);
            if let Some((cached, source)) = state.cache_service.get(&key).await {
                let Ok(mut cached_response) = serde_json::from_value::<ChatResponse>(cached) else {
                    continue;
                };
                // Only complete replies from an eligible model are suitable.
                if cached_response.model != model
                    || cached_response.finish_reason != "stop"
                    || cached_response.response.trim().is_empty()
                {
                    continue;
                }
                cached_response.fallback_used = selection != ModelSelection::Qwen
                    && cached_response.model == ai.fallback_model_name;
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

    let mut routed_req = req.into_inner();
    routed_req.model = Some(if complexity == TaskComplexity::Simple {
        ai.model_name.clone()
    } else {
        ai.fallback_model_name.clone()
    });
    let response = state.ai_service.generate(&routed_req).await;

    match response {
        Ok(mut chat_response) => {
            chat_response.conversation_id = conversation_id;
            chat_response.cache_hit = false;
            chat_response.cache_source = None;
            chat_response.fallback_used =
                selection != ModelSelection::Qwen && chat_response.model == ai.fallback_model_name;
            let value = serde_json::to_value(&chat_response)
                .unwrap_or_else(|_| serde_json::json!({ "response": chat_response.response }));
            if use_cache && chat_response.finish_reason == "stop" {
                let key = chat_cache_key(&routed_req, ai, &chat_response.model);
                let _ = state.cache_service.set(&key, &value).await;
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

fn chat_cache_key(req: &ChatRequest, config: &AiConfig, response_model: &str) -> String {
    cache_key(&[
        "task-routing-v3",
        req.effective_system_prompt(&config.system_prompt),
        &req.message,
        response_model,
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
    use std::sync::{atomic::AtomicBool, Arc, Mutex};
    use std::time::Instant;
    use tokio::sync::RwLock;

    use crate::config::Config;
    use crate::models::{AIModel, Generation, InferenceBackend};
    use crate::services::{AIService, CacheService};

    struct RecordingBackend {
        config: AiConfig,
        complexity: TaskComplexity,
        tiny_defers: bool,
        finish_reason: String,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl InferenceBackend for RecordingBackend {
        fn evaluate_task(&mut self, _: &str, _: &str) -> TaskComplexity {
            self.calls.lock().unwrap().push("evaluate".into());
            self.complexity
        }

        fn generate(
            &mut self,
            _: &str,
            _: &str,
            _: f32,
            _: usize,
            requested: Option<&str>,
        ) -> anyhow::Result<Generation> {
            let direct_qwen = requested == Some(self.config.fallback_model_name.as_str());
            self.calls
                .lock()
                .unwrap()
                .push(if direct_qwen { "qwen" } else { "tiny" }.into());
            let qwen = direct_qwen || self.tiny_defers;
            Ok(Generation {
                text: if qwen { "Qwen answer" } else { "Tiny answer" }.into(),
                model: if qwen {
                    self.config.fallback_model_name.clone()
                } else {
                    self.config.model_name.clone()
                },
                fallback_used: !direct_qwen && qwen,
                finish_reason: self.finish_reason.clone(),
            })
        }
    }

    async fn routing_state(
        complexity: TaskComplexity,
        tiny_defers: bool,
        finish_reason: &str,
    ) -> (AppState, Arc<Mutex<Vec<String>>>) {
        let mut config = Config::default();
        config.cache.redis_url.clear();
        config.cache.sqlite_path.clear();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let backend = Arc::new(RwLock::new(RecordingBackend {
            config: config.ai.clone(),
            complexity,
            tiny_defers,
            finish_reason: finish_reason.into(),
            calls: calls.clone(),
        }));
        let ai_model = Arc::new(RwLock::new(AIModel::new(config.ai.clone())));
        let state = AppState {
            ai_service: AIService::new(backend, config.ai.clone()),
            ai_model,
            models_ready: Arc::new(AtomicBool::new(false)),
            cache_service: CacheService::new(config.cache.clone()).await.unwrap(),
            config,
            start_time: Instant::now(),
        };
        (state, calls)
    }

    async fn request(state: &AppState, body: serde_json::Value) -> actix_web::dev::ServiceResponse {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .route("/api/chat", web::post().to(chat)),
        )
        .await;
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/chat")
                .set_json(body)
                .to_request(),
        )
        .await
    }

    async fn json_response(state: &AppState, body: serde_json::Value) -> ChatResponse {
        let response = request(state, body).await;
        assert_eq!(response.status(), StatusCode::OK);
        test::read_body_json(response).await
    }

    #[actix_web::test]
    async fn simple_auto_misses_generate_with_tiny_and_repeats_use_its_cache() {
        let (state, calls) = routing_state(TaskComplexity::Simple, false, "stop").await;
        let first = json_response(&state, serde_json::json!({"message": "hi"})).await;
        assert_eq!(first.model, state.config.ai.model_name);
        assert!(!first.cache_hit);
        assert!(!first.fallback_used);
        let repeat = json_response(
            &state,
            serde_json::json!({"message": "hi", "model": "auto"}),
        )
        .await;
        assert!(repeat.cache_hit);
        assert_eq!(repeat.response, first.response);
        assert_eq!(*calls.lock().unwrap(), ["evaluate", "tiny", "evaluate"]);

        let qwen = json_response(
            &state,
            serde_json::json!({"message": "hi", "model": "Qwen"}),
        )
        .await;
        assert!(!qwen.cache_hit); // A direct Qwen request must never reuse TinyLlama's answer.
        assert_eq!(qwen.model, state.config.ai.fallback_model_name);
        assert!(!qwen.fallback_used);
        assert_eq!(
            *calls.lock().unwrap(),
            ["evaluate", "tiny", "evaluate", "qwen"]
        );
    }

    #[actix_web::test]
    async fn complex_auto_uses_qwen_and_shares_cache_with_direct_qwen() {
        let (state, calls) = routing_state(TaskComplexity::Complex, false, "stop").await;
        let auto = json_response(&state, serde_json::json!({"message": "analyze"})).await;
        assert_eq!(auto.model, state.config.ai.fallback_model_name);
        assert!(auto.fallback_used);
        assert!(!auto.cache_hit);
        for model in ["qwen", "Qwen", &state.config.ai.fallback_model_name] {
            let direct = json_response(
                &state,
                serde_json::json!({"message": "analyze", "model": model}),
            )
            .await;
            assert!(direct.cache_hit);
            assert!(!direct.fallback_used);
        }
        assert_eq!(*calls.lock().unwrap(), ["evaluate", "qwen"]);
    }

    #[actix_web::test]
    async fn simple_auto_can_reuse_direct_qwen_answer() {
        let (state, calls) = routing_state(TaskComplexity::Simple, false, "stop").await;
        json_response(
            &state,
            serde_json::json!({"message": "hi", "model": "qwen"}),
        )
        .await;
        let auto = json_response(&state, serde_json::json!({"message": "hi"})).await;
        assert!(auto.cache_hit);
        assert!(auto.fallback_used);
        assert_eq!(auto.model, state.config.ai.fallback_model_name);
        assert_eq!(*calls.lock().unwrap(), ["qwen", "evaluate"]);
    }

    #[actix_web::test]
    async fn complex_tasks_ignore_cached_tiny_answers() {
        let (simple, _) = routing_state(TaskComplexity::Simple, false, "stop").await;
        json_response(&simple, serde_json::json!({"message": "hi"})).await;
        let (mut complex, calls) = routing_state(TaskComplexity::Complex, false, "stop").await;
        complex.cache_service = simple.cache_service;
        let response = json_response(&complex, serde_json::json!({"message": "hi"})).await;
        assert!(!response.cache_hit);
        assert_eq!(response.model, complex.config.ai.fallback_model_name);
        assert_eq!(*calls.lock().unwrap(), ["evaluate", "qwen"]);
    }

    #[actix_web::test]
    async fn deferred_tiny_answers_are_cached_as_qwen() {
        let (state, calls) = routing_state(TaskComplexity::Simple, true, "stop").await;
        let response = json_response(&state, serde_json::json!({"message": "hi"})).await;
        assert_eq!(response.model, state.config.ai.fallback_model_name);
        assert!(response.fallback_used);
        let response = json_response(
            &state,
            serde_json::json!({"message": "hi", "model": "qwen"}),
        )
        .await;
        assert!(response.cache_hit);
        assert!(!response.fallback_used);
        assert_eq!(*calls.lock().unwrap(), ["evaluate", "tiny"]);
    }

    #[actix_web::test]
    async fn cache_bypass_skips_reads_and_writes_and_truncation_is_not_cached() {
        let (state, calls) = routing_state(TaskComplexity::Simple, false, "stop").await;
        json_response(
            &state,
            serde_json::json!({"message": "hi", "cache_bypass": true}),
        )
        .await;
        let uncached = json_response(&state, serde_json::json!({"message": "hi"})).await;
        assert!(!uncached.cache_hit); // The bypassed request did not write.
        let bypassed = json_response(
            &state,
            serde_json::json!({"message": "hi", "cache_bypass": true}),
        )
        .await;
        assert!(!bypassed.cache_hit); // The bypassed request did not read.
        assert_eq!(
            *calls.lock().unwrap(),
            ["evaluate", "tiny", "evaluate", "tiny", "evaluate", "tiny"]
        );
        let (state, calls) = routing_state(TaskComplexity::Complex, false, "length").await;
        for _ in 0..2 {
            let response = json_response(&state, serde_json::json!({"message": "long"})).await;
            assert!(!response.cache_hit);
        }
        assert_eq!(
            *calls.lock().unwrap(),
            ["evaluate", "qwen", "evaluate", "qwen"]
        );
    }

    #[actix_web::test]
    async fn invalid_models_do_not_run_the_classifier_or_generation() {
        let (state, calls) = routing_state(TaskComplexity::Simple, false, "stop").await;
        let response = request(
            &state,
            serde_json::json!({"message": "hi", "model": "unknown"}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(calls.lock().unwrap().is_empty());
    }

    #[actix_web::test]
    async fn cached_ndjson_and_plain_text_use_the_same_routing() {
        let (state, calls) = routing_state(TaskComplexity::Simple, false, "stop").await;
        json_response(&state, serde_json::json!({"message": "hi"})).await;
        let response = request(&state, serde_json::json!({"message": "hi", "stream": true})).await;
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/x-ndjson"
        );
        let body = test::read_body(response).await;
        let records: Vec<serde_json::Value> = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let done = records.last().unwrap();
        assert_eq!(done["cache_hit"], true);
        assert_eq!(done["model"], state.config.ai.model_name);
        assert_eq!(done["fallback_used"], false);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .route("/api/chat", web::post().to(chat)),
        )
        .await;
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/chat")
                .insert_header(("Accept", "text/plain"))
                .set_json(serde_json::json!({"message": "hi"}))
                .to_request(),
        )
        .await;
        assert_eq!(test::read_body(response).await, "Tiny answer");
        assert_eq!(
            *calls.lock().unwrap(),
            ["evaluate", "tiny", "evaluate", "evaluate"]
        );
    }

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
                &chat_cache_key(&req, &config.ai, &config.ai.fallback_model_name),
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
            chat_cache_key(&req, &config.ai, &config.ai.fallback_model_name),
            chat_cache_key(&req, &changed_default, &config.ai.fallback_model_name)
        );
    }
}
