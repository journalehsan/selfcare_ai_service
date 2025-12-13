use actix_web::{web, HttpRequest, HttpResponse, Result};
use chrono::Utc;
use uuid::Uuid;
use validator::Validate;

use crate::models::{ChatRequest, ChatResponse, ErrorResponse};
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

    // Get mutable reference to AI model
    let mut ai_model = state.ai_model.write().await;

    // Process the chat request
    match ai_model
        .chat(&req.message, Some(conversation_id.to_string()))
        .await
    {
        Ok(response) => {
            let accept = http_req
                .headers()
                .get(actix_web::http::header::ACCEPT)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if accept.contains("text/plain") {
                return Ok(HttpResponse::Ok()
                    .content_type("text/plain; charset=utf-8")
                    .body(response));
            }

            let chat_response = ChatResponse {
                response,
                conversation_id,
                timestamp: Utc::now(),
            };
            Ok(HttpResponse::Ok().json(chat_response))
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
