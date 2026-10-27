#!/usr/bin/env python3
"""Assemble a scratch runtime using the built binary's actual shared libraries."""
from pathlib import Path
import re
import shutil
import subprocess
import sys

binary, root = Path(sys.argv[1]), Path(sys.argv[2])
root.mkdir(parents=True, exist_ok=True)


def copy(source, target=None):
    source = Path(source)
    target = root / str(target or source).lstrip("/")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target, follow_symlinks=True)


for executable, target in [(binary, "/usr/local/bin/selfcare_ai_service"),
                           (Path("/usr/bin/curl"), "/usr/bin/curl")]:
    copy(executable, target)
    listing = subprocess.check_output(["ldd", str(executable)], text=True)
    if "not found" in listing:
        raise RuntimeError(listing)
    for library in re.findall(r"(?:=>\s+)?(/[^\s()]+)", listing):
        copy(library)
copy("/etc/ssl/certs/ca-certificates.crt")
(root / "etc/passwd").write_text("root:x:0:0:root:/root:/sbin/nologin\nselfcare:x:10001:10001:SelfCare:/app:/sbin/nologin\n")
(root / "etc/group").write_text("root:x:0:\nselfcare:x:10001:\n")
(root / "etc/nsswitch.conf").write_text("hosts: files dns\n")
(root / "tmp").mkdir(exist_ok=True)
(root / "tmp").chmod(0o1777)
(root / "app/data").mkdir(parents=True, exist_ok=True)
