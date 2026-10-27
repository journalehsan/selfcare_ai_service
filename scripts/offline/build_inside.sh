#!/usr/bin/env bash
set -euo pipefail
export CARGO_HOME=/work/cargo
export CARGO_TARGET_DIR=/work/target
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
# Use a portable x86-64 baseline, including when the laptop supports newer SIMD.
export RUSTFLAGS='-C target-cpu=x86-64'
mkdir -p /work/source /work/cargo
if [[ ! -d /work/cargo/registry && -d /seed/registry ]]; then
    cp -a /seed/registry /work/cargo/
fi
cp /source/Cargo.toml /work/source/
cp -a /source/src /work/source/
if [[ -f /source/Cargo.lock ]]; then
    cp /source/Cargo.lock /work/source/
elif [[ -f /source/offline-bundle/Cargo.lock ]]; then
    cp /source/offline-bundle/Cargo.lock /work/source/
fi
cd /work/source
if [[ -f Cargo.lock ]]; then
    cargo build --release --locked
else
    cargo build --release
fi
mkdir -p /work/runtime
python3 /source/scripts/offline/pack_runtime.py /work/target/release/selfcare_ai_service /work/runtime/rootfs
cp /source/deploy/Dockerfile.runtime /work/runtime/Dockerfile
cp Cargo.lock /work/Cargo.lock
ldd /work/target/release/selfcare_ai_service > /work/runtime-libraries.txt
