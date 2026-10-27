#!/usr/bin/env python3
"""Download only required model files, with pinned revisions and verified hashes."""
import hashlib
import json
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else "/bundle")
SPECS = [
    ("TinyLlama/TinyLlama-1.1B-Chat-v1.0", "shared/tinyllama-1.1b",
     ["config.json", "generation_config.json", "tokenizer.json", "tokenizer_config.json",
      "special_tokens_map.json", "model.safetensors", "README.md"]),
    ("bartowski/Qwen2.5-7B-Instruct-GGUF", "fallback/qwen2.5-7b-instruct",
     ["Qwen2.5-7B-Instruct-Q4_K_M.gguf", "README.md"]),
    ("Qwen/Qwen2.5-7B-Instruct", "fallback/qwen2.5-7b-instruct",
     ["tokenizer.json", "tokenizer_config.json", "LICENSE", "README.md"]),
]


def get_json(url):
    for attempt in range(5):
        try:
            with urllib.request.urlopen(url, timeout=60) as response:
                return json.load(response)
        except (OSError, ValueError):
            if attempt == 4:
                raise
            time.sleep(2 ** attempt)


def hashes(path):
    sha = hashlib.sha256()
    git = hashlib.sha1()
    git.update(f"blob {path.stat().st_size}\0".encode())
    with path.open("rb") as handle:
        while chunk := handle.read(4 * 1024 * 1024):
            sha.update(chunk)
            git.update(chunk)
    return sha.hexdigest(), git.hexdigest()


def verified(path, entry):
    if not path.is_file() or path.stat().st_size != entry["size"]:
        return False
    sha, git = hashes(path)
    expected = entry.get("lfs", {}).get("sha256")
    return sha == expected if expected else git == entry["blobId"]


def download(url, destination, entry):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if verified(destination, entry):
        print(f"Cached and verified: {destination.name}", flush=True)
        return
    partial = destination.with_name(destination.name + ".partial")
    for attempt in range(8):
        offset = partial.stat().st_size if partial.exists() else 0
        if offset == entry["size"]:
            if verified(partial, entry):
                partial.replace(destination)
                return
            partial.unlink()
            offset = 0
        try:
            # Refresh redirects on each attempt; signed CDN URLs expire.
            request = urllib.request.Request(url + f"?download=true&attempt={time.time_ns()}")
            if offset:
                request.add_header("Range", f"bytes={offset}-")
            with urllib.request.urlopen(request, timeout=120) as response:
                if response.status == 206:
                    content_range = response.headers.get("Content-Range", "")
                    if not content_range.startswith(f"bytes {offset}-"):
                        raise ValueError(f"Unexpected Content-Range: {content_range}")
                elif response.status == 200:
                    offset = 0
                else:
                    raise ValueError(f"Unexpected HTTP status {response.status}")
                mode = "ab" if offset else "wb"
                last_report = time.monotonic()
                with partial.open(mode) as output:
                    count = offset
                    while chunk := response.read(4 * 1024 * 1024):
                        output.write(chunk)
                        count += len(chunk)
                        if count > entry["size"]:
                            raise ValueError("Response exceeded expected file size")
                        if time.monotonic() - last_report > 15:
                            print(f"{destination.name}: {count / 1e9:.3f}/{entry['size'] / 1e9:.3f} GB", flush=True)
                            last_report = time.monotonic()
            if not verified(partial, entry):
                raise ValueError(f"Size/hash mismatch for {destination.name}")
            partial.replace(destination)
            print(f"Downloaded and verified: {destination.name}", flush=True)
            return
        except (OSError, ValueError) as error:
            # Avoid logging signed URLs or credentials from exceptions.
            print(f"Retry {attempt + 1}/8 for {destination.name}: {type(error).__name__}", flush=True)
            if attempt == 7:
                raise RuntimeError(f"Download failed: {destination.name}") from None
            if partial.exists() and partial.stat().st_size >= entry["size"]:
                partial.unlink()
            time.sleep(min(2 ** attempt, 30))


def main():
    manifest_path = ROOT / "model-manifest.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
    else:
        manifest = {"repositories": [], "files": []}
        for repo, directory, filenames in SPECS:
            info = get_json(f"https://huggingface.co/api/models/{repo}?blobs=true")
            revision = info["sha"]
            manifest["repositories"].append({"id": repo, "revision": revision})
            available = {item["rfilename"]: item for item in info["siblings"]}
            for name in filenames:
                entry = available[name]
                local_name = f"{repo.split('/')[0]}-README.md" if name == "README.md" else name
                manifest["files"].append({
                    "repo": repo, "revision": revision, "filename": name,
                    "path": f"models/{directory}/{local_name}", "metadata": entry,
                })
        ROOT.mkdir(parents=True, exist_ok=True)
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Model payload: {sum(f['metadata']['size'] for f in manifest['files']) / 1e9:.3f} GB", flush=True)
    def fetch(item):
        url = f"https://huggingface.co/{item['repo']}/resolve/{item['revision']}/{urllib.parse.quote(item['filename'])}"
        download(url, ROOT / item["path"], item["metadata"])
    with ThreadPoolExecutor(max_workers=3) as pool:
        list(pool.map(fetch, manifest["files"]))
    print("All model files verified.", flush=True)


if __name__ == "__main__":
    main()
