#!/usr/bin/env python3
"""Replace the owned AI container, preserving configuration and rolling back on failure."""
import copy
import http.client
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import time
from urllib.parse import quote


def docker(*args, check=True):
    return subprocess.run(["docker", *args], check=check, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def inspect(name):
    return json.loads(docker("inspect", name).stdout)[0]


def replacement_spec(previous, image):
    config = copy.deepcopy(previous["Config"])
    config["Image"] = image
    if config.get("Hostname") == previous["Id"][:12]:
        config["Hostname"] = ""
    host = copy.deepcopy(previous["HostConfig"])
    if host.get("AutoRemove") or host.get("ContainerIDFile"):
        raise RuntimeError("AutoRemove and ContainerIDFile are unsupported for service updates")
    # Bind mounts are already in HostConfig. Docker-created anonymous volumes
    # must be made explicit to keep the same data when creating the replacement.
    host["Mounts"] = host.get("Mounts") or []
    declared = {m["Target"] for m in host["Mounts"]}
    for mount in previous.get("Mounts", []):
        if mount["Type"] == "volume" and mount["Destination"] not in declared:
            # Binds can contain Docker's legacy named-volume syntax.
            if any(f':{mount["Destination"]}' in b for b in host.get("Binds") or []):
                continue
            host.setdefault("Mounts", []).append({
                "Type": "volume", "Source": mount["Name"],
                "Target": mount["Destination"], "ReadOnly": not mount["RW"],
            })
    endpoints = {}
    for network, endpoint in previous["NetworkSettings"]["Networks"].items():
        endpoints[network] = {
            key: copy.deepcopy(endpoint[key])
            for key in ("IPAMConfig", "Links", "Aliases", "DriverOpts")
            if endpoint.get(key) is not None
        }
        endpoints[network]["Aliases"] = [
            alias for alias in endpoint.get("Aliases") or []
            if alias not in (previous["Id"], previous["Id"][:12])
        ]
    return {**config, "HostConfig": host, "NetworkingConfig": {"EndpointsConfig": endpoints}}


class DockerConnection(http.client.HTTPConnection):
    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect("/var/run/docker.sock")


def create_container(name, spec):
    version = docker("version", "--format", "{{.Server.APIVersion}}").stdout.strip()
    connection = DockerConnection("localhost", timeout=60)
    try:
        connection.request("POST", f"/v{version}/containers/create?name={quote(name)}",
                           json.dumps(spec), {"Content-Type": "application/json"})
        response = connection.getresponse()
        result = json.loads(response.read())
        if response.status != 201:
            raise RuntimeError(f'Cannot create replacement: {result.get("message", response.status)}')
        return result["Id"]
    finally:
        connection.close()


def wait_ready(service, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = docker("exec", service, "/usr/bin/curl", "--fail", "--silent",
                        "--max-time", "4", "http://127.0.0.1:5732/api/ready", check=False)
        if result.returncode == 0:
            return
        if not inspect(service)["State"]["Running"]:
            break
        time.sleep(3)
    logs = docker("logs", "--tail", "80", service, check=False)
    print(logs.stdout + logs.stderr, flush=True)
    raise RuntimeError("Updated service did not become ready")


def reload_proxy(service):
    ui = f"{service}-ui"
    result = docker("inspect", ui, check=False)
    if result.returncode:
        return
    info = json.loads(result.stdout)[0]
    if (info["State"]["Running"] and
            info["Config"].get("Labels", {}).get("com.selfcare.component") == "ui"):
        docker("exec", ui, "nginx", "-s", "reload")


def update_service(image, service="selfcare-ai", timeout=300):
    previous = inspect(service)
    if previous["Config"].get("Labels", {}).get("com.selfcare.offline") != "true":
        raise RuntimeError(f"Refusing to replace unrelated container {service}")
    if not previous["State"]["Running"] or previous["State"].get("Paused"):
        raise RuntimeError("Existing service must be running and unpaused before updating")
    spec = replacement_spec(previous, image)
    backup = f"{service}-previous-{time.time_ns()}"
    candidate = create_container(f"{service}-candidate-{time.time_ns()}", spec)
    renamed = False
    stopped = False
    try:
        print("Stopping the AI service; Redis, models and cache volumes remain in place...", flush=True)
        # Mark the stop before issuing it so rollback also handles an interrupted stop.
        stopped = True
        docker("stop", "--time", "30", service)
        docker("rename", service, backup)
        renamed = True
        docker("rename", candidate, service)
        docker("start", candidate)
        print("Waiting for local models to load...", flush=True)
        wait_ready(service, timeout)
        reload_proxy(service)
    except BaseException:
        print("Update failed; restoring the previous service...", flush=True)
        docker("rm", "--force", candidate, check=False)
        if renamed:
            docker("rename", backup, service)
        if stopped:
            docker("start", service)
            wait_ready(service, timeout)
            reload_proxy(service)
        raise
    print(f"Service ready. Previous container retained as {backup} (stopped).", flush=True)


def main():
    if os.geteuid() != 0:
        raise SystemExit("Run with sudo python3 ./update-service.py")
    def interrupted(signum, _frame):
        raise RuntimeError(f"Update interrupted by signal {signum}")
    for signum in (signal.SIGHUP, signal.SIGTERM):
        signal.signal(signum, interrupted)
    root = Path(__file__).resolve().parent
    subprocess.run(["sha256sum", "--check", "--quiet", "SHA256SUMS"], cwd=root, check=True)
    image = (root / "image-id").read_text().strip()
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", image):
        raise SystemExit("Invalid image-id")
    service = os.environ.get("STACK_NAME", "selfcare-ai")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", service):
        raise SystemExit("Invalid STACK_NAME")
    timeout = int(os.environ.get("READY_TIMEOUT", "300"))
    if timeout < 1:
        raise SystemExit("READY_TIMEOUT must be positive")
    # Check ownership before loading the update or touching the live service.
    if inspect(service)["Config"].get("Labels", {}).get("com.selfcare.offline") != "true":
        raise SystemExit(f"Refusing to replace unrelated container {service}")
    print("Loading the service-only image...", flush=True)
    docker("load", "-i", str(root / "service-image.tar.gz"))
    docker("image", "inspect", image)
    update_service(image, service, timeout)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        # Do not print subprocess arguments: preserved environment values may be sensitive.
        raise SystemExit(str(error) if isinstance(error, RuntimeError) else error.stderr or "Docker command failed")
