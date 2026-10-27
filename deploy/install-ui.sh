#!/usr/bin/env bash
# Run in the UI bundle on the VM. The existing AI/Redis containers stay running.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
[[ "$(id -u)" == 0 ]] || { echo 'Run with sudo bash ./install-ui.sh' >&2; exit 1; }
sha256sum --check --quiet SHA256SUMS
UI_PORT="${UI_PORT:-8080}"
[[ "$UI_PORT" =~ ^[0-9]+$ ]] && (( UI_PORT > 0 && UI_PORT < 65536 )) || { echo 'Invalid UI_PORT' >&2; exit 1; }
[[ "$(docker inspect --format '{{.State.Running}}' selfcare-ai)" == true ]]
[[ "$(docker network inspect --format '{{index .Labels "com.selfcare.offline"}}' selfcare-ai-network)" == true ]]
docker exec selfcare-ai /usr/bin/curl --fail --silent --max-time 5 http://127.0.0.1:5732/api/ready >/dev/null
. ./ui.env
docker load -i ui-image.tar.gz
[[ "$(docker image inspect --format '{{.Id}}' "$UI_IMAGE")" == "$UI_IMAGE_ID" ]]
docker run --rm --pull never --network selfcare-ai-network --read-only --tmpfs /tmp:rw,noexec,nosuid,size=32m --cap-drop ALL --security-opt no-new-privileges "$UI_IMAGE" -t
if docker container inspect selfcare-ai-ui >/dev/null 2>&1; then
    [[ "$(docker inspect --format '{{index .Config.Labels "com.selfcare.component"}}' selfcare-ai-ui)" == ui ]] || { echo 'Refusing to replace unrelated selfcare-ai-ui container' >&2; exit 1; }
    docker stop --time 15 selfcare-ai-ui >/dev/null
    docker rm selfcare-ai-ui >/dev/null
fi
docker run -d --pull never --name selfcare-ai-ui --label com.selfcare.offline=true \
    --label com.selfcare.component=ui --restart unless-stopped --network selfcare-ai-network \
    -p "${BIND_ADDRESS:-0.0.0.0}:$UI_PORT:8080" --read-only --tmpfs /tmp:rw,noexec,nosuid,size=32m \
    --cap-drop ALL --security-opt no-new-privileges \
    --log-opt max-size=10m --log-opt max-file=3 "$UI_IMAGE" >/dev/null
for ((attempt=0; attempt<20; attempt++)); do
    if docker exec selfcare-ai-ui wget -q -O /dev/null http://127.0.0.1:8080/api/ready; then
        echo "Chat UI ready on port $UI_PORT. API requests use the local proxy."
        exit 0
    fi
    sleep 1
done
docker logs --tail 50 selfcare-ai-ui >&2
exit 1
