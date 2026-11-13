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
            content = '<p class="error">同期に失敗: ' + html.escape(result.stderr) + '</p>'
        else:
            with open('/data/snapshot.json') as f:
                snapshot = json.load(f)
            entries = snapshot.get('entries') or []
            content = '<div class="section-heading"><h2>エントリー</h2><span class="count">検証済み ' + str(len(entries)) + ' 件</span></div>'
            if not entries:
                content += '<p class="empty">エントリーはまだありません。</p>'
            for index, entry in enumerate(entries):
                raw = base64.b64decode(entry).decode('utf-8', errors='replace')
                label = ''
                try:
                    parsed = json.loads(raw)
                    if isinstance(parsed, dict) and isinstance(parsed.get('store_path'), str):
                        label = ' — ' + html.escape(parsed['store_path'])
                    raw = json.dumps(parsed, ensure_ascii=False, indent=2)
                except ValueError:
                    pass
                content += '<details><summary>エントリー ' + str(index) + label + '</summary><pre>' + html.escape(raw) + '</pre></details>'
            content += '<h2 class="checkpoint-heading">署名付きチェックポイント</h2><pre class="checkpoint">' + html.escape(base64.b64decode(snapshot['checkpoint']).decode('utf-8', errors='replace')) + '</pre>'
        body = ('''<!doctype html>
<html lang="ja"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Transparency Log</title>
<style>
  * { box-sizing: border-box; }
  body { margin: 0; background: #f5f7fa; color: #202b3a; font: 15px/1.6 system-ui, sans-serif; }
  main { max-width: 1000px; margin: 0 auto; padding: 40px 20px 64px; }
  header, .section-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  header { margin-bottom: 36px; }
  h1 { margin: 0; font-size: 1.6rem; letter-spacing: -.02em; }
  h2 { margin: 0; font-size: 1.05rem; }
  .refresh { color: #2456a0; background: #fff; border: 1px solid #cbd5e1; border-radius: 6px; padding: 6px 16px; text-decoration: none; white-space: nowrap; }
  .refresh:hover { background: #eef3fa; }
  .refresh:focus-visible, summary:focus-visible { outline: 3px solid #76a4e7; outline-offset: 2px; }
  .section-heading { margin-bottom: 12px; }
  .count { color: #526477; font-size: .9rem; }
  details, .checkpoint, .empty, .error { background: #fff; border: 1px solid #dce3eb; border-radius: 8px; }
  details { margin: 8px 0; overflow: hidden; }
  summary { cursor: pointer; padding: 12px 16px; font-weight: 600; }
  summary:hover { background: #f0f5fb; }
  details[open] summary { border-bottom: 1px solid #dce3eb; }
  pre { margin: 0; padding: 16px; white-space: pre-wrap; overflow-wrap: anywhere; font: .85rem/1.6 ui-monospace, monospace; color: #34455a; }
  .checkpoint-heading { margin: 36px 0 12px; }
  .empty, .error { padding: 16px; }
  .error { color: #9b2929; border-color: #e9bcbc; }
  @media (max-width: 480px) { main { padding: 24px 14px 48px; } h1 { font-size: 1.3rem; } }
</style>
<main><header><h1>Transparency Log</h1><a class="refresh" href="/">更新</a></header>''' + content + '</main></html>').encode()
        self.send_response(200 if result.returncode == 0 else 503)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

HTTPServer(('0.0.0.0', 8084), Handler).serve_forever()
