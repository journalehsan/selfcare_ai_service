# AI Orchestrator Microservice Transformation Plan

## Project Overview

Transform the existing SelfCare AI Service into a full-featured AI Orchestrator Microservice that mimics Ollama's API with intelligent caching and routing capabilities.

## Current State Analysis

### Existing Infrastructure ✅
- **Framework**: Actix Web 4.x
- **Local Model**: Mistral 7B via Candle
- **Port**: 5732
- **Language**: Rust 2021 Edition
- **Basic API**: `/api/chat`, `/api/analyze-logs`, `/api/generate-script`

### Missing Components ❌
- Redis caching layer
- SQLite persistent cache
- OpenRouter cloud model integration
- Ollama API compatibility
- Intelligent query routing

## Implementation Phases

### Phase 1: Foundation & Configuration (Week 1)

#### Day 1-2: Enhanced Configuration System
- [ ] Extend `src/config.rs` with cache and OpenRouter settings
- [ ] Create `CacheSettings` struct with Redis and SQLite configuration
- [ ] Create `OpenRouterSettings` struct with API key and model configuration
- [ ] Update `.env.example` with new environment variables
- [ ] Maintain backward compatibility with existing config

#### Day 3-4: Database & Repository Layer
- [ ] Create `src/repositories/cache_repo.rs` with SQLite operations
- [ ] Create `src/repositories/redis_repo.rs` with Redis connection management
- [ ] Implement database schema with auto-cleanup logic
- [ ] Create indexes for optimal cache performance
- [ ] Implement cache key generation utilities

#### Day 5-7: Core Services Foundation
- [ ] Build `src/services/cache_service.rs` with three-layer caching
- [ ] Create `src/services/ai_service.rs` for model orchestration
- [ ] Implement `src/utils/hashing.rs` for cache key generation
- [ ] Create `src/utils/ranking.rs` for similarity search
- [ ] Implement LRU cache for hot access patterns

### Phase 2: AI Orchestration & API Enhancement (Week 2)

#### Day 8-10: Model Routing Logic
- [ ] Implement query complexity analysis (Low/Medium/High)
- [ ] Create model selection based on query complexity
- [ ] Add web search integration for Medium/High complexity queries
- [ ] Implement response adaptation for similar cached results
- [ ] Create cloud model routing logic

#### Day 11-14: Ollama API Compatibility
- [ ] Build Ollama-compatible request/response models
- [ ] Create new endpoint handlers in `src/handlers/`
- [ ] Maintain backward compatibility with existing `/api/chat`
- [ ] Implement `/api/generate` endpoint (Ollama compatible)
- [ ] Implement `/api/embeddings` endpoint
- [ ] Implement `/api/tags` endpoint (list available models)
- [ ] Implement `/api/version` endpoint

### Phase 3: Performance & Integration (Week 3)

#### Day 15-17: Caching & Optimization
- [ ] Implement auto-cleanup for SQLite database (10GB limit)
- [ ] Add connection pooling for Redis and SQLite
- [ ] Optimize cache hit rates with LRU and similarity search
- [ ] Implement background cleanup tasks
- [ ] Add cache statistics and monitoring

#### Day 18-21: Monitoring & Statistics
- [ ] Create comprehensive statistics tracking
- [ ] Implement `/api/stats` endpoint
- [ ] Add performance metrics and monitoring
- [ ] Create cache hit rate reporting
- [ ] Implement cost tracking for cloud models

### Phase 4: Deployment & Polish (Week 4)

#### Day 22-24: Docker & Deployment
- [ ] Create `docker-compose.yml` for development
- [ ] Update `scripts/manage.sh` with cache setup
- [ ] Add health checks and monitoring
- [ ] Create production deployment configuration
- [ ] Add environment-specific configurations

#### Day 25-28: Testing & Documentation
- [ ] Write comprehensive tests for all components
- [ ] Create API documentation and examples
- [ ] Performance testing and optimization
- [ ] Integration testing with real models
- [ ] Create migration guide for existing users

## Technical Architecture

### Enhanced Configuration Structure
```rust
pub struct CacheSettings {
    pub redis_url: String,
    pub redis_max_memory_mb: u64,
    pub redis_ttl_seconds: u64,
    pub sqlite_path: String,
    pub sqlite_max_size_gb: u64,
    pub sqlite_ttl_days: u32,
    pub similarity_threshold: f32,
    pub max_similar_results: usize,
}

pub struct OpenRouterSettings {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}
```

### Three-Layer Cache Architecture
1. **Memory Cache (Hot)**: LRU cache for recent requests (< 1 minute)
2. **Redis Cache (Warm)**: Fast access for recent requests (< 1 hour)
3. **SQLite Cache (Cold)**: Persistent storage for older requests (1 hour - 30 days)

### AI Orchestration Logic
```rust
async fn orchestrate(prompt: &str, context: &Context) -> Response {
    // 1. Check three-layer cache
    if let Some(cached) = cache_service.get(&cache_key).await {
        return cached;
    }
    
    // 2. Analyze query complexity
    let complexity = analyze_complexity(prompt).await;
    
    // 3. Route based on complexity
    let response = match complexity {
        Complexity::Low => local_model.generate(prompt).await,
        Complexity::Medium => {
            let search_results = web_search(prompt).await;
            let enriched = enrich_prompt(prompt, &search_results);
            local_model.generate(&enriched).await
        }
        Complexity::High => {
            let search_results = web_search(prompt).await;
            let enriched = enrich_prompt(prompt, &search_results);
            let model = select_cloud_model(&complexity);
            cloud_model.generate(&model, &enriched).await
        }
    };
    
    // 4. Cache the response
    cache_service.set(&cache_key, &response).await?;
    
    response
}
```

## New API Endpoints

### Ollama-Compatible Endpoints
- `POST /api/generate` - Generate completion (like Ollama)
- `POST /api/chat` - Chat completion (enhanced)
- `POST /api/embeddings` - Generate embeddings
- `GET /api/tags` - List available models
- `GET /api/version` - Service version

### Custom Endpoints
- `POST /api/analyze` - Analyze query complexity
- `GET /api/stats` - Cache and performance statistics
- `POST /api/search` - Web search with ranking

### Existing Endpoints (Maintained)
- `POST /api/chat` - Backward compatible
- `POST /api/analyze-logs` - Enhanced with caching
- `POST /api/generate-script` - Enhanced with caching

## Performance Targets

### Cache Performance
- **Hit Rate**: >70% after warmup
- **Memory Usage**: <2GB for service + models
- **Response Time**: <100ms for cache hits, <2s for cloud requests
- **Throughput**: 1000+ requests/second on 4-core machine

### Database Performance
- **SQLite Size**: Auto-cleanup at 10GB
- **Redis Memory**: 2GB with LRU eviction
- **Cache Key**: `md5(prompt + model_name + parameters)`
- **TTL**: Redis=1h, SQLite=30d

## Risk Mitigation

### Redis Dependency
- **Fallback**: Graceful degradation to SQLite-only mode
- **Monitoring**: Health checks and automatic recovery
- **Configuration**: Optional Redis setup with clear error messages

### Model Loading
- **Background Loading**: Non-blocking startup
- **Caching**: Pre-warmed models for faster response
- **Fallback**: Graceful error handling for model failures

### Cache Corruption
- **Validation**: Automatic cache key validation
- **Cleanup**: Regular cleanup tasks for invalid entries
- **Backup**: SQLite backup and restore capabilities

### API Breaking Changes
- **Versioning**: Clear version headers for API changes
- **Migration**: Step-by-step migration guide
- **Compatibility**: Maintain existing endpoints during transition

## Success Metrics

### Functional Metrics
- [ ] All Ollama API endpoints working with caching
- [ ] Backward compatibility maintained for existing API
- [ ] Intelligent routing based on query complexity
- [ ] Web search integration working

### Performance Metrics
- [ ] 70%+ cache hit rate within 1 hour of operation
- [ ] Response time <100ms for cache hits
- [ ] Throughput >1000 requests/second
- [ ] Memory usage <2GB total

### Cost Metrics
- [ ] 60-80% reduction in cloud API costs
- [ ] Automatic cost optimization through intelligent routing
- [ ] Detailed cost tracking and reporting

### User Experience Metrics
- [ ] Zero downtime during deployment
- [ ] Clear migration path for existing users
- [ ] Comprehensive documentation and examples
- [ ] Performance monitoring and alerting

## Dependencies & Requirements

### New Dependencies
- `redis` - Redis client for caching
- `sqlx` - Async SQLite operations
- `lru` - LRU cache implementation
- `dashmap` - Concurrent hash map for memory cache
- `reqwest` - HTTP client for OpenRouter integration
- `serde_json` - JSON serialization for cache

### System Requirements
- **Memory**: 8-16GB RAM (model + cache)
- **Storage**: 10GB for SQLite cache + model files
- **Network**: Access to OpenRouter API and search services
- **Redis**: Optional but recommended for production

This transformation plan provides a comprehensive roadmap for converting the SelfCare AI Service into a full-featured AI Orchestrator Microservice while maintaining backward compatibility and adding powerful caching and routing capabilities.