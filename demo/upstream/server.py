from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import urlopen
from urllib.error import HTTPError, URLError
from pathlib import Path

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        try:
            with urlopen('https://cache.nixos.org' + self.path, timeout=30) as response:
                body = response.read()
                content_type = response.headers.get('Content-Type', 'application/octet-stream')
            if self.path.endswith('.narinfo') and Path('/control/tamper').exists():
                body = body.replace(b'URL: nar/', b'URL: nar/tampered-', 1)
            self.send_response(200)
            self.send_header('Content-Type', content_type)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        except HTTPError as e:
            self.send_error(e.code)
        except URLError as e:
            self.send_error(502, str(e.reason))

ThreadingHTTPServer(('0.0.0.0', 8083), Handler).serve_forever()
