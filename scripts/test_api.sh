#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:5732}"
API_URL="$BASE_URL/api"

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

log "GET /api/health"
curl -sS "$API_URL/health" | pretty_print
printf '\n'

log "GET /api/ready"
curl -sS "$API_URL/ready" | pretty_print
printf '\n'

log "POST /api/chat"
curl -sS -X POST "$API_URL/chat" \
    -H 'Content-Type: application/json' \
    -d '{
        "message": "Hello! tell me a joke about computers.",
        "conversation_id": null
    }' | pretty_print
printf '\n'

log "POST /api/analyze-logs"
curl -sS -X POST "$API_URL/analyze-logs" \
    -H 'Content-Type: application/json' \
    -d '{
        "logs": "2024-01-01 ERROR disk full on /var\n2024-01-01 WARN high memory usage",
        "context": "production"
    }' | pretty_print
printf '\n'

log "POST /api/generate-script"
curl -sS -X POST "$API_URL/generate-script" \
    -H 'Content-Type: application/json' \
    -d '{
        "requirement": "Check disk usage and clean temp files",
        "type": "linux",
        "language": "bash"
    }' | pretty_print
printf '\n'

log "All requests completed."
