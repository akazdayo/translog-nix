# Proxy による回避のデモ

[通常動作のデモ](normal.md)で正規の `gcc-wrapper` をログに記録・同期し、[すり替えデモ](attack.md)で攻撃を有効にした状態から始めます。

リポジトリのルートで実行します。

```sh
# 攻撃は有効のまま、Proxy 経由で同じ flake をビルド・実行する
docker compose run --rm client run --option substituters http://proxy:8081 -L path:/demo#default
```

標準出力が `Hello from the legitimate program!` になることを確認します。直接上流を使ったときと異なり、Proxy はログから同期した最初の有効な NarInfo を使うため、正規のコンパイラが取得されます。[ログの見方](logs.md)では、記録と各サービスの動作ログを確認できます。

Proxy は上流との差異を通知しません。正規の値が記録される前にすり替えられた場合は防げず、正規の NAR が上流から提供されなくなった場合も復旧できません。

デモが終わったら、上流を通常状態に戻します。

```sh
docker compose exec upstream rm -f /control/tamper
```
