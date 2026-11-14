#!/bin/sh
# Reset this Compose project's demo log without deleting keys or attack data.
set -eu
cd "$(dirname "$0")/.."
if docker compose version >/dev/null 2>&1; then
    compose() { docker compose "$@"; }
elif command -v docker-compose >/dev/null 2>&1; then
    compose() { docker-compose "$@"; }
else
    echo 'Docker Compose が見つかりません。' >&2
    exit 1
fi
printf '%s\n' 'デモのサービスを停止します。'
compose stop proxy observer ui transparency upstream
compose run --rm --no-deps reset-log
printf '%s\n' 'デモのサービスを起動します。'
compose up -d
compose exec -T upstream python -c '
import time
import urllib.error
import urllib.request

deadline = time.monotonic() + 30
for url in ("http://transparency:8082/checkpoint", "http://proxy:8081/nix-cache-info"):
    while True:
        try:
            with urllib.request.urlopen(url, timeout=2) as response:
                response.read()
            break
        except (OSError, urllib.error.URLError):
            if time.monotonic() >= deadline:
                raise SystemExit("サービスの起動を確認できませんでした: " + url)
            time.sleep(0.5)
'
printf '%s\n' '初期化しました。ログの鍵と偽コンパイラは保持しています。'
