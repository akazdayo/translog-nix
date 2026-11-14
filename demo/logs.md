# ログ閲覧のデモ

[通常動作のデモ](normal.md)を実行すると、Observer が取得した NarInfo を Transparency Log に記録します。

## Transparency Log UI

[http://localhost:18084/](http://localhost:18084/) を開き、更新ボタンを押します。エントリー一覧で `gcc-wrapper` を探して開くと、観測した store path、NarHash、NarInfo、観測時刻を確認できます。画面下部には署名付きチェックポイントが表示されます。

UI はチェックポイントの署名、全エントリーのハッシュ、前回のチェックポイントからの追記整合性を検証して表示します。観測とログの公開は非同期なので、表示されない場合は少し待って更新してください。

## 各コンテナの動作ログ

リポジトリのルートで実行します。

```sh
# 全サービスのログを追跡する（終了は Ctrl+C）
docker compose logs -f
# 個別に確認する
docker compose logs -f proxy
docker compose logs -f observer
```

Proxy のログでは上流から取得したか、検証済みログのエントリーを返したかを確認できます。Observer は観測を記録したときに出力します。各コンテナのログと Transparency Log のエントリーは別のものです。

## ログを初期化する

デモを最初からやり直すには、リポジトリのルートで実行します。

```sh
./demo/reset-log.sh
```

サービスを一度停止して Transparency Log の全エントリーと Proxy・UI の検証済みスナップショットを消し、`tamper` を解除して再起動します。ログ署名鍵と偽コンパイラは残ります。コンテナの標準出力ログは消去しません。この操作は記録を復元できないため、必要な内容は先に保存してください。[最初から不正だった場合のデモ](first-observation-attacked.md)にも使えます。

終了するときは `docker compose down` を実行します。`docker compose down -v` はデモの記録と鍵を含むボリュームを削除します。
