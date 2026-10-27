#!/usr/bin/env bash
# Build and export the chat UI from a cached nginx image. No network downloads.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_BUNDLE="${UI_BUNDLE_DIR:-$ROOT/offline-ui-bundle}"
UI_IMAGE="${UI_IMAGE:-selfcare-ai-ui:offline}"
NGINX_IMAGE="${NGINX_IMAGE:-nginx:alpine}"
mkdir -p "$ROOT/.cache" "$UI_BUNDLE"
UI_BUNDLE="$(cd "$UI_BUNDLE" && pwd)"
CONTEXT="$(mktemp -d "$ROOT/.cache/ui-build.XXXXXX")"
trap 'rm -rf -- "$CONTEXT"' EXIT
python3 - "$ROOT" <<'PY'
import hashlib, json, sys
from pathlib import Path
root = Path(sys.argv[1]) / 'assets/vendor'
manifest = json.loads((root / 'manifest.json').read_text())
for name, record in manifest.items():
    assert hashlib.sha256((root/name).read_bytes()).hexdigest() == record['sha256'], name
print('Browser asset integrity verified.')
PY
BASE_IMAGE_ID="$(docker image inspect --format '{{.Id}}' "$NGINX_IMAGE")"
cp "$ROOT/index.html" "$ROOT/deploy/nginx-ui.conf" "$CONTEXT/"
cp -R "$ROOT/assets" "$CONTEXT/assets"
docker build --pull=false --network none --build-arg "NGINX_IMAGE=$BASE_IMAGE_ID" \
    -f "$ROOT/deploy/Dockerfile.ui" -t "$UI_IMAGE" "$CONTEXT"
docker run --rm --pull never --network none --add-host selfcare-ai:127.0.0.1 \
    --read-only --tmpfs /tmp:rw,noexec,nosuid,size=32m --cap-drop ALL \
    --security-opt no-new-privileges "$UI_IMAGE" -t
docker save "$UI_IMAGE" | gzip -1 > "$UI_BUNDLE/ui-image.tar.gz.partial"
mv "$UI_BUNDLE/ui-image.tar.gz.partial" "$UI_BUNDLE/ui-image.tar.gz"
printf 'UI_IMAGE=%q\nUI_IMAGE_ID=%q\n' "$UI_IMAGE" "$(docker image inspect --format '{{.Id}}' "$UI_IMAGE")" > "$UI_BUNDLE/ui.env"
cp "$ROOT/deploy/install-ui.sh" "$UI_BUNDLE/install-ui.sh"
cp "$ROOT/assets/vendor/manifest.json" "$UI_BUNDLE/browser-assets.json"
(cd "$UI_BUNDLE" && sha256sum ui-image.tar.gz ui.env install-ui.sh browser-assets.json > SHA256SUMS)
echo "Offline chat UI bundle: $UI_BUNDLE"
du -sh "$UI_BUNDLE"
