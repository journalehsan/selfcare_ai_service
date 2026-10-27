#!/usr/bin/env bash
# Deploy the small standalone UI bundle over the office VPN; never pull images.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="${UI_BUNDLE_DIR:-$ROOT/offline-ui-bundle}"
TARGET="${DEPLOY_TARGET:-localadmin@10.227.36.102}"
cd "$BUNDLE"
sha256sum --check --quiet SHA256SUMS
CONTROL_DIR="$(mktemp -d /tmp/selfcare-ui-ssh.XXXXXXXX)"
CONTROL_PATH="${DEPLOY_SSH_CONTROL_PATH:-$CONTROL_DIR/socket}"
cleanup() {
    ssh -S "$CONTROL_PATH" -O exit "$TARGET" >/dev/null 2>&1 || true
    rmdir "$CONTROL_DIR" 2>/dev/null || true
}
trap cleanup EXIT
SSH_OPTIONS=(-o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o IPQoS=none -o ControlMaster=auto -o ControlPersist=300 -S "$CONTROL_PATH")
# OpenSSH prompts interactively. No password is stored or passed as an argument.
ssh "${SSH_OPTIONS[@]}" "$TARGET" 'umask 022; mkdir -p "$HOME/selfcare-ai-ui-offline"'
tar -czf - SHA256SUMS ui.env install-ui.sh browser-assets.json ui-image.tar.gz | \
    ssh "${SSH_OPTIONS[@]}" "$TARGET" 'tar -xzf - -C "$HOME/selfcare-ai-ui-offline"'
ssh -t "${SSH_OPTIONS[@]}" "$TARGET" 'cd "$HOME/selfcare-ai-ui-offline" && sudo bash ./install-ui.sh'
echo "Open http://${TARGET#*@}:8080"
