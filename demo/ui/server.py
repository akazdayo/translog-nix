import base64
import html
import json
import os
import subprocess
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != '/':
            self.send_error(404)
            return
        # Keep the previous snapshot so sync also checks append-only consistency.
        result = subprocess.run(['sync-log', '-url', os.environ['LOG_URL'], '-public-key', '/keys/public', '-output', '/data/snapshot.json'], capture_output=True, text=True)
        if result.returncode:
            content = '<p>同期に失敗: ' + html.escape(result.stderr) + '</p>'
        else:
            with open('/data/snapshot.json') as f:
                snapshot = json.load(f)
            entries = snapshot.get('entries') or []
            content = '<p>検証済みエントリー: ' + str(len(entries)) + ' 件</p>'
            for index, entry in enumerate(entries):
                raw = base64.b64decode(entry).decode('utf-8', errors='replace')
                try:
                    raw = json.dumps(json.loads(raw), ensure_ascii=False, indent=2)
                except ValueError:
                    pass
                content += '<details><summary>エントリー ' + str(index) + '</summary><pre>' + html.escape(raw) + '</pre></details>'
            content += '<h2>署名付きチェックポイント</h2><pre>' + html.escape(base64.b64decode(snapshot['checkpoint']).decode('utf-8', errors='replace')) + '</pre>'
        body = ('<!doctype html><meta charset="utf-8"><title>Transparency Log</title><style>body{font:16px sans-serif;max-width:90em;margin:2em auto;padding:0 1em}pre{white-space:pre-wrap;overflow-wrap:anywhere}details{margin:1em 0}</style><h1>Transparency Log</h1><p><a href="/">更新</a></p>' + content).encode()
        self.send_response(200 if result.returncode == 0 else 503)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

HTTPServer(('0.0.0.0', 8084), Handler).serve_forever()
