mod config;
mod handlers;
mod middleware;
mod models;
mod routes;
mod utils;

use actix_cors::Cors;
use actix_web::{middleware::Logger, web, App, HttpServer};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use handlers::health::not_found;
use models::AIModel;
use routes::api;

#[derive(Clone)]
pub struct AppState {
    pub ai_model: Arc<RwLock<AIModel>>,
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
    let ai_model = AIModel::new(config.ai.clone());
    let state = AppState {
        ai_model: Arc::new(RwLock::new(ai_model)),
        config: config.clone(),
        start_time: Instant::now(),
    };

    // Start model loading in background
    let model_loader = state.ai_model.clone();
    tokio::spawn(async move {
        info!("Starting background model loading...");
        if let Err(e) = model_loader.write().await.load_model().await {
            error!("Failed to load AI model: {}", e);
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
