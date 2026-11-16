#!/usr/bin/env bash
# Run the API, the web app and the browser worker in one terminal, each line
# labelled with where it came from. Ctrl+C stops all three.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

docker compose -f "$ROOT/docker-compose.yml" up -d --wait

# Stop everything this script started when it ends.
trap 'trap - INT TERM EXIT; kill 0 2>/dev/null' INT TERM EXIT

start() { # label, folder, command...
  local label=$1 dir=$2
  shift 2
  (cd "$ROOT/$dir" && exec "$@") 2>&1 | sed -u "s/^/[$label] /" &
}

start api backend cargo run
start web web npm run dev
start worker browser-worker npm start

echo "Starting... open http://localhost:3000 once [web] says Ready. Ctrl+C stops everything."
wait
