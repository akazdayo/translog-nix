# 最初から不正だった場合のデモ

最初に観測される `gcc-wrapper` が偽物だった場合、Proxy がそれを基準にしてしまう様子を確認します。デモの信頼鍵が侵害され、偽物にも有効な署名を付けられる想定です。[通常のすり替えデモ](attack.md)と異なり、正規コンパイラを先に記録しません。

Docker Compose とインターネット接続が必要です。コマンドはリポジトリのルートで実行します。

```sh
docker compose up --build -d
# 偽コンパイラがまだなければ準備する（既にあれば再実行してもよい）
docker compose run --rm --build prepare-attack
# 以前のログとスナップショットを消し、通常状態に戻す
./demo/reset-log.sh
# コンパイラを最初に取得する前に、すり替えを有効にする
docker compose exec upstream touch /control/tamper
# Proxy 経由で正規 flake をビルド・実行する
docker compose run --rm client run --option substituters http://proxy:8081 -L path:/demo#default
```

実行結果は `HACKED! HACKED! HACKED!` になります。Proxy は未記録の NarInfo を上流から返し、Observer に非同期で観測を依頼します。[Transparency Log UI](http://localhost:18084/) を更新し、`gcc-wrapper` のエントリーに `Sig: demo-compromised-1:` と `URL: nar/attack.nar` が含まれることを確認してください。非同期のため、表示されるまで少し待つ必要があります。

UI に現れてからさらに5秒以上待ち、同じ Proxy 経由のコマンドをもう一度実行します。Proxy はログに記録された最初の有効な値を使いますが、その値自体が偽物なので、実行結果は再び `HACKED!` になります。`docker compose logs proxy` に対象の `serving verified log entry` が表示されることも確認できます。

このデモは初回の不正を検知・防止できないことを示します。Proxy は記録済みの値を上流と比較して警告する機能も持っていません。

終了後は `./demo/reset-log.sh` を実行すると、不正な記録とスナップショットが削除され、`tamper` が解除されます。正規の値を記録した状態からやり直すには、続けて[通常動作のデモ](normal.md)を実行してください。
