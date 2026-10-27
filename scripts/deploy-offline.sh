#!/usr/bin/env bash
# Run only after reconnecting to the office LAN. Passwords are prompted by SSH/sudo.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="${BUNDLE_DIR:-$ROOT/offline-bundle}"
TARGET="${DEPLOY_TARGET:-localadmin@10.227.36.102}"
MODE=full
SKIP_BUILD=false
for argument in "$@"; do
    case "$argument" in
        --update-service) MODE=update ;;
        --prepare-update) MODE=prepare ;;
        --skip-build) SKIP_BUILD=true ;;
        --full) MODE=full ;;
        -h|--help)
            echo 'Usage: scripts/deploy-offline.sh [--full | --update-service [--skip-build] | --prepare-update]'
            echo 'Default: full installation. --update-service builds offline and replaces only the AI service.'
            exit 0 ;;
        *) echo "Unknown argument: $argument" >&2; exit 1 ;;
    esac
done
if [[ "$SKIP_BUILD" == true && "$MODE" != update ]]; then
    echo '--skip-build requires --update-service' >&2
    exit 1
fi
UPDATE="${SERVICE_UPDATE_DIR:-$ROOT/.cache/offline-service-update}"
if [[ "$MODE" != full && "$SKIP_BUILD" != true ]]; then
    BUILD="${BUILD_DIR:-$ROOT/.cache/offline-build}"
    BUILDER_IMAGE="${BUILDER_IMAGE:-docker.arvancloud.ir/rust:latest}"
    docker info >/dev/null
    docker image inspect "$BUILDER_IMAGE" >/dev/null
    [[ -d "$BUILD/cargo/registry" ]] || { echo 'Offline Cargo cache missing. Prepare the full bundle first.' >&2; exit 1; }
    mkdir -p "$UPDATE" "$BUILD"
    BUILD="$(cd "$BUILD" && pwd)"
    echo 'Building only the service using cached dependencies (network disabled)...'
    docker run --rm --pull never --network none --user "$(id -u):$(id -g)" \
        -e CARGO_NET_OFFLINE=true \
        --mount "type=bind,src=$ROOT,dst=/source,readonly" \
        --mount "type=bind,src=$BUILD,dst=/work" \
        "$BUILDER_IMAGE" bash /source/scripts/offline/build_inside.sh
    UPDATE_IMAGE="selfcare-ai:update-$(date -u +%Y%m%dT%H%M%SZ)"
    docker build --network none --pull=false -t "$UPDATE_IMAGE" "$BUILD/runtime"
    docker save "$UPDATE_IMAGE" | gzip -1 > "$UPDATE/service-image.tar.gz.partial"
    mv "$UPDATE/service-image.tar.gz.partial" "$UPDATE/service-image.tar.gz"
    docker image inspect --format '{{.Id}}' "$UPDATE_IMAGE" > "$UPDATE/image-id"
    cp "$ROOT/deploy/update-service.py" "$UPDATE/update-service.py"
    cp "$BUILD/Cargo.lock" "$BUILD/runtime-libraries.txt" "$UPDATE/"
    (cd "$UPDATE" && sha256sum service-image.tar.gz image-id update-service.py Cargo.lock runtime-libraries.txt > SHA256SUMS)
    echo "Service update ready: $UPDATE"
fi
if [[ "$MODE" == prepare ]]; then
    exit 0
elif [[ "$MODE" == update ]]; then
    BUNDLE="$UPDATE"
fi
if [[ ! -f "$BUNDLE/SHA256SUMS" ]]; then
    echo "Bundle not ready. Run ./scripts/prepare-offline.sh on the connected machine first." >&2
    exit 1
fi
cd "$BUNDLE"
sha256sum --check --quiet SHA256SUMS
# A private control socket lets transfer/install reuse interactive SSH login.
CONTROL_DIR="$(mktemp -d /tmp/selfcare-ai-ssh.XXXXXXXX)"
CONTROL_PATH="${DEPLOY_SSH_CONTROL_PATH:-$CONTROL_DIR/socket}"
cleanup() {
    ssh -S "$CONTROL_PATH" -O exit "$TARGET" >/dev/null 2>&1 || true
    rmdir "$CONTROL_DIR" 2>/dev/null || true
}
trap cleanup EXIT
SSH=(ssh -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o IPQoS=none -o ControlMaster=auto -o ControlPersist=300 -S "$CONTROL_PATH")
"${SSH[@]}" "$TARGET" 'set -eu; test "$(uname -m)" = x86_64; command -v tar >/dev/null; command -v sudo >/dev/null; printf "VM: "; uname -srm; cat /etc/os-release; command -v docker >/dev/null && docker --version || true; df -h "$HOME"'
if [[ "$MODE" == update ]]; then
    "${SSH[@]}" "$TARGET" 'command -v python3 >/dev/null; command -v docker >/dev/null'
    echo 'Transferring only the service image and updater...'
    tar -cf - SHA256SUMS service-image.tar.gz image-id update-service.py Cargo.lock runtime-libraries.txt | \
        "${SSH[@]}" "$TARGET" 'set -eu; umask 077; mkdir -p "$HOME/selfcare-ai-service-update"; tar -xf - -C "$HOME/selfcare-ai-service-update"'
    "${SSH[@]}" -t "$TARGET" 'cd "$HOME/selfcare-ai-service-update" && sha256sum --check --quiet SHA256SUMS && sudo python3 ./update-service.py'
    exit 0
fi
echo 'Transferring models, images and installer packages over the office LAN...'
tar --exclude='./state' -cf - . | "${SSH[@]}" "$TARGET" \
    'set -eu; umask 022; mkdir -p "$HOME/selfcare-ai-offline"; tar -xf - -C "$HOME/selfcare-ai-offline"'
"${SSH[@]}" -t "$TARGET" 'cd "$HOME/selfcare-ai-offline" && sudo bash ./install.sh'
