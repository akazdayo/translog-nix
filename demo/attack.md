# コンパイラのすり替えデモ

正規の flake と C ソースを変えず、上流の substituter が偽コンパイラを返す状態でビルドします。[通常動作のデモ](normal.md)を先に完了し、正規の `gcc-wrapper` がログに記録されたことを確認してください。

リポジトリのルートで実行します。

```sh
# 上流の substituter が偽コンパイラを返すようにする
docker compose exec upstream touch /control/tamper
# 上流を直接使って、正規の flake をビルド・実行する
docker compose run --rm client run --option substituters http://upstream:8083 -L path:/demo#default
```

実行ファイルの標準出力が `HACKED! HACKED! HACKED!` になります。偽の GCC wrapper はコンパイル時に `puts` を書き換えるヘッダーを挿入し、出来上がるプログラムの動作を変えます。

このデモは、**信頼済みのデモ専用キャッシュ鍵が侵害された状況**を想定しています。偽物にはデモ鍵で有効な署名を付け、Nix は署名とハッシュを通常どおり検証します。cache.nixos.org の署名鍵は使いません。デモ鍵はリポジトリに公開されており、実環境には使えません。

すり替えを有効にしたまま[Proxy のデモ](protection.md)に進めます。ここで終了する場合は、次のコマンドで上流を通常状態に戻してください。

```sh
docker compose exec upstream rm -f /control/tamper
```
