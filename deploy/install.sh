#!/usr/bin/env bash
# Runs on the offline VM. No downloads, registry pulls or source builds.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
BUNDLE="$(pwd -P)"
if [[ "$(id -u)" != 0 ]]; then
    echo 'Run with sudo bash ./install.sh' >&2
    exit 1
fi
[[ "$(uname -m)" == x86_64 ]] || { echo 'Linux amd64 required.' >&2; exit 1; }
sha256sum --check --quiet SHA256SUMS
if ! command -v docker >/dev/null; then
    . /etc/os-release
    host_os_id="$ID"
    host_os_version="$VERSION_ID"
    . ./packages/target.env
    if [[ "$host_os_id" != "$ID" || "$host_os_version" != "$VERSION_ID" || "$(dpkg --print-architecture)" != "$ARCH" ]]; then
        echo "Bundled Docker installer targets $ID $VERSION_ID $ARCH; this VM is $host_os_id $host_os_version. Prepare matching packages first." >&2
        exit 1
    fi
    # dpkg never contacts a repository. Preserve already installed versions
    # that are newer than the bundled dependency instead of downgrading them.
    packages=()
    for file in "$BUNDLE"/packages/*.deb; do
        package="$(dpkg-deb --field "$file" Package)"
        version="$(dpkg-deb --field "$file" Version)"
        status="$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)"
        installed="$(dpkg-query -W -f='${Version}' "$package" 2>/dev/null || true)"
        if [[ "$status" == 'install ok installed' ]] && dpkg --compare-versions "$installed" ge "$version"; then
            continue
        fi
        packages+=("$file")
    done
    if (( ${#packages[@]} )); then
        dpkg --install "${packages[@]}"
    fi
    systemctl enable --now docker
fi
docker info >/dev/null
. ./images.env
docker load -i images/services.tar.gz
[[ "$(docker image inspect --format '{{.Id}}' "$SERVICE_IMAGE")" == "$SERVICE_IMAGE_ID" ]]
[[ "$(docker image inspect --format '{{.Id}}' "$REDIS_IMAGE")" == "$REDIS_IMAGE_ID" ]]
STACK_NAME="${STACK_NAME:-selfcare-ai}"
[[ "$STACK_NAME" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo 'Invalid STACK_NAME' >&2; exit 1; }
NETWORK="$STACK_NAME-network"
SERVICE="$STACK_NAME"
REDIS="$STACK_NAME-redis"
DATA_VOLUME="$STACK_NAME-data"
REDIS_VOLUME="$STACK_NAME-redis-data"
for container in "$SERVICE" "$REDIS"; do
    if docker container inspect "$container" >/dev/null 2>&1; then
        [[ "$(docker inspect --format '{{index .Config.Labels "com.selfcare.offline"}}' "$container")" == true ]] || {
            echo "Refusing to replace unrelated container $container" >&2; exit 1;
        }
    fi
done
if docker network inspect "$NETWORK" >/dev/null 2>&1; then
    [[ "$(docker network inspect --format '{{index .Labels "com.selfcare.offline"}}' "$NETWORK")" == true ]] || {
        echo "Refusing to reuse unrelated network $NETWORK" >&2; exit 1;
    }
    [[ "$(docker network inspect --format '{{.Internal}}' "$NETWORK")" == false ]] || {
        echo "Existing network $NETWORK blocks the published API port; refusing to use it." >&2; exit 1;
    }
else
    docker network create --label com.selfcare.offline=true "$NETWORK" >/dev/null
fi
for volume in "$DATA_VOLUME" "$REDIS_VOLUME"; do
    if docker volume inspect "$volume" >/dev/null 2>&1; then
        [[ "$(docker volume inspect --format '{{index .Labels "com.selfcare.offline"}}' "$volume")" == true ]] || {
            echo "Refusing to reuse unrelated volume $volume" >&2; exit 1;
        }
    else
        docker volume create --label com.selfcare.offline=true "$volume" >/dev/null
    fi
done
# Remove only containers owned by this installer. Named data volumes survive.
for container in "$SERVICE" "$REDIS"; do
    if docker container inspect "$container" >/dev/null 2>&1; then
        docker stop --time 30 "$container" >/dev/null
        docker rm "$container" >/dev/null
    fi
done
docker run -d --pull never --name "$REDIS" --label com.selfcare.offline=true \
    --restart unless-stopped --network "$NETWORK" --network-alias redis \
    --mount "type=volume,src=$REDIS_VOLUME,dst=/data" \
    --log-opt max-size=10m --log-opt max-file=3 \
    "$REDIS_IMAGE" redis-server --appendonly yes --maxmemory 512mb --maxmemory-policy allkeys-lru >/dev/null
redis_ready=false
for ((attempt=0; attempt<30; attempt++)); do
    if [[ "$(docker exec "$REDIS" redis-cli ping 2>/dev/null || true)" == PONG ]]; then
        redis_ready=true
        break
    fi
    sleep 1
done
[[ "$redis_ready" == true ]] || { docker logs --tail 40 "$REDIS"; exit 1; }
docker run -d --pull never --name "$SERVICE" --label com.selfcare.offline=true \
    --restart unless-stopped --network "$NETWORK" \
    -p "${BIND_ADDRESS:-0.0.0.0}:${SERVICE_PORT:-5732}:5732" \
    --mount "type=bind,src=$BUNDLE/models,dst=/models,readonly" \
    --mount "type=volume,src=$DATA_VOLUME,dst=/app/data" \
    --security-opt no-new-privileges --cap-drop ALL --read-only \
    --tmpfs /tmp:rw,noexec,nosuid,size=64m \
    --log-opt max-size=10m --log-opt max-file=3 \
    -e MODEL_PATH=/models/shared/tinyllama-1.1b \
    -e FALLBACK_MODEL_PATH=/models/fallback/qwen2.5-7b-instruct/Qwen2.5-7B-Instruct-Q4_K_M.gguf \
    -e FALLBACK_TOKENIZER_PATH=/models/fallback/qwen2.5-7b-instruct/tokenizer.json \
    -e REDIS_URL=redis://redis:6379 -e SQLITE_PATH=/app/data/ai_cache.sqlite \
    "$SERVICE_IMAGE" >/dev/null
echo 'Waiting for local models to load...'
deadline=$((SECONDS + ${READY_TIMEOUT:-180}))
while (( SECONDS < deadline )); do
    if docker exec "$SERVICE" /usr/bin/curl --fail --silent --max-time 4 http://127.0.0.1:5732/api/ready; then
        printf '\nReady on port %s. Models and caches run offline.\n' "${SERVICE_PORT:-5732}"
        exit 0
    fi
    if [[ "$(docker inspect --format '{{.State.Running}}' "$SERVICE")" != true ]]; then
        break
    fi
    sleep 3
done
echo 'Service did not become ready. Recent logs:' >&2
docker logs --tail 80 "$SERVICE" >&2
exit 1
