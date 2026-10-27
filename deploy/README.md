# Offline SelfCare AI deployment

This directory contains pinned and verified TinyLlama and Qwen2.5 model files,
the precompiled Linux amd64 service and Redis Docker images, and offline Docker
installer packages for Ubuntu 26.04 amd64. No source compilation or downloads run
on the VM. Model provenance is in `model-manifest.json`; file checksums are in
`SHA256SUMS`. The exact Rust dependencies are recorded in `Cargo.lock`.

From the laptop, reconnect to the office network and run:

```bash
./scripts/deploy-offline.sh
```

The default target is `localadmin@10.227.36.102`. SSH and sudo prompt for your
password; it is never embedded in these files. SSH's normal host-key verification
remains enabled. Files are transferred into `~/selfcare-ai-offline` on the VM.
There is no SSH connection or deployment during preparation.

For later Rust code changes, update only the AI service from the laptop checkout:

```bash
./scripts/deploy-offline.sh --update-service
```

This builds with the cached Docker builder and Cargo dependencies with networking
disabled, transfers only the service image and updater, and replaces the AI
container. Models, Docker installer packages, Redis and its data are not transferred
or restarted. The existing service's environment, ports, mounts, network and
security settings are preserved. Python 3 and Docker must already be installed
on the VM. The updater waits for readiness, reloads the owned UI's nginx proxy
if present, and restores the previous service if startup fails. A successful
update keeps the previous service container stopped for inspection.

To prepare the small update before reconnecting to the office LAN:

```bash
./scripts/deploy-offline.sh --prepare-update
./scripts/deploy-offline.sh --update-service --skip-build
```

Artifacts are stored separately in `.cache/offline-service-update/`; the original
full bundle is unchanged. `SERVICE_UPDATE_DIR` overrides that location. SSH and
sudo still prompt normally; no credentials are stored in the update. Running
without flags or with `--full` retains the original full-installation behavior.

For manual deployment, copy this entire directory to the VM, then run:

```bash
sudo bash ./install.sh
```

The VM needs Linux amd64, Bash, tar, gzip, sha256sum, sudo, systemd (if installing
Docker), and sufficient storage for the 7 GB bundle plus loaded images and cache
growth. Start with at least 12 GB free disk and approximately 24 GB RAM; verify
the actual service memory and response latency with the VM's workload. If Docker
is installed already, the host distribution need not be Ubuntu. If Docker is
absent, the included packages require Ubuntu 26.04 amd64; the installer refuses
other distributions before trying to install these packages.
The current package set was resolved against this deployment VM's installed
packages; `packages/installation-plan.txt` records the three new packages.

The service publishes port 5732, Redis has no published port, and both containers
use a private Docker bridge network. Model paths and offline flags are configured
in the service container; startup never pulls images. The service API has no
authentication layer; publish it only on the intended trusted office network.
To bind a specific VM interface, use `sudo BIND_ADDRESS=10.227.36.102 bash ./install.sh`.

The installer verifies every artifact, loads images, starts Redis, and waits up
to 180 seconds for `/api/ready`. Readiness confirms Qwen loaded; examine logs to
confirm TinyLlama also loaded. An inference request is still required to measure
latency and answer quality. If readiness fails, containers and logs remain for
inspection. Re-running replaces only containers labeled by this installer and
preserves the named SQLite and Redis data volumes. Keep `models/` at this path
while the service runs; it is mounted read-only into the service container.

```bash
sudo docker logs --tail 100 selfcare-ai
sudo docker exec selfcare-ai /usr/bin/curl -fsS http://127.0.0.1:5732/api/ready
sudo docker exec selfcare-ai /usr/bin/curl -fsS http://127.0.0.1:5732/api/chat \
  -H 'Content-Type: application/json' \
  -d '{"message":"Reply with a short greeting.","max_tokens":16,"cache_bypass":true}'
sudo docker stats --no-stream selfcare-ai selfcare-ai-redis
sudo docker stop selfcare-ai selfcare-ai-redis
sudo docker start selfcare-ai-redis selfcare-ai
```

Container logs are rotated. The deployment uses fresh named cache volumes; it
does not copy the laptop's existing SQLite database or `.env`. Models and their
license/source documents are preserved separately from images to avoid duplicate
multi-gigabyte copies. The browser chat UI is packaged separately with all assets vendored locally.
Build it with `./scripts/prepare-ui.sh` and deploy with `./scripts/deploy-ui.sh`
from the laptop checkout. It runs on port 8080 and proxies the existing API.
No model transfer or service restart is needed for UI updates.
