#!/usr/bin/env python3
"""Fetch pinned browser assets. Run online; office browsers use only local files.

On repeat runs, the checked-in manifest is an integrity check, not a new trust
anchor. Review versions, upstream releases and hashes when updating libraries.
"""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[2] / 'assets' / 'vendor'
PACKAGES = {
    'alpinejs': ('3.17.5', {'dist/cdn.min.js': 'alpine.min.js', 'https://cdn.jsdelivr.net/gh/alpinejs/alpine@v3.17.5/LICENSE.md': 'alpine.LICENSE'}),
    'dompurify': ('3.4.17', {'dist/purify.min.js': 'purify.min.js', 'LICENSE': 'dompurify.LICENSE'}),
    'marked': ('18.1.2', {'lib/marked.umd.js': 'marked.umd.js', 'LICENSE': 'marked.LICENSE'}),
    'mammoth': ('1.13.0', {'mammoth.browser.min.js': 'mammoth.browser.min.js', 'LICENSE': 'mammoth.LICENSE'}),
    'pdfjs-dist': ('6.4.299', {'build/pdf.min.mjs': 'pdf.min.mjs', 'build/pdf.worker.min.mjs': 'pdf.worker.min.mjs', 'LICENSE': 'pdfjs.LICENSE'}),
}

def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    manifest_path = ROOT / 'manifest.json'
    expected = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    jobs = [(name, version, source, target) for name, (version, files) in PACKAGES.items() for source, target in files.items()]
    def fetch(job):
        name, version, source, target = job
        url = source if source.startswith('https://') else f'https://cdn.jsdelivr.net/npm/{name}@{version}/{source}'
        cached = ROOT / target
        if cached.exists():
            data = cached.read_bytes()
        else:
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read()
        digest = hashlib.sha256(data).hexdigest()
        if target in expected and expected[target]['sha256'] != digest:
            raise RuntimeError(f'Integrity mismatch: {target}')
        (ROOT / target).write_bytes(data)
        return target, {'package': name, 'version': version, 'source': url, 'bytes': len(data), 'sha256': digest}
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        records = {}
        futures = [(job, pool.submit(fetch, job)) for job in jobs]
        for job, future in futures:
            try:
                target, record = future.result()
                records[target] = record
            except Exception as error:
                print(f'Failed {job}: {error}', flush=True)
                raise
    manifest_path.write_text(json.dumps(records, indent=2, sort_keys=True) + '\n')
    print(f'Vendored {len(records)} files, {sum(r["bytes"] for r in records.values()):,} bytes.')

if __name__ == '__main__':
    main()
