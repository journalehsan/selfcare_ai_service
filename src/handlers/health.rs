use actix_web::{web, HttpResponse, Result};
use std::sync::atomic::Ordering;

use crate::models::{ErrorResponse, HealthResponse};
use crate::AppState;

pub async fn health_check(state: web::Data<AppState>) -> Result<HttpResponse> {
    let uptime = state.start_time.elapsed().as_secs();
    let model_loaded = state.models_ready.load(Ordering::Acquire);

    let response = HealthResponse {
        status: if model_loaded {
            "healthy"
        } else {
            "initializing"
        }
        .to_string(),
        model_loaded,
        uptime_seconds: uptime,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    Ok(HttpResponse::Ok().json(response))
}

pub async fn ready_check(state: web::Data<AppState>) -> Result<HttpResponse> {
    let model_loaded = state.models_ready.load(Ordering::Acquire);

    if model_loaded {
        Ok(HttpResponse::Ok().json(HealthResponse {
            status: "ready".to_string(),
            model_loaded: true,
            uptime_seconds: state.start_time.elapsed().as_secs(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(ErrorResponse::new(
            "Local models are not ready; check model paths and service logs",
        )))
    }
}

pub async fn not_found() -> Result<HttpResponse> {
    Ok(HttpResponse::NotFound().json(ErrorResponse::new("Endpoint not found")))
}
