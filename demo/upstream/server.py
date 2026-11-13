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
            attack = Path('/attack')
            if (Path('/control/tamper').exists()
                and (self.path == '/nar/attack.nar' or (self.path.count('/') == 1 and self.path.endswith('.narinfo')))):
                target = attack / ('attack.nar' if self.path == '/nar/attack.nar' else self.path[1:])
            else:
                target = None
            if target is not None and target.is_file():
                body = target.read_bytes()
                self.send_response(200)
                self.send_header('Content-Type', 'application/octet-stream')
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                if send_body:
                    self.wfile.write(body)
                return
            method = 'HEAD' if not send_body and not self.path.endswith('.narinfo') else 'GET'
            request = Request('https://cache.nixos.org' + self.path, method=method)
            with urlopen(request, timeout=30) as response:
                body = response.read()
                content_type = response.headers.get('Content-Type', 'application/octet-stream')
                length = response.headers.get('Content-Length') if method == 'HEAD' else None
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
