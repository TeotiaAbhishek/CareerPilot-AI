#!/usr/bin/env bash
# One-time setup for CareerPilot on Ubuntu or WSL2. Safe to run again: it
# skips what's already done.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

say() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing: $1. $2" >&2
    exit 1
  fi
}

say "Checking tools"
need docker "Install Docker Desktop (turn on WSL integration) or Docker Engine."
need cargo "Install Rust from https://rustup.rs"
need node "Install Node.js 22.18 or newer, e.g. with nvm."
need ollama "Install it with: curl -fsSL https://ollama.com/install.sh | sh"
if ! node -e 'const [a, b] = process.versions.node.split(".").map(Number); process.exit(a > 22 || (a === 22 && b >= 18) ? 0 : 1)'; then
  echo "Node.js 22.18 or newer is needed. You have $(node -v)." >&2
  exit 1
fi
echo "docker, cargo, node $(node -v) and ollama found."

say "Starting Postgres in Docker"
docker compose up -d --wait

say "Settings files"
if [ ! -f backend/.env ]; then cp backend/.env.example backend/.env; echo "Created backend/.env"; else echo "backend/.env already there"; fi
if [ ! -f web/.env.local ]; then cp web/.env.example web/.env.local; echo "Created web/.env.local"; else echo "web/.env.local already there"; fi

say "Backend (the first build takes a few minutes)"
(cd backend && cargo build --release)
if ! grep -q '^VAULT_KEY=' backend/.env; then
  (cd backend && ./target/release/careerpilot-api vault-init)
  echo "Back up the VAULT_KEY line in backend/.env somewhere safe. Never commit or share it."
fi

say "Web app"
(cd web && npm ci && npm run build)

say "Browser worker"
(cd browser-worker && npm ci && npx playwright install chromium)

say "Local model"
if ollama list | grep -q '^qwen3:14b'; then
  echo "qwen3:14b already downloaded"
else
  ollama pull qwen3:14b
fi

say "Done"
echo "Run it now (Ctrl+C stops it):   scripts/dev.sh"
echo "Or keep it always on:            scripts/install-services.sh"
