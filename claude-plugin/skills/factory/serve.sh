#!/usr/bin/env bash
# Serves the factory dashboard for one factory root.
set -euo pipefail

root="${1:?usage: serve.sh <factory root>}"
here="$(cd "$(dirname "$0")" && pwd)"
config="$root/config.json"

if [ ! -f "$config" ]; then
  printf '{ "port": 8731 }\n' > "$config"
fi

cp "$here/dashboard.html" "$here/marked.min.js" "$here/purify.min.js" "$root/"

port="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["port"])' "$config")"

echo "Factory dashboard: http://localhost:$port/dashboard.html"
exec python3 "$here/serve.py" "$root" "$port"
