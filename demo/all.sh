# normal
docker compose up --build -d
docker compose exec upstream rm -f /control/tamper
## 後のデモで使う偽コンパイラを準備する
docker compose run --rm --build prepare-attack
## 正規のコンパイラで flake をビルドし、実行する
docker compose run --rm client run --option substituters http://proxy:8081 -L path:/demo#default

# attack
## 準備
## 上流の substituter が偽コンパイラを返すようにする
docker compose exec upstream touch /control/tamper

## ビルド実行する
docker compose run --rm client run --option substituters http://upstream:8083 -L path:/demo#default

# シェルを開く
docker compose run --rm --entrypoint sh client

# narinfoの比較
## proxy済
curl -i http://localhost:18081/mqg95s149wr7az0xbn6d6s45m76pzz8n.narinfo

## 直upstream
curl -i http://localhost:18083/mqg95s149wr7az0xbn6d6s45m76pzz8n.narinfo

# ログリセット
./demo/reset-log.sh

# 通常ハッシュ sha256:124ljwp69ybd7dlshzyzky6s6gz5f0isrffz8kkh6nrkml138qxh
# 攻撃ハッシュ sha256:178km1zhphd77bah2d7am10siw2dd5p6j2yv272limbpnqmbp7zf

# 対応できる攻撃
# - ログの改ざん
# - substituterのすり替え
