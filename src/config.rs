use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub ai: AiConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub max_json_payload_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub model_name: String,
    pub model_path: Option<String>,
    pub huggingface_cache_dir: Option<String>,
    pub context_length: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: usize,
    pub quantized: bool,
    pub quantization_bits: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub rate_limit_requests: u32,
    pub rate_limit_period: u64,
    pub allowed_origins: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 5732,
                workers: num_cpus::get(),
                max_json_payload_size: 2_000_000, // 2MB
            },
            ai: AiConfig {
                model_name: "mistralai/Mistral-7B-Instruct-v0.2".to_string(),
                model_path: None,
                huggingface_cache_dir: None,
                context_length: 4096,
                temperature: 0.7,
                top_p: 0.9,
                max_tokens: 2048,
                quantized: true,
                quantization_bits: Some(4),
            },
            security: SecurityConfig {
                rate_limit_requests: 100,
                rate_limit_period: 3600,
                allowed_origins: vec!["*".to_string()],
            },
        }
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenv::dotenv().ok();

        let mut config = Config::default();

        // Server configuration
        if let Ok(host) = env::var("HOST") {
            config.server.host = host;
        }
        if let Ok(port) = env::var("PORT") {
            config.server.port = port.parse()?;
        }
        if let Ok(workers) = env::var("WORKERS") {
            config.server.workers = workers.parse()?;
        }
        if let Ok(max_json_payload_size) = env::var("MAX_JSON_PAYLOAD_SIZE") {
            config.server.max_json_payload_size = max_json_payload_size.parse()?;
        }

        // AI configuration
        if let Ok(model_name) = env::var("MODEL_NAME") {
            config.ai.model_name = model_name;
        }
        if let Ok(model_path) = env::var("MODEL_PATH") {
            config.ai.model_path = Some(model_path);
        }
        if let Ok(huggingface_cache_dir) = env::var("HUGGINGFACE_CACHE_DIR") {
            config.ai.huggingface_cache_dir = Some(huggingface_cache_dir);
        }
        if let Ok(context_length) = env::var("CONTEXT_LENGTH") {
            config.ai.context_length = context_length.parse()?;
        }
        if let Ok(temperature) = env::var("TEMPERATURE") {
            config.ai.temperature = temperature.parse()?;
        }
        if let Ok(top_p) = env::var("TOP_P") {
            config.ai.top_p = top_p.parse()?;
        }
        if let Ok(max_tokens) = env::var("MAX_TOKENS") {
            config.ai.max_tokens = max_tokens.parse()?;
        }
        if let Ok(quantized) = env::var("QUANTIZED") {
            config.ai.quantized = quantized.parse()?;
        }
        if let Ok(quantization_bits) = env::var("QUANTIZATION_BITS") {
            config.ai.quantization_bits = Some(quantization_bits.parse()?);
        }

        // Security configuration
        if let Ok(rate_limit_requests) = env::var("RATE_LIMIT_REQUESTS") {
            config.security.rate_limit_requests = rate_limit_requests.parse()?;
        }
        if let Ok(rate_limit_period) = env::var("RATE_LIMIT_PERIOD") {
            config.security.rate_limit_period = rate_limit_period.parse()?;
        }
        if let Ok(allowed_origins) = env::var("ALLOWED_ORIGINS") {
            config.security.allowed_origins = allowed_origins
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }

        Ok(config)
    }
}
