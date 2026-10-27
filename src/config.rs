use serde::{Deserialize, Serialize};
use std::env;

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are a helpful assistant. Answer the user's request directly and concisely.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub ai: AiConfig,
    pub security: SecurityConfig,
    pub cache: CacheSettings,
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
    pub system_prompt: String,
    pub model_name: String,
    pub model_path: String,
    pub fallback_model_name: String,
    pub fallback_model_path: String,
    pub fallback_tokenizer_path: String,
    pub context_length: usize,
    pub fallback_context_length: usize,
    pub tiny_max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: usize,
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
                system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
                model_name: "TinyLlama/TinyLlama-1.1B-Chat-v1.0".to_string(),
                model_path: "models/shared/tinyllama-1.1b".to_string(),
                fallback_model_name: "Qwen2.5-7B-Instruct-Q4_K_M".to_string(),
                fallback_model_path:
                    "models/fallback/qwen2.5-7b-instruct/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
                        .to_string(),
                fallback_tokenizer_path: "models/fallback/qwen2.5-7b-instruct/tokenizer.json"
                    .to_string(),
                context_length: 2048,
                fallback_context_length: 4096,
                tiny_max_tokens: 256,
                temperature: 0.7,
                top_p: 0.9,
                max_tokens: 512,
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
                cache_probability: 1.0,
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
        if let Ok(system_prompt) = env::var("SYSTEM_PROMPT") {
            config.ai.system_prompt = system_prompt;
        }
        if let Ok(model_name) = env::var("MODEL_NAME") {
            config.ai.model_name = model_name;
        }
        if let Ok(model_path) = env::var("MODEL_PATH") {
            if !model_path.trim().is_empty() {
                config.ai.model_path = model_path;
            }
        }
        if let Ok(name) = env::var("FALLBACK_MODEL_NAME") {
            config.ai.fallback_model_name = name;
        }
        if let Ok(path) = env::var("FALLBACK_MODEL_PATH") {
            config.ai.fallback_model_path = path;
        }
        if let Ok(path) = env::var("FALLBACK_TOKENIZER_PATH") {
            config.ai.fallback_tokenizer_path = path;
        }
        if let Ok(context_length) = env::var("CONTEXT_LENGTH") {
            config.ai.context_length = context_length.parse()?;
        }
        if let Ok(context_length) = env::var("FALLBACK_CONTEXT_LENGTH") {
            config.ai.fallback_context_length = context_length.parse()?;
        }
        if let Ok(max_tokens) = env::var("TINY_MAX_TOKENS") {
            config.ai.tiny_max_tokens = max_tokens.parse()?;
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

        anyhow::ensure!(config.ai.context_length > 1, "CONTEXT_LENGTH must exceed 1");
        anyhow::ensure!(
            config.ai.fallback_context_length > 1,
            "FALLBACK_CONTEXT_LENGTH must exceed 1"
        );
        anyhow::ensure!(
            config.ai.tiny_max_tokens > 0,
            "TINY_MAX_TOKENS must be positive"
        );
        anyhow::ensure!(config.ai.max_tokens > 0, "MAX_TOKENS must be positive");
        anyhow::ensure!(
            config.ai.temperature.is_finite() && config.ai.temperature >= 0.0,
            "TEMPERATURE must be finite and nonnegative"
        );
        anyhow::ensure!(
            config.ai.top_p > 0.0 && config.ai.top_p <= 1.0,
            "TOP_P must be in (0, 1]"
        );
        anyhow::ensure!(
            (0.0..=1.0).contains(&config.cache.cache_probability),
            "CACHE_PROBABILITY must be in [0, 1]"
        );

        Ok(config)
    }
}
