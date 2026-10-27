#!/usr/bin/env python3
"""Seal a complete bundle without including runtime state or partial downloads."""
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1]).resolve()
manifest = json.loads((root / "model-manifest.json").read_text())
required = [entry["path"] for entry in manifest["files"]]
required += ["images/services.tar.gz", "images.env", "install.sh", "README.md", "Cargo.lock",
             "packages/target.env", "runtime-libraries.txt", "image-inspect.json"]
for name in required:
    if not (root / name).is_file() or (root / name).stat().st_size == 0:
        raise SystemExit(f"Bundle incomplete: {name}")
if not list((root / "packages").glob("*.deb")):
    raise SystemExit("Bundle missing offline Docker packages")
if list(root.rglob("*.partial")):
    raise SystemExit("Bundle still contains partial downloads")
lines = []
for path in sorted(root.rglob("*")):
    if not path.is_file() or path.name == "SHA256SUMS" or "state" in path.relative_to(root).parts:
        continue
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(4 * 1024 * 1024):
            digest.update(chunk)
    lines.append(f"{digest.hexdigest()}  {path.relative_to(root)}\n")
(root / "SHA256SUMS").write_text("".join(lines))
print(f"Sealed {len(lines)} files ({sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) / 1e9:.3f} GB total bundle).")
