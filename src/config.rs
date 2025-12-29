use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub ai: AiConfig,
    pub security: SecurityConfig,
    pub cache: CacheSettings,
    pub openrouter: OpenRouterSettings,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSettings {
    pub redis_url: String,
    pub redis_max_memory_mb: u64,
    pub redis_ttl_seconds: u64,
    pub sqlite_path: String,
    pub sqlite_max_size_gb: u64,
    pub sqlite_ttl_days: u32,
    pub similarity_threshold: f32,
    pub max_similar_results: usize,
    pub memory_cache_entries: usize,
    pub memory_ttl_seconds: u64,
    pub cache_probability: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterSettings {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
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
                model_name: "TinyLlama/TinyLlama-1.1B-Chat-v1.0".to_string(),
                model_path: None,
                huggingface_cache_dir: None,
                context_length: 2048,
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
            cache: CacheSettings {
                redis_url: "redis://127.0.0.1:6379".to_string(),
                redis_max_memory_mb: 2048,
                redis_ttl_seconds: 86_400,
                sqlite_path: "data/ai_cache.sqlite".to_string(),
                sqlite_max_size_gb: 10,
                sqlite_ttl_days: 30,
                similarity_threshold: 0.92,
                max_similar_results: 3,
                memory_cache_entries: 512,
                memory_ttl_seconds: 3_600,
                cache_probability: 0.3,
            },
            openrouter: OpenRouterSettings {
                api_key: "".to_string(),
                base_url: "https://openrouter.ai/api/v1".to_string(),
                default_model: "openrouter/auto".to_string(),
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

        // Cache configuration
        if let Ok(redis_url) = env::var("REDIS_URL") {
            config.cache.redis_url = redis_url;
        }
        if let Ok(redis_max_memory_mb) = env::var("REDIS_MAX_MEMORY_MB") {
            config.cache.redis_max_memory_mb = redis_max_memory_mb.parse()?;
        }
        if let Ok(redis_ttl_seconds) = env::var("REDIS_TTL_SECONDS") {
            config.cache.redis_ttl_seconds = redis_ttl_seconds.parse()?;
        }
        if let Ok(sqlite_path) = env::var("SQLITE_PATH") {
            config.cache.sqlite_path = sqlite_path;
        }
        if let Ok(sqlite_max_size_gb) = env::var("SQLITE_MAX_SIZE_GB") {
            config.cache.sqlite_max_size_gb = sqlite_max_size_gb.parse()?;
        }
        if let Ok(sqlite_ttl_days) = env::var("SQLITE_TTL_DAYS") {
            config.cache.sqlite_ttl_days = sqlite_ttl_days.parse()?;
        }
        if let Ok(similarity_threshold) = env::var("SIMILARITY_THRESHOLD") {
            config.cache.similarity_threshold = similarity_threshold.parse()?;
        }
        if let Ok(max_similar_results) = env::var("MAX_SIMILAR_RESULTS") {
            config.cache.max_similar_results = max_similar_results.parse()?;
        }
        if let Ok(memory_cache_entries) = env::var("MEMORY_CACHE_ENTRIES") {
            config.cache.memory_cache_entries = memory_cache_entries.parse()?;
        }
        if let Ok(memory_ttl_seconds) = env::var("MEMORY_TTL_SECONDS") {
            config.cache.memory_ttl_seconds = memory_ttl_seconds.parse()?;
        }
        if let Ok(cache_probability) = env::var("CACHE_PROBABILITY") {
            config.cache.cache_probability = cache_probability.parse()?;
        }

        // OpenRouter configuration
        if let Ok(api_key) = env::var("OPENROUTER_API_KEY") {
            config.openrouter.api_key = api_key;
        }
        if let Ok(base_url) = env::var("OPENROUTER_BASE_URL") {
            config.openrouter.base_url = base_url;
        }
        if let Ok(default_model) = env::var("OPENROUTER_DEFAULT_MODEL") {
            config.openrouter.default_model = default_model;
        }

        Ok(config)
    }
}
