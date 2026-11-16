#!/usr/bin/env bash
# Keep CareerPilot always on: the API, the web app and the browser worker run
# as systemd user services. They start when Ubuntu (WSL) starts and restart if
# they crash. Postgres already restarts with Docker (restart: unless-stopped).
#
#   scripts/install-services.sh            build, install and start
#   scripts/install-services.sh --remove   stop and remove the services
#
# After pulling new code, run it again: it rebuilds and restarts.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UNITS="$HOME/.config/systemd/user"
SERVICES=(careerpilot-api careerpilot-web careerpilot-worker)

if [ "${1:-}" = "--remove" ]; then
  systemctl --user disable --now "${SERVICES[@]}" 2>/dev/null || true
  for s in "${SERVICES[@]}"; do rm -f "$UNITS/$s.service"; done
  systemctl --user daemon-reload
  echo "Removed. Use scripts/dev.sh to run it by hand."
  exit 0
fi

if ! systemctl --user status >/dev/null 2>&1; then
  echo "systemd isn't running. In WSL, add [boot] systemd=true to /etc/wsl.conf, then run 'wsl --shutdown' in PowerShell." >&2
  exit 1
fi
if ! grep -q '^VAULT_KEY=' "$ROOT/backend/.env" 2>/dev/null; then
  echo "backend/.env has no VAULT_KEY yet. Run scripts/setup.sh first." >&2
  exit 1
fi

# systemd doesn't see nvm's PATH, so the services use node's full path.
NODE="$(command -v node)"
# The agent's Chromium window needs a display. WSLg provides :0.
DISPLAY_VALUE="${DISPLAY:-:0}"

echo "==> Building (the API in release mode, the web app for production)"
(cd "$ROOT/backend" && cargo build --release)
(cd "$ROOT/web" && npm run build)

mkdir -p "$UNITS"

cat > "$UNITS/careerpilot-api.service" <<UNIT
[Unit]
Description=CareerPilot API
After=network-online.target

[Service]
WorkingDirectory=$ROOT/backend
ExecStart=$ROOT/backend/target/release/careerpilot-api
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
UNIT

cat > "$UNITS/careerpilot-web.service" <<UNIT
[Unit]
Description=CareerPilot web app
After=careerpilot-api.service

[Service]
WorkingDirectory=$ROOT/web
ExecStart=$NODE $ROOT/web/node_modules/next/dist/bin/next start -H 127.0.0.1 -p 3000
Environment=NODE_ENV=production NEXT_TELEMETRY_DISABLED=1
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
UNIT

cat > "$UNITS/careerpilot-worker.service" <<UNIT
[Unit]
Description=CareerPilot browser worker
After=careerpilot-api.service

[Service]
WorkingDirectory=$ROOT/browser-worker
ExecStart=$NODE src/main.ts
Environment=DISPLAY=$DISPLAY_VALUE
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
UNIT

systemctl --user daemon-reload
systemctl --user enable "${SERVICES[@]}"
systemctl --user restart "${SERVICES[@]}"

# Without "linger", user services stop when you close your last terminal.
if [ "$(loginctl show-user "$USER" --property=Linger --value 2>/dev/null)" != "yes" ]; then
  echo "==> Letting your services run without an open terminal (asks for your password)"
  sudo loginctl enable-linger "$USER"
fi

echo
echo "Running. Open http://localhost:3000"
echo "Status:  systemctl --user status ${SERVICES[*]}"
echo "Logs:    journalctl --user -u careerpilot-api -f"
