# translog-nix

Nix バイナリキャッシュの NarInfo を観測し、最初の有効な署名付き NarInfo を固定するプロトタイプです。上流の再観測結果を SQLite に記録し、固定した NarInfo を Nix 向けプロキシから返します。目的と信頼境界は [設計書](specs/design.md) を参照してください。

## 起動

Rust と C コンパイラを用意し、`cargo build --release --workspace` でビルドします。以下は cache.nixos.org を対象にした例です。

```sh
export TRANSLOG_PUBLIC_KEYS='cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY='
./target/release/translog-log-server
```

別の端末でプロキシを起動します。

```sh
./target/release/translog-proxy
```

### NixOS module

このリポジトリの `nix/modules` を import すると、ログサーバーとプロキシをそれぞれ systemd サービスとして有効にできます。例えば、このリポジトリを `./translog-nix` に配置した場合は次のように設定します。

```nix
{
  imports = [ ./translog-nix/nix/modules ];

  services.translog-nix = {
    logServer = {
      enable = true;
      publicKeys = [
        "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      ];
    };
    proxy.enable = true;
  };
}
```

ログサーバーのみ、またはプロキシのみを別々のホストで有効化できます。`logServer.listen`、`upstream`、`cacheId`、`publicKeys`、`reobserveSeconds` と、`proxy.listen`、`logUrl` を変更できます。`package` で使用するパッケージを上書きできます。ログサーバーの DB は systemd が管理する `/var/lib/translog-nix/translog.sqlite` に保存されます。module は Nix の substituter や trusted-public-keys、ファイアウォールを変更しません。

デフォルトではログサーバーが `127.0.0.1:8080`、プロキシが `127.0.0.1:8081` で待ち受けます。Nix の substituter に `http://127.0.0.1:8081` を設定し、上流の公開鍵を Nix の trusted-public-keys に設定します。複数 substituter やフォールバックは本プロトタイプの対象外です。Nix daemon を使う場合、これらの設定とプロキシへの接続は daemon 側から有効である必要があります。

## 設定

環境変数で設定します。URL は `http` または `https` で、末尾を `/` にします。

| 環境変数 | 用途 | デフォルト |
| --- | --- | --- |
| `TRANSLOG_LISTEN` | 待受アドレス | log: `127.0.0.1:8080`、proxy: `127.0.0.1:8081` |
| `TRANSLOG_UPSTREAM` | 上流キャッシュの URL | `https://cache.nixos.org/` |
| `TRANSLOG_CACHE_ID` | 上流を識別する文字列 | 上流 URL |
| `TRANSLOG_PUBLIC_KEYS` | 信頼する Nix 公開鍵。複数の場合は空白区切り | 必須 |
| `TRANSLOG_DATABASE` | SQLite ファイルのパス | `translog.sqlite` |
| `TRANSLOG_REOBSERVE_SECONDS` | 全登録対象の再観測開始間隔。正の整数秒 | `3600` |
| `TRANSLOG_LOG_URL` | プロキシが参照するログサーバー URL | `http://127.0.0.1:8080/` |

１つのログサーバープロセスは１つの上流を対象とします。同じ `TRANSLOG_CACHE_ID` は同じキャッシュについてのみ使ってください。初回観測時のエンドポイントは基準とともに保持されます。DB は基準と観測履歴を含むため、SQLite の WAL を含めて整合性の取れた方法でバックアップしてください。

## 観測と監視

- `GET /<store-path-hash>.narinfo`: 基準がなければ上流から取得・署名検証・記録し、基準を返します。基準があれば上流を問い合わせずに返します。
- `GET /observations?after=0`: 記録順の観測を JSON で最大 1,000 件返します。続きを取得するには最後の `id` を `after` に指定します。`baseline` が初回基準かを示します。
- `GET /metrics`: 基準数、基準と異なるハッシュの観測回数、観測エラー数、再観測の残件数、最後の観測成功時刻を返します。

同じ不一致を繰り返し観測すると、その都度履歴と不一致カウンターに加算されます。基準数と不一致回数は DB から計算し、エラー数と最終成功時刻はプロセス再起動時にリセットされます。再観測の間隔は各巡回の開始を基準とし、対象が多いと上流への問い合わせ負荷と履歴の増加が生じます。

初回の取得や署名検証に失敗した場合、基準は作成されずリクエストは失敗します。現時点では署名検証に失敗した NarInfo の本文は履歴に保存せず、エラー件数として記録します。NAR が変更・削除されれば、固定した NarInfo を使う取得は失敗し得ます。プロキシとログサーバーは信頼して運用してください。履歴は公開できますが、現段階では改ざんを暗号学的に検証する transparency log の証明機能はありません。ログ API に認証機能はないため、公開する場合は別途アクセス制御や TLS を設定してください。
