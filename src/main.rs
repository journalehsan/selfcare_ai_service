mod config;
mod handlers;
mod middleware;
mod models;
mod repositories;
mod routes;
mod services;
mod utils;

use actix_cors::Cors;
use actix_web::{middleware::Logger, web, App, HttpServer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::{CacheSettings, Config};
use handlers::health::not_found;
use models::AIModel;
use routes::api;
use services::{AIService, CacheService};

#[derive(Clone)]
pub struct AppState {
    pub ai_model: Arc<RwLock<AIModel>>,
    pub models_ready: Arc<AtomicBool>,
    pub ai_service: AIService,
    pub cache_service: CacheService,
    pub config: Config,
    pub start_time: Instant,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = match Config::from_env() {
        Ok(config) => {
            info!("Configuration loaded successfully");
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    info!(
        "Starting SelfCare AI Service on port {}",
        config.server.port
    );

    // Initialize AI model
    let ai_model = Arc::new(RwLock::new(AIModel::new(config.ai.clone())));
    let cache_service = match CacheService::new(config.cache.clone()).await {
        Ok(service) => service,
        Err(e) => {
            error!("Failed to initialize cache service: {}", e);
            let fallback = CacheSettings {
                redis_url: "".to_string(),
                sqlite_path: "".to_string(),
                ..config.cache.clone()
            };
            CacheService::new(fallback).await.expect("cache service")
        }
    };
    let ai_service = AIService::new(ai_model.clone(), config.ai.clone());
    let models_ready = Arc::new(AtomicBool::new(false));

    let state = AppState {
        ai_model: ai_model.clone(),
        models_ready: models_ready.clone(),
        ai_service,
        cache_service,
        config: config.clone(),
        start_time: Instant::now(),
    };

    // Start model loading in background
    let model_loader = state.ai_model.clone();
    tokio::task::spawn_blocking(move || {
        info!("Loading local TinyLlama and Qwen fallback from disk...");
        match model_loader.blocking_write().load_models() {
            Ok(()) => models_ready.store(true, Ordering::Release),
            Err(e) => error!("Failed to load local models: {e:#}"),
        }
    });

    // Create HTTP server
    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .app_data(web::Data::new(state.clone()))
            .app_data(web::JsonConfig::default().limit(state.config.server.max_json_payload_size))
            .wrap(cors)
            .wrap(Logger::default())
            .service(api::config())
            .default_service(web::route().to(not_found))
    })
    .bind(format!("{}:{}", config.server.host, config.server.port))?;

    info!(
        "Server started successfully at http://{}:{}",
        config.server.host, config.server.port
    );

    // Run the server
    server.workers(config.server.workers).run().await
}
