# SelfCare AI Service

A general-purpose offline AI gateway built with Rust and Actix Web, with optional log analysis and script generation endpoints. Requests use local caches, then TinyLlama, with a local Qwen2.5-7B-Instruct Q4_K_M fallback through Candle. Runtime inference does not use OpenRouter, web search, or Hugging Face downloads.

## Features

- **AI Chat Interface**: TinyLlama first, with a local quantized Qwen fallback
- **Caching**: Memory, Redis, and SQLite caches before chat inference
- **Log Analysis**: Analyze system logs and identify issues
- **Script Generation**: Generate troubleshooting scripts based on user requirements
- **RESTful API**: Clean REST API endpoints for integration
- **High Performance**: Built with Rust and Actix Web for optimal performance

## Architecture

- **Web Framework**: Actix Web
- **Primary Model**: TinyLlama-1.1B-Chat-v1.0 safetensors, CPU F32 via Candle
- **Fallback Model**: Qwen2.5-7B-Instruct Q4_K_M GGUF, CPU via Candle
- **Port**: 5732
- **Language**: Rust (2021 Edition)

## API Endpoints

### Chat
```
POST /api/chat
Content-Type: application/json

{
  "message": "Your message here",
  "system_prompt": "You are a concise translation assistant. Translate user text into Persian.",
  "conversation_id": null,
  "model": "auto"
}
```

`system_prompt` is optional (up to 100,000 characters). Omit it or send `null`
to use the global `SYSTEM_PROMPT` default. A supplied string replaces that default
for both TinyLlama and Qwen; `""` omits the application system prompt. TinyLlama
still receives its internal fallback instruction. Chat has no built-in SelfCare
identity or technical-support role. The log and script endpoints retain their
task-specific instructions.

### Log Analysis
```
POST /api/analyze-logs
Content-Type: application/json

{
  "logs": "Your log content here",
  "context": "optional-context-information"
}
```

### Script Generation
```
POST /api/generate-script
Content-Type: application/json

{
  "requirement": "Describe what the script should do",
  "environment": "linux|windows|macos",
  "language": "bash|python|powershell"
}
```

## Getting Started

### Prerequisites

- A Rust toolchain compatible with Candle 0.9.1 on the connected build machine
- Docker for the optional local Redis container; prepare its image before deployment
- Local model files and a release binary compatible with the VM's OS, CPU, and system libraries

### Installation

For a complete Docker deployment to the offline VM, prepare artifacts on the
connected laptop with `./scripts/prepare-offline.sh`. This caches the models,
builds the service in Docker, saves service/Redis images, and includes Ubuntu
26.04 Docker installer packages. After reconnecting to the office network, run
`./scripts/deploy-offline.sh` to transfer and install on
`localadmin@10.227.36.102`. Passwords are prompted interactively. See
[the offline deployment guide](deploy/README.md) for prerequisites and controls.
The bundle is stored in the ignored `offline-bundle/` directory.
Docker build files and Cargo downloads are cached under `.cache/offline-build/`
for later preparation runs.

For service code changes after installation, run
`./scripts/deploy-offline.sh --update-service`. It builds offline from the cache
and transfers only the new service image. Existing model files, Redis and data
volumes are reused; startup failure restores the previous service. See the
[deployment guide](deploy/README.md) for separate preparation and deployment commands.

1. Clone the repository:
```bash
git clone https://github.com/your-username/selfcare_ai_service.git
cd selfcare_ai_service
```

2. Prepare the release binary, both models, and Redis image on a connected machine:
```bash
./scripts/manage.sh setup
docker save -o redis-7-alpine.tar redis:7-alpine
```

3. Copy the project, including `target/release/selfcare_ai_service`, `models/`, and the saved Redis image, to the VM. Build on a machine with the same architecture and compatible system libraries. The offline VM does not need Cargo or the Hugging Face CLI.

4. On the VM, load Redis, configure the service, and start the prebuilt binary:
```bash
docker load -i redis-7-alpine.tar
cp .env.example .env
./scripts/manage.sh start
```

The service listens on `http://localhost:5732`. Model loading runs in a background blocking worker. `/api/ready` returns 503 until Qwen is loaded; if TinyLlama fails to load, requests can still use Qwen. Loading errors are written to `logs/service.log`. Startup never compiles, downloads model files, or pulls a Docker image. `setup` is an online preparation command; do not run it on the disconnected VM.

The offline Docker bundle has been built and tested locally, including real
TinyLlama and Qwen replies and installation of the Docker packages without
networking. Deployment validation on the target VM remains pending.

### Configuration

The service can be configured through environment variables:

- `PORT`: Server port (default: 5732)
- `SYSTEM_PROMPT`: Global default instruction (default: `You are a helpful assistant. Answer the user's request directly and concisely.`); chat requests can override it
- `MODEL_PATH`: TinyLlama directory containing `config.json`, `tokenizer.json`, and safetensors
- `CONTEXT_LENGTH`: TinyLlama total token limit, capped at the model's native limit (default: 2048)
- `TINY_MAX_TOKENS`: TinyLlama answer budget before falling back (default: 256)
- `FALLBACK_MODEL_PATH`: Path to a single merged Qwen2.5 Q4_K_M `.gguf` file
- `FALLBACK_TOKENIZER_PATH`: Path to the Qwen2.5-7B-Instruct `tokenizer.json`
- `FALLBACK_CONTEXT_LENGTH`: Qwen total token limit, capped at the GGUF limit (default: 4096)
- `MAX_TOKENS`: Maximum generated answer tokens (default: 512)
- `RUST_LOG`: Logging level (info, debug, warn, error)

The binary reads `.env` from the working directory. The management script uses shell environment variables for its own paths and passes those paths to the binary; export path overrides when using the script. `MODEL_DIR` is the script's alias for `MODEL_PATH`. `SERVICE_BIN` overrides the release binary location. `HF_CACHE_DIR`, `MODEL_REPO`, and `FALLBACK_MODEL_REPO` configure online preparation only. Legacy `OPENROUTER_*`, `QUANTIZED`, and `QUANTIZATION_BITS` settings are no longer used; the GGUF file contains the fallback's quantization.

## Usage Examples

### Chat with AI
```bash
curl -X POST http://localhost:5732/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "message": "Help me troubleshoot a slow database connection"
  }'
```

### Analyze Logs
```bash
curl -X POST http://localhost:5732/api/analyze-logs \
  -H "Content-Type: application/json" \
  -d '{
    "logs": "ERROR: Connection timeout after 30 seconds\nWARNING: High memory usage detected",
    "context": "PostgreSQL database server"
  }'
```

### Generate Script
```bash
curl -X POST http://localhost:5732/api/generate-script \
  -H "Content-Type: application/json" \
  -d '{
    "requirement": "Check disk space and clean temporary files",
    "environment": "linux",
    "language": "bash"
  }'
```

## Development

### Running Tests
```bash
cargo test
```

### Development Mode
```bash
cargo run
```

### Code Formatting
```bash
cargo fmt
```

### Linting
```bash
cargo clippy
```

## Service Management Script

Use `scripts/manage.sh` for common lifecycle tasks:

```bash
# Online preparation only (build + download TinyLlama and Qwen + pull Redis)
./scripts/manage.sh setup

# Start/stop/restart the background service
./scripts/manage.sh start
./scripts/manage.sh status
./scripts/manage.sh stop
./scripts/manage.sh restart
```

The management script publishes its Redis container on host port `6380` by default, so it can run alongside Redis on the standard port `6379`. It passes the matching `REDIS_URL` to the service. To use another host port, run `REDIS_PORT=6381 ./scripts/manage.sh start`; an explicitly exported `REDIS_URL` takes precedence. These overrides are shell environment variables. Direct `cargo run` uses `REDIS_URL` from the environment or `.env`, falling back to port `6379`.

An existing Redis container keeps its original port mapping. If it differs from `REDIS_PORT`, the script reports the mismatch. Use the existing port or choose a new container name, for example `REDIS_CONTAINER=selfcare_redis_6380 ./scripts/manage.sh start`.

During `setup`, the script uses Hugging Face's CLI (`hf` or `huggingface-cli`) to prepare:

```text
models/shared/tinyllama-1.1b/
  config.json
  tokenizer.json
  model.safetensors (or index + every shard)
models/fallback/qwen2.5-7b-instruct/
  Qwen2.5-7B-Instruct-Q4_K_M.gguf
  tokenizer.json
```

TinyLlama comes from `TinyLlama/TinyLlama-1.1B-Chat-v1.0`. The single-file fallback comes from [bartowski/Qwen2.5-7B-Instruct-GGUF](https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/blob/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf); its tokenizer comes from `Qwen/Qwen2.5-7B-Instruct`. Candle's loader expects all tensors in one GGUF: split GGUF downloads must be merged before deployment. Preserve model licenses with the deployment artifacts.

If the CLI is absent, setup tries `pipx`, then a project-local virtual environment in `.venv/hf_cli`. None of these tools are used by `start`.

## Chat workspace

`index.html` is an Alpine.js chat interface with responsive navigation, tab-local
conversation history, sanitized Markdown, copy/export, retry, and connection
settings. It defaults to the office API at `http://10.227.36.102:5732`; when served
from that VM's UI port it uses the same-origin API proxy. Alpine and all document
readers are vendored under `assets/vendor/`; there are no CDN, font or analytics
requests at runtime. The user-provided `alpine.min.js.js` matches the vendored
Alpine file; the HTML loads only `assets/vendor/alpine.min.js`.

Preview the public files without exposing the repository or models:

```bash
python3 scripts/serve-ui.py --port 8000
```

Open `http://localhost:8000`. Connection settings can target another service
origin for this tab. No chat text or settings are written to browser storage;
refreshing or closing the tab clears them. Server-side caching still applies.

Use **Chat** for questions and follow-ups, or **Summarize** for long pasted text.
Long text is automatically summarized when it exceeds a section's byte budget;
the composer indicates this before sending. Summary requests use Qwen directly,
process every section sequentially, split again on explicit model context errors,
and combine the resulting notes. CPU inference can take several minutes per
section; progress counts completed sections, not generated tokens. Summaries are lossy: check the original for exact
details. Progress and response-limit notices are shown. Up to 200,000 characters
or 600 KB of extracted UTF-8 text can be processed per request. No input is silently
truncated to satisfy the model context limit. Follow-ups send bounded recent
exchanges because the backend conversation ID does not store history; document
follow-ups use its summary rather than resending the full original.

Optional attachments support TXT, Markdown, LOG, CSV, JSON, DOCX and text-based
PDFs. Files are read in the browser and only extracted text goes to the API.
One attachment at a time, up to 10 MB; PDFs are limited to 150 pages. Word images,
formatting and embedded content are omitted, and PDF table/reading order can be
imperfect. Scanned documents require OCR elsewhere; password-protected PDFs,
legacy `.doc` files and non-UTF-8/UTF-16 text files need conversion first.

To deploy the UI using Docker already installed on the office VM:

```bash
./scripts/prepare-ui.sh   # uses the cached nginx:alpine image; no downloads
./scripts/deploy-ui.sh    # SSH/sudo prompts; open http://10.227.36.102:8080
```

The small separate `offline-ui-bundle/` contains the UI image, verified browser
asset manifest and installer. Its container joins `selfcare-ai-network` and proxies
`/api/` to the existing `selfcare-ai:5732`. AI and Redis stay running. The UI
installer supports `UI_PORT` and `BIND_ADDRESS`. The UI uses no external inference
or document-upload service. Stop ends the browser's wait; server inference may
continue until its current request completes.

Browser dependencies are pinned with licenses and SHA-256 hashes in
`assets/vendor/manifest.json`. `python3 scripts/offline/vendor_ui.py` fills missing
files online and verifies existing files against the manifest. Updating a version
requires reviewing the corresponding manifest entries as well as the script.
Meaningful browser checks use a mocked API and real local document readers:

```bash
# With the preview running and playwright-core installed for development:
PLAYWRIGHT_MODULE=/path/to/playwright-core node scripts/tests/ui-smoke.cjs
```

These checks cover context splitting without losing the document tail, follow-up
context, TXT/DOCX/PDF extraction, safe Markdown, errors/retry/stop, export, mobile
navigation, and offline assets. They do not measure model quality or latency.

## Model Information

Chat requests first check memory, Redis, and SQLite caches. Cache lookup is enabled for every request by default (`CACHE_PROBABILITY=1.0`); `cache_bypass=true` skips reads and writes. Cache storage and eviction are ordinary Rust code, not model inference.

On a miss, TinyLlama tries to answer. Qwen takes over if TinyLlama is unavailable, errors, cannot fit the prompt, returns an empty answer or `[USE_FALLBACK]`, or reaches its answer budget without an EOS token. This explicit deferral mechanism does not measure answer confidence. The fallback receives the original task and uses its own Qwen chat template and tokenizer. Inputs exceeding Qwen's configured context are rejected rather than silently truncated.

Chat `model` accepts `auto` (default), the configured `MODEL_NAME` (TinyLlama with fallback), or `FALLBACK_MODEL_NAME` (direct Qwen). Other model names return 400. JSON responses and final NDJSON records include the actual `model`, `fallback_used`, and `finish_reason`. Responses ending with `finish_reason=length` are not cached. Cache keys include the effective system prompt, user message, model configuration, and generation settings, so different use cases do not reuse each other's answers. The new cache namespace excludes replies generated with the old hardcoded support prompt without deleting stored data. Conversation IDs are correlation identifiers; the service does not currently retain multi-turn conversation history. NDJSON output is chunked after generation, not live token inference.

Log analysis and script generation use the same local TinyLlama-to-Qwen pipeline. Model loading and inference run on blocking workers with serialized model access. Each request starts with fresh KV attention state. The quantized loader follows [Candle's Qwen GGUF implementation](https://github.com/huggingface/candle/blob/0.9.1/candle-transformers/src/models/quantized_qwen2.rs).

## Performance Considerations

- Both models stay resident. TinyLlama is CPU F32; Q4_K_M applies to Qwen's GGUF tensors. Some Qwen tensors are dequantized by Candle.
- Budget RAM for both models, attention state, Redis, and the OS. A 24 GB VM is a starting sizing estimate, not a measured requirement; validate memory and latency after deployment.
- The current build uses CPU inference; it does not require CUDA, Metal, or a separate inference server.
- Generation is serialized to bound concurrent attention-state memory. Latency, throughput, and answer quality have not been benchmarked for this migration.

## Security

- Input validation and sanitization
- Rate limiting on API endpoints
- Chat cache entries persist in SQLite and Redis when configured
- Secure model loading from trusted sources

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Support

For issues and questions:
- Create an issue on GitHub
- Check the documentation
- Review the API examples

## Roadmap

- [x] Browser chat workspace
- [ ] Conversation history persistence
- [ ] Multiple model support
- [ ] Plugin system for custom integrations
- [x] Docker containerization
- [ ] Kubernetes deployment manifests
