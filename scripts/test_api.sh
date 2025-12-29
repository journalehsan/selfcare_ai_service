#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:5732}"
API_URL="$BASE_URL/api"
STREAM_CHAT="${STREAM_CHAT:-1}"

log() {
    printf '[%s] %s\n' "$(date +'%Y-%m-%dT%H:%M:%S')" "$*"
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        log "Missing required command: $1"
        exit 1
    fi
}

pretty_print() {
    if command -v jq >/dev/null 2>&1; then
        jq .
    else
        cat
    fi
}

require_cmd curl

curl_json() {
    local method="$1"
    local path="$2"
    local payload="$3"
    curl -sS -f -X "$method" "$API_URL$path" \
        -H 'Content-Type: application/json' \
        -d "$payload"
}

curl_ndjson() {
    local payload="$1"
    curl -sS -f -X POST "$API_URL/chat" \
        -H 'Content-Type: application/json' \
        -H 'Accept: application/x-ndjson' \
        -d "$payload"
}

log "GET /api/health"
curl -sS -f "$API_URL/health" | pretty_print
printf '\n'

log "GET /api/ready"
curl -sS -f "$API_URL/ready" | pretty_print
printf '\n'

log "POST /api/chat"
curl_json POST /chat '{
    "message": "Hello! tell me a joke about computers.",
    "conversation_id": null
}' | pretty_print
printf '\n'

if [[ "$STREAM_CHAT" == "1" ]]; then
    log "POST /api/chat (streaming NDJSON)"
    curl_ndjson '{
        "message": "Stream a short answer about what a lunar eclipse is.",
        "conversation_id": null,
        "stream": true
    }'
    printf '\n'
fi

log "POST /api/analyze-logs"
curl_json POST /analyze-logs '{
    "logs": "2024-01-01 ERROR disk full on /var\n2024-01-01 WARN high memory usage",
    "context": "production"
}' | pretty_print
printf '\n'

log "POST /api/generate-script"
curl_json POST /generate-script '{
    "requirement": "Check disk usage and clean temp files",
    "type": "linux",
    "language": "bash"
}' | pretty_print
printf '\n'

log "All requests completed."
