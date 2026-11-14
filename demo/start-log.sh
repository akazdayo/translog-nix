#!/bin/sh
set -eu
mkdir -p /keys /data
if [ ! -s /keys/private ] || [ ! -s /keys/public ]; then
  keys=$(transparency -generate-key)
  printf '%s\n' "$keys" | sed -n 's/^Private: //p' > /keys/private
  printf '%s\n' "$keys" | sed -n 's/^Public: //p' > /keys/public
fi
exec transparency -listen 0.0.0.0:8082 -storage /data -private-key /keys/private
