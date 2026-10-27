#!/usr/bin/env python3
"""Preview only public chat assets; never serve model files, caches or the repo."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
class PublicFiles(SimpleHTTPRequestHandler):
    def send_head(self):
        path = unquote(urlsplit(self.path).path)
        resolved = (ROOT / path.lstrip('/')).resolve()
        assets = ROOT / 'assets'
        if path in ('/', '/index.html'):
            self.path = '/index.html'
        elif not resolved.is_relative_to(assets) or not resolved.is_file():
            self.send_error(404)
            return None
        return super().send_head()
    def end_headers(self):
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=8000)
    parser.add_argument('--bind', default='127.0.0.1')
    args = parser.parse_args()
    server = ThreadingHTTPServer((args.bind, args.port), partial(PublicFiles, directory=str(ROOT)))
    print(f'SelfCare UI: http://{args.bind}:{args.port}', flush=True)
    server.serve_forever()
