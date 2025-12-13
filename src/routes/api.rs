use crate::handlers;
use actix_web::{web, Scope};

pub fn config() -> Scope {
    web::scope("/api")
        .route("/health", web::get().to(handlers::health_check))
        .route("/ready", web::get().to(handlers::ready_check))
        .route("/chat", web::post().to(handlers::chat))
        .route("/analyze-logs", web::post().to(handlers::analyze_logs))
        .route(
            "/generate-script",
            web::post().to(handlers::generate_script),
        )
}
