# 通常動作のデモ

正規の flake を、Proxy 経由で取得した nixpkgs のコンパイラでビルドし、実行します。次の[すり替えデモ](attack.md)と[Proxy のデモ](protection.md)に必要な、正規のコンパイラの記録も作ります。

Docker Compose とインターネット接続が必要です。すべてのコマンドはリポジトリのルートで実行します。`docker compose` の代わりに `docker-compose` を使う環境では読み替えてください。

以前に不正な値をログに記録した場合は、先に `./demo/reset-log.sh` でログを初期化してください。[ログを初期化する](logs.md#ログを初期化する)を参照してください。

```sh
docker compose up --build -d
docker compose exec upstream rm -f /control/tamper
# 後のデモで使う偽コンパイラを準備する
docker compose run --rm --build prepare-attack
# 正規のコンパイラで flake をビルドし、実行する
docker compose run --rm client run --option substituters http://proxy:8081 -L path:/demo#default
```

最後の標準出力が `Hello from the legitimate program!` になることを確認します。初回はコンパイラなどのダウンロードに時間がかかります。`client` は実行のたびに新しいコンテナと独立した Nix store を使うため、毎回 substituter から取得してビルドします。

[Transparency Log UI](http://localhost:18084/) を更新し、対象の `gcc-wrapper` のエントリーが表示されるまで待ちます。Proxy は5秒ごとに同期するため、表示後さらに5秒待ってから[すり替えデモ](attack.md)に進んでください。正規の値を先に記録・同期することが、Proxy による回避の前提です。[ログの見方](logs.md)も参照してください。
