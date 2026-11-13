from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError
from pathlib import Path

class Handler(BaseHTTPRequestHandler):
    def do_HEAD(self):
        self.serve(False)

    def do_GET(self):
        self.serve(True)

    def serve(self, send_body):
        try:
            method = 'HEAD' if not send_body and not self.path.endswith('.narinfo') else 'GET'
            request = Request('https://cache.nixos.org' + self.path, method=method)
            with urlopen(request, timeout=30) as response:
                body = response.read()
                content_type = response.headers.get('Content-Type', 'application/octet-stream')
                length = response.headers.get('Content-Length') if method == 'HEAD' else None
            if self.path.endswith('.narinfo') and Path('/control/tamper').exists():
                body = body.replace(b'URL: nar/', b'URL: nar/tampered-', 1)
            self.send_response(200)
            self.send_header('Content-Type', content_type)
            self.send_header('Content-Length', length or str(len(body)))
            self.end_headers()
            if send_body:
                self.wfile.write(body)
        except HTTPError as e:
            self.send_error(e.code)
        except URLError as e:
            self.send_error(502, str(e.reason))

ThreadingHTTPServer(('0.0.0.0', 8083), Handler).serve_forever()
