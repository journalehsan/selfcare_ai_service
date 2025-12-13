# SelfCare AI Service

A Rust-based AI service built with Actix Web that provides intelligent assistance for troubleshooting, log analysis, and script generation using the Mistral 7B model.

## Features

- **AI Chat Interface**: Interactive chat with Mistral 7B model
- **Log Analysis**: Analyze system logs and identify issues
- **Script Generation**: Generate troubleshooting scripts based on user requirements
- **RESTful API**: Clean REST API endpoints for integration
- **High Performance**: Built with Rust and Actix Web for optimal performance

## Architecture

- **Web Framework**: Actix Web
- **AI Model**: Mistral 7B via Candle and Hugging Face
- **Port**: 5732
- **Language**: Rust (2021 Edition)

## API Endpoints

### Chat
```
POST /api/chat
Content-Type: application/json

{
  "message": "Your message here",
  "conversation_id": "optional-conversation-id"
}
```

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

- Rust 1.70+ 
- Git

### Installation

1. Clone the repository:
```bash
git clone https://github.com/your-username/selfcare_ai_service.git
cd selfcare_ai_service
```

2. Build the project:
```bash
cargo build --release
```

3. Run the service:
```bash
cargo run --release
```

The service will start on `http://localhost:5732`

### Configuration

The service can be configured through environment variables:

- `PORT`: Server port (default: 5732)
- `MODEL_PATH`: Path to Mistral 7B model files
- `HUGGINGFACE_CACHE_DIR`: Cache directory for model downloads
- `LOG_LEVEL`: Logging level (info, debug, warn, error)

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
# Prepare the project (build + download the Mistral model)
./scripts/manage.sh setup

# Start/stop/restart the background service
./scripts/manage.sh start
./scripts/manage.sh status
./scripts/manage.sh stop
./scripts/manage.sh restart
```

The script downloads `mistralai/Mistral-7B-Instruct-v0.2` into `models/` using Hugging Face’s CLI (`hf` or `huggingface-cli`) and prefers the `.safetensors` weights. It first looks for a system-installed CLI, otherwise tries installing via `pipx`, and finally falls back to a project-local virtual environment in `.venv/hf_cli`. Ensure you have a valid Hugging Face token configured (via `hf auth login` / `huggingface-cli login` or `HUGGING_FACE_HUB_TOKEN`) before running `setup`.

## API Showcase UI

A lightweight `index.html` built with Alpine.js and Tailwind CSS is included in the repository root. It provides simple forms to call the `/api/chat`, `/api/analyze-logs`, and `/api/generate-script` endpoints.

To preview it locally:

```bash
# Run the API (cargo run --release or ./scripts/manage.sh start), then
python3 -m http.server 8000
```

Navigate to `http://localhost:8000/index.html` and update the “Base URL” field if your API is hosted elsewhere.

## Model Information

This service uses the Mistral 7B model, a powerful open-source language model optimized for instruction following and reasoning tasks. The model is loaded using the Candle framework for efficient inference.

## Performance Considerations

- Model loading may take 30-60 seconds on first startup
- Memory usage: ~8-16GB RAM recommended for optimal performance
- GPU acceleration supported when available
- Response time: 2-10 seconds depending on input complexity

## Security

- Input validation and sanitization
- Rate limiting on API endpoints
- No user data persistence by default
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

- [ ] Web dashboard interface
- [ ] Conversation history persistence
- [ ] Multiple model support
- [ ] Plugin system for custom integrations
- [ ] Docker containerization
- [ ] Kubernetes deployment manifests
