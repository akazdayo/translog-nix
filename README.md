# translog-nix

Nix バイナリキャッシュの NarInfo を Observer が観測し、Tessera の Transparency Log に記録するプロトタイプです。Proxy は Nix の substituter として動き、署名と整合性を検証してローカルに同期したログから、最初の有効な NarInfo を基準として返します。対象が未記録の場合は上流から取得して返し、Observer に非同期で観測を依頼します。[設計書](specs/design.md) も参照してください。

## 開発環境とビルド

`nix develop` で Rust と Go の開発環境に入ります。

```sh
cargo build --release --workspace
(cd transparency-log && go build -o ../target/translog-transparency .)
(cd transparency-log && go build -o ../target/translog-sync ./cmd/sync)
```

## 起動

まず Transparency Log の鍵を作り、表示された `Private:` と `Public:` の値をそれぞれ別のファイルに保存します。秘密鍵とログデータは継続して保持します。

```sh
./target/translog-transparency -generate-key
./target/translog-transparency -private-key /path/to/private-key -storage /path/to/log-data
```

次に Observer と Proxy をそれぞれ起動します。下記では両方が同じ上流を観測し、同じ Nix 公開鍵を信頼します。

```sh
export TRANSLOG_PUBLIC_KEYS='cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY='
./target/release/translog-log-server
```

```sh
export TRANSLOG_PUBLIC_KEYS='cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY='
export TRANSLOG_LOG_PUBLIC_KEY_FILE=/path/to/public-key
export TRANSLOG_SYNC_COMMAND="$PWD/target/translog-sync"
./target/release/translog-proxy
```

Nix の substituter に `http://127.0.0.1:8081` を、trusted-public-keys に上流の公開鍵を設定します。Nix daemon を使う場合、設定と Proxy への接続は daemon 側から有効である必要があります。

## 環境変数

URL は HTTP(S) で末尾を `/` にします。

| 変数 | 用途 | デフォルト |
| --- | --- | --- |
| `TRANSLOG_LISTEN` | 待受アドレス | Observer: `127.0.0.1:8080`、Proxy: `127.0.0.1:8081` |
| `TRANSLOG_UPSTREAM` | 観測・配信する上流 URL | `https://cache.nixos.org/` |
| `TRANSLOG_PUBLIC_KEYS` | NarInfo の検証に使う Nix 公開鍵。空白区切り | 必須 |
| `TRANSLOG_TRANSPARENCY_URL` | Transparency Log のベース URL | `http://127.0.0.1:8082/` |
| `TRANSLOG_OBSERVER_URL` | Proxy が観測を依頼する Observer の URL | `http://127.0.0.1:8080/` |
| `TRANSLOG_LOG_PUBLIC_KEY_FILE` | Proxy がチェックポイントの検証に使う公開鍵ファイル | 必須 |
| `TRANSLOG_SYNC_COMMAND` | Tessera 同期コマンド | `translog-sync` |
| `TRANSLOG_SNAPSHOT` | 検証済みログの保存先 | `translog-snapshot.json` |
| `TRANSLOG_SYNC_SECONDS` | Proxy のログ同期間隔 | `3600` |

Transparency Log は Tessera のエントリー形式に合わせ、1件あたり 65,535 バイト以下のデータを受け付けます。

Observer は `POST /observe/<store-path-hash>` で観測を受け付け、NarInfo の署名と対象を検証したうえで `POST /add` に観測を記録します。Transparency Log は `GET /checkpoint` と `/tile/`、`/entries/` を公開します。

同期コマンドはログの全エントリーを取得してルートハッシュを再計算し、チェックポイントの署名と、前回保存したチェックポイントに対する追記の整合性を検証します。成功したスナップショットのみを保存します。Proxy は記録の NarInfo の署名も検証し、同じ上流と store path の最初の有効な記録を使います。起動時は保存済みスナップショットを検証してから利用します。現状、同期のたびに全件を再取得するため、ログの成長に伴い通信量と処理時間が増加します。

ログに対象がないときは Proxy が上流から NarInfo を取得し、Observer への依頼完了を待たずに返します。依頼の再送キューはありません。初回に返した内容と後でログに記録された内容の差は当面検出・通知しません。同期に失敗した場合は直前の検証済みスナップショットを引き続き使用し、対象がなければ上流から取得します。

Transparency Log の追記 API と Observer の観測 API に認証はありません。外部公開する場合は別途アクセス制御と TLS を設定してください。チェックポイント検証はログが利用者ごとに異なる履歴を示すことを単独では検出できません。NAR 本体の保存や、上流が削除した NAR の提供も行いません。

## NixOS module

`nix/modules` を import し、Observer の `services.translog-nix.logServer` と Proxy の `services.translog-nix.proxy` をそれぞれ有効にできます。両方に `publicKeys` を設定し、Proxy の `logPublicKeyFile` に Transparency Log 公開鍵のパスを指定してください。`upstream` と `transparencyUrl` は両サービスで揃えます。`observerUrl`、`syncSeconds` と各 `listen` も変更できます。Transparency Log は別途起動してください。module は Nix の substituter、trusted-public-keys、ファイアウォールを変更しません。

## 複数コンテナのデモ

正規の flake をビルド・実行する際、substituter が nixpkgs のコンパイラを偽物にすり替えるデモです。上流コンテナは cache.nixos.org を中継し、Proxy、Observer、Transparency Log、独立した閲覧 UI は別々のコンテナで動きます。Docker Compose とインターネット接続が必要です。コマンドはプロジェクトのルートで実行します。`docker compose` の代わりに `docker-compose` を使う環境では読み替えてください。

### 1. 準備と通常動作

```sh
docker compose up --build -d
docker compose exec upstream rm -f /control/tamper
# 正規の GCC wrapper から、同じ store path を名乗る偽物を準備する
docker compose run --rm --build prepare-attack
# 正規コンパイラを Proxy 経由で取得し、正規 flake をビルド・実行する
docker compose run --rm client run --option substituters http://proxy:8081 -L path:/demo#default
```

最後に `Hello from the legitimate program!` と表示されます。初回はコンパイラなどのダウンロードに時間がかかります。新しい client コンテナは毎回独立した Nix store を使うので、後の比較でも実際に substituter から取得してビルドします。

[Transparency Log UI](http://localhost:18084/) を更新し、`gcc-wrapper` のエントリーが表示されたことを確認します。Proxy は5秒ごとにログを同期するので、表示されたあとさらに5秒待ちます。正規の値を先に記録・同期することが、このデモの前提です。

### 2. 偽コンパイラを提供する

```sh
docker compose exec upstream touch /control/tamper
# 偽コンパイラを返す上流を直接使い、同じ flake をビルド・実行する
docker compose run --rm client run --option substituters http://upstream:8083 -L path:/demo#default
```

最後の標準出力が `HACKED! HACKED! HACKED!` に変わります。flake や C ソースは変更していません。偽の GCC wrapper が C の `puts` を書き換えるヘッダーをコンパイル時に挿入し、ビルドされたプログラムの動作を変えています。

### 3. Proxy 経由で同じビルドを比較する

```sh
# すり替えは有効のまま、別のクライアントから同じ flake を実行する
docker compose run --rm client run --option substituters http://proxy:8081 -L path:/demo#default
# 終わったら通常状態に戻す
docker compose exec upstream rm -f /control/tamper
```

Proxy 経由では、ログから同期した正規の NarInfo を使うため、再び `Hello from the legitimate program!` と表示されます。Proxy は上流との差異を通知しません。UI は署名付きチェックポイントと全エントリーを検証して表示します。全サービスの動作ログは `docker compose logs -f`、特定サービスは `docker compose logs -f proxy` などで確認できます。

この攻撃は **信頼済みのデモ専用キャッシュ鍵が侵害された想定**です。偽物にはデモ鍵で正しい署名を付け、Nix は署名とハッシュを通常どおり検証します。cache.nixos.org の署名鍵を使ったり、Nix の署名検証を無効にしたりはしません。デモ鍵はリポジトリに公開されており、実環境では使えません。正規の値が記録される前にすり替えられた場合、現在の仕組みはそれを防げません。また、正規の NAR が上流から提供されなくなる場合も復旧できません。

終了は `docker compose down`。デモの記録と鍵を含む初期化は `docker compose down -v` です。Proxy が返す NAR の URL は Compose 内の `upstream` を指すため、取得には client コンテナを使ってください。
