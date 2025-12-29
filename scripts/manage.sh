#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$PROJECT_ROOT/target"
LOGS_DIR="$PROJECT_ROOT/logs"
DATA_DIR="$PROJECT_ROOT/data"
MODELS_DIR="$PROJECT_ROOT/models"
PID_FILE="$TARGET_DIR/selfcare_ai_service.pid"
SQLITE_PATH="${SQLITE_PATH:-$DATA_DIR/ai_cache.sqlite}"

MODEL_REPO="${MODEL_REPO:-TinyLlama/TinyLlama-1.1B-Chat-v1.0}"
MODEL_DIR="${MODEL_DIR:-$MODELS_DIR/shared/tinyllama-1.1b}"
HF_CACHE_DIR="${HF_CACHE_DIR:-$PROJECT_ROOT/.cache/huggingface}"
HF_VENV_DIR="${HF_VENV_DIR:-$PROJECT_ROOT/.venv/hf_cli}"
HF_CMD="${HF_CMD:-}"
REDIS_CONTAINER="${REDIS_CONTAINER:-selfcare_redis}"
REDIS_IMAGE="${REDIS_IMAGE:-redis:7-alpine}"
REDIS_PORT="${REDIS_PORT:-6379}"
REDIS_MAX_MEMORY_MB="${REDIS_MAX_MEMORY_MB:-2048}"
REDIS_URL="${REDIS_URL:-redis://127.0.0.1:6379}"

mkdir -p "$TARGET_DIR" "$LOGS_DIR" "$MODEL_DIR" "$HF_CACHE_DIR"
mkdir -p "$DATA_DIR"
mkdir -p "$MODELS_DIR/orchestrator" "$MODELS_DIR/reviewer" "$MODELS_DIR/embeddings" "$MODELS_DIR/shared"

log() {
    printf '[%s] %s\n' "$(date +'%Y-%m-%dT%H:%M:%S')" "$*"
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        log "Missing required command: $1"
        exit 1
    fi
}

start_redis() {
    if ! command -v docker >/dev/null 2>&1; then
        log "Docker not found; skipping Redis startup."
        return
    fi

    if docker ps --format '{{.Names}}' | grep -qx "$REDIS_CONTAINER"; then
        log "Redis container already running: $REDIS_CONTAINER"
        return
    fi

    if docker ps -a --format '{{.Names}}' | grep -qx "$REDIS_CONTAINER"; then
        log "Starting existing Redis container: $REDIS_CONTAINER"
        docker start "$REDIS_CONTAINER" >/dev/null
        return
    fi

    log "Starting Redis container: $REDIS_CONTAINER"
    docker run -d \
        --name "$REDIS_CONTAINER" \
        -p "$REDIS_PORT:6379" \
        "$REDIS_IMAGE" \
        redis-server --maxmemory "${REDIS_MAX_MEMORY_MB}mb" --maxmemory-policy allkeys-lru >/dev/null
}

stop_redis() {
    if ! command -v docker >/dev/null 2>&1; then
        return
    fi

    if docker ps --format '{{.Names}}' | grep -qx "$REDIS_CONTAINER"; then
        log "Stopping Redis container: $REDIS_CONTAINER"
        docker stop "$REDIS_CONTAINER" >/dev/null
    fi
}

cache_clear() {
    if [[ -f "$SQLITE_PATH" ]]; then
        log "Removing SQLite cache at $SQLITE_PATH"
        rm -f "$SQLITE_PATH"
    else
        log "SQLite cache not found at $SQLITE_PATH"
    fi

    if command -v docker >/dev/null 2>&1; then
        if docker ps --format '{{.Names}}' | grep -qx "$REDIS_CONTAINER"; then
            log "Flushing Redis cache in container: $REDIS_CONTAINER"
            docker exec "$REDIS_CONTAINER" redis-cli FLUSHALL >/dev/null
            return
        fi
    fi

    if command -v redis-cli >/dev/null 2>&1; then
        log "Flushing Redis cache via redis-cli"
        redis-cli -u "$REDIS_URL" FLUSHALL >/dev/null
    else
        log "redis-cli not found; skipping Redis flush."
    fi
}

install_huggingface_cli() {
    if command -v huggingface-cli >/dev/null 2>&1; then
        HF_CMD="$(command -v huggingface-cli)"
        return
    fi
    if command -v hf >/dev/null 2>&1; then
        HF_CMD="$(command -v hf)"
        return
    fi

    if command -v pipx >/dev/null 2>&1; then
        log "Installing huggingface-cli via pipx."
        pipx install huggingface-hub >/dev/null 2>&1 || pipx install --force huggingface-hub >/dev/null 2>&1 || true
        if command -v huggingface-cli >/dev/null 2>&1; then
            HF_CMD="$(command -v huggingface-cli)"
            return
        fi
        if command -v hf >/dev/null 2>&1; then
            HF_CMD="$(command -v hf)"
            return
        fi
        log "pipx installation did not make 'hf' or 'huggingface-cli' available."
    fi

    require_cmd python3
    if [[ ! -d "$HF_VENV_DIR" ]]; then
        log "Creating virtualenv for huggingface-cli at $HF_VENV_DIR"
        mkdir -p "$(dirname "$HF_VENV_DIR")"
        python3 -m venv "$HF_VENV_DIR"
    fi

    log "Installing huggingface_hub inside the virtualenv."
    "$HF_VENV_DIR/bin/python" -m pip install --upgrade pip >/dev/null
    if ! "$HF_VENV_DIR/bin/python" -m pip install --upgrade huggingface-hub >/dev/null; then
        log "Failed to install huggingface_hub in virtualenv. Install it manually and re-run setup."
        exit 1
    fi

    export PATH="$HF_VENV_DIR/bin:$PATH"
    hash -r || true

    if command -v huggingface-cli >/dev/null 2>&1; then
        HF_CMD="$(command -v huggingface-cli)"
        return
    fi
    if command -v hf >/dev/null 2>&1; then
        HF_CMD="$(command -v hf)"
        return
    fi

    log "Neither 'hf' nor 'huggingface-cli' is available after installation."
    log "Virtualenv bin directory listing:"
    ls -la "$HF_VENV_DIR/bin" || true
    exit 1
}

download_model() {
    local repo_id="${1:-$MODEL_REPO}"
    local target_dir="${2:-$MODEL_DIR}"

    if [[ -n "$(ls -A "$target_dir" 2>/dev/null)" ]]; then
        local index_file="$target_dir/model.safetensors.index.json"
        local direct_file="$target_dir/model.safetensors"
        local base_ok=false
        if [[ -f "$target_dir/config.json" ]] && [[ -f "$target_dir/tokenizer.json" ]]; then
            base_ok=true
        fi

        local weights_ok=false
        if [[ -f "$direct_file" ]]; then
            weights_ok=true
        elif [[ -f "$index_file" ]]; then
            require_cmd python3
            if python3 - <<'PY' "$index_file" "$target_dir"
import json
import os
import sys

index_path = sys.argv[1]
root_dir = sys.argv[2]
with open(index_path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

weight_map = data.get("weight_map", {})
files = set(weight_map.values())
missing = [f for f in files if not os.path.exists(os.path.join(root_dir, f))]
if missing:
    print("Missing safetensors shards:", ", ".join(sorted(missing)))
    sys.exit(1)
sys.exit(0)
PY
            then
                weights_ok=true
            fi
        fi

        if [[ "$base_ok" == "true" && "$weights_ok" == "true" ]]; then
            log "Model already present at $target_dir"
            return
        fi

        log "Model directory is incomplete; re-downloading to $target_dir"
        rm -rf "$target_dir"
        mkdir -p "$target_dir"
    fi

    install_huggingface_cli

    log "Downloading $repo_id into $target_dir"
    if [[ -z "${HF_CMD}" ]]; then
        log "HF_CMD was not resolved; cannot download model."
        exit 1
    fi

    local hf_basename
    hf_basename="$(basename "$HF_CMD")"

    if [[ "$hf_basename" == "hf" ]]; then
        if ! "$HF_CMD" download \
            "$repo_id" \
            --repo-type model \
            --cache-dir "$HF_CACHE_DIR" \
            --local-dir "$target_dir" \
            --include '*' \
            --exclude 'pytorch_model*' \
            --exclude '*.bin'; then
            log "Model download failed. Ensure you have set the HUGGING_FACE_HUB_TOKEN if required."
            exit 1
        fi
    else
        if ! HF_HOME="$HF_CACHE_DIR" "$HF_CMD" download \
            "$repo_id" \
            --local-dir "$target_dir" \
            --local-dir-use-symlinks False; then
            log "Model download failed. Ensure you have set the HUGGING_FACE_HUB_TOKEN if required."
            exit 1
        fi
    fi

    if [[ -z "$(ls -A "$target_dir" 2>/dev/null)" ]]; then
        log "Model download completed but $target_dir is empty."
        exit 1
    fi

    log "Model downloaded successfully."
}

build_project() {
    require_cmd cargo
    log "Fetching dependencies..."
    cargo fetch --locked
    log "Building release binary..."
    cargo build --release
}

setup() {
    build_project
    download_model "$MODEL_REPO" "$MODEL_DIR"
    log "Setup complete. Configure MODEL_PATH=$MODEL_DIR if you override defaults."
}

start() {
    if [[ -f "$PID_FILE" ]]; then
        if kill -0 "$(cat "$PID_FILE")" >/dev/null 2>&1; then
            log "Service already running with PID $(cat "$PID_FILE")"
            exit 0
        else
            rm -f "$PID_FILE"
        fi
    fi

    require_cmd cargo
    start_redis
    log "Starting SelfCare AI Service..."
    (
        cd "$PROJECT_ROOT"
        export MODEL_PATH="$MODEL_DIR"
        export HUGGINGFACE_CACHE_DIR="$HF_CACHE_DIR"
        nohup cargo run --release >"$LOGS_DIR/service.log" 2>&1 &
        echo $! >"$PID_FILE"
    )
    log "Service started with PID $(cat "$PID_FILE"). Logs: $LOGS_DIR/service.log"
}

stop() {
    stop_redis
    if [[ ! -f "$PID_FILE" ]]; then
        log "No PID file found; service is not running?"
        return
    fi

    local pid
    pid="$(cat "$PID_FILE")"
    if ! kill -0 "$pid" >/dev/null 2>&1; then
        log "Process $pid is not running. Cleaning up stale PID file."
        rm -f "$PID_FILE"
        return
    fi

    log "Stopping service with PID $pid"
    kill "$pid"
    for _ in {1..20}; do
        if kill -0 "$pid" >/dev/null 2>&1; then
            sleep 0.5
        else
            break
        fi
    done
    if kill -0 "$pid" >/dev/null 2>&1; then
        log "Process did not exit gracefully; sending SIGKILL"
        kill -9 "$pid" || true
    fi
    rm -f "$PID_FILE"
    log "Service stopped."
}

restart() {
    stop
    start
}

usage() {
    cat <<EOF
Usage: $(basename "$0") <command>

Commands:
  setup    Fetch dependencies, build the binary, and download the TinyLlama model
  start    Start the service in the background (logs written to $LOGS_DIR/service.log)
  stop     Stop the background service
  restart  Restart the background service
  status   Display whether the service is running
  cache-clear  Clear Redis + SQLite caches
  help     Show this message
EOF
}

status() {
    if [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" >/dev/null 2>&1; then
        log "Service is running with PID $(cat "$PID_FILE")"
    else
        log "Service is not running."
    fi
}

cmd="${1:-help}"
case "$cmd" in
    setup) setup ;;
    start) start ;;
    stop) stop ;;
    restart) restart ;;
    status) status ;;
    cache-clear) cache_clear ;;
    help|--help|-h) usage ;;
    *)
        log "Unknown command: $cmd"
        usage
        exit 1
        ;;
esac
