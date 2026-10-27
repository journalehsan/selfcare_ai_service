#!/usr/bin/env bash
# Run on the connected laptop. This command never contacts the deployment VM.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="${BUNDLE_DIR:-$ROOT/offline-bundle}"
BUILD="${BUILD_DIR:-$ROOT/.cache/offline-build}"
BUILDER_IMAGE="${BUILDER_IMAGE:-docker.arvancloud.ir/rust:latest}"
DOWNLOADER_IMAGE="${DOWNLOADER_IMAGE:-docker.arvancloud.ir/python:latest}"
REDIS_IMAGE="${REDIS_IMAGE:-redis:7-alpine}"
RUNTIME_IMAGE="${RUNTIME_IMAGE:-selfcare-ai:offline}"
UBUNTU_IMAGE="${UBUNTU_IMAGE:-devops-harbor.mtnirancell.ir/dockerhub/ubuntu:24.04}"
DOCKER_UBUNTU_VERSION="${DOCKER_UBUNTU_VERSION:-26.04}"
case "$DOCKER_UBUNTU_VERSION" in
    24.04) DOCKER_UBUNTU_SUITE=noble ;;
    26.04) DOCKER_UBUNTU_SUITE=resolute ;;
    *) echo 'Supported offline Docker package targets: Ubuntu 24.04 and 26.04.' >&2; exit 1 ;;
esac
cd "$ROOT"
mkdir -p "$BUNDLE" "$BUILD"
BUNDLE="$(cd "$BUNDLE" && pwd)"
BUILD="$(cd "$BUILD" && pwd)"
docker info >/dev/null
for image in "$BUILDER_IMAGE" "$DOWNLOADER_IMAGE" "$REDIS_IMAGE" "$UBUNTU_IMAGE"; do
    docker image inspect "$image" >/dev/null
done
if [[ "$(uname -m)" != x86_64 ]]; then
    echo 'This bundle currently targets Linux amd64; prepare it on an x86_64 Docker host.' >&2
    exit 1
fi
docker run --rm --pull never --user "$(id -u):$(id -g)" \
    --mount "type=bind,src=$BUNDLE,dst=/bundle" \
    --mount "type=bind,src=$ROOT/scripts/offline,dst=/scripts,readonly" \
    "$DOWNLOADER_IMAGE" python -u /scripts/download_models.py /bundle
docker run --rm --pull never --user "$(id -u):$(id -g)" \
    --mount "type=bind,src=$ROOT,dst=/source,readonly" \
    --mount "type=bind,src=$BUILD,dst=/work" \
    --mount "type=bind,src=${CARGO_SEED_DIR:-$HOME/.cargo},dst=/seed,readonly" \
    "$BUILDER_IMAGE" bash /source/scripts/offline/build_inside.sh
docker build --network none --pull=false -t "$RUNTIME_IMAGE" "$BUILD/runtime"
mkdir -p "$BUNDLE/images" "$BUNDLE/packages"
docker save "$RUNTIME_IMAGE" "$REDIS_IMAGE" | gzip -1 > "$BUNDLE/images/services.tar.gz.partial"
mv "$BUNDLE/images/services.tar.gz.partial" "$BUNDLE/images/services.tar.gz"
# Resolve the target Ubuntu repositories from a clean cached base image. This
# also includes any dependency upgrades required relative to the older base.
if [[ -s "$BUNDLE/packages/target.env" ]] && (
    . "$BUNDLE/packages/target.env"
    [[ "$ID" == ubuntu && "$VERSION_ID" == "$DOCKER_UBUNTU_VERSION" && "$ARCH" == amd64 ]]
); then
    echo "Using cached Docker installer packages for Ubuntu $DOCKER_UBUNTU_VERSION."
else
docker run --rm --pull never \
    -e "DOCKER_UBUNTU_VERSION=$DOCKER_UBUNTU_VERSION" -e "DOCKER_UBUNTU_SUITE=$DOCKER_UBUNTU_SUITE" \
    --mount "type=bind,src=$BUNDLE/packages,dst=/packages" \
    "$UBUNTU_IMAGE" bash -euc '
        . /etc/os-release
        sed -i "s/$VERSION_CODENAME/$DOCKER_UBUNTU_SUITE/g" /etc/apt/sources.list.d/ubuntu.sources
        apt-get update
        apt-get -y --download-only --no-install-recommends install docker.io
        # Remove only the previous generated installer packages after fetching
        # their complete replacement successfully.
        find /packages -maxdepth 1 -type f -name "*.deb" -delete
        cp /var/cache/apt/archives/*.deb /packages/
        printf "ID=ubuntu\nVERSION_ID=%s\nARCH=amd64\n" "$DOCKER_UBUNTU_VERSION" > /packages/target.env
        chmod -R a+rX /packages
    '
fi
cp "$BUILD/Cargo.lock" "$BUNDLE/Cargo.lock"
cp "$BUILD/runtime-libraries.txt" "$BUNDLE/runtime-libraries.txt"
cp "$ROOT/deploy/install.sh" "$BUNDLE/install.sh"
cp "$ROOT/deploy/README.md" "$BUNDLE/README.md"
printf 'SERVICE_IMAGE=%q\nREDIS_IMAGE=%q\n' "$RUNTIME_IMAGE" "$REDIS_IMAGE" > "$BUNDLE/images.env"
printf 'SERVICE_IMAGE_ID=%q\nREDIS_IMAGE_ID=%q\n' \
    "$(docker image inspect --format '{{.Id}}' "$RUNTIME_IMAGE")" \
    "$(docker image inspect --format '{{.Id}}' "$REDIS_IMAGE")" >> "$BUNDLE/images.env"
docker image inspect "$RUNTIME_IMAGE" "$REDIS_IMAGE" > "$BUNDLE/image-inspect.json"
python3 "$ROOT/scripts/offline/seal_bundle.py" "$BUNDLE"
echo "Offline bundle ready: $BUNDLE"
du -sh "$BUNDLE"
echo 'After connecting to the office network: ./scripts/deploy-offline.sh'
