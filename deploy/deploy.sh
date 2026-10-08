#!/usr/bin/env bash
# Build and (re)start randterminal.org on the droplet. Usage: deploy/deploy.sh <droplet-ip>
set -euo pipefail
IP="${1:?usage: deploy/deploy.sh <droplet-ip>}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
SSH="ssh -o StrictHostKeyChecking=accept-new -o ServerAliveInterval=30 terminal@$IP"
SRC=/opt/terminal/src
REV=$(git -C "$HERE/circuits" rev-parse --short=7 HEAD)
rsync -az --delete --exclude target --exclude node_modules --exclude .next --exclude data --exclude 'web/e2e/fixtures' \
  "$HERE/" "terminal@$IP:$SRC/"

# shellcheck disable=SC2086
$SSH REV="$REV" bash -se <<'REMOTE'
set -euo pipefail
source "$HOME/.cargo/env"
cd /opt/terminal/src
docker image inspect "terminal-sandbox:$REV" >/dev/null 2>&1 || sandbox/build.sh
(cd api && cargo build --release -p terminal-api)
mkdir -p /opt/terminal/bin /var/lib/terminal/.docker
install -m 0755 api/target/release/terminal-api /opt/terminal/bin/terminal-api.new
rsync -a --delete samples/ /opt/terminal/samples/
cd web
npx --yes npm@11 ci
rm -rf .next
npm run build
rm -rf /opt/terminal/web.new
mkdir -p /opt/terminal/web.new
cp -r .next/standalone/. /opt/terminal/web.new/
cp -r .next/static /opt/terminal/web.new/.next/static
[ -d public ] && cp -r public /opt/terminal/web.new/public || true
mv /opt/terminal/bin/terminal-api.new /opt/terminal/bin/terminal-api
rm -rf /opt/terminal/web.old && { [ -d /opt/terminal/web ] && mv /opt/terminal/web /opt/terminal/web.old || true; }
mv /opt/terminal/web.new /opt/terminal/web
grep -q "TERMINAL_SANDBOX_IMAGE=terminal-sandbox:$REV" /etc/terminal/api.env || echo "WARNING: /etc/terminal/api.env names a different sandbox image than terminal-sandbox:$REV"
sudo systemctl restart terminal-api terminal-web
for _ in $(seq 15); do
  curl -fsS http://127.0.0.1:8080/api/v1/health && curl -fsS -o /dev/null http://127.0.0.1:3002/ && { echo; echo "web: up"; exit 0; }
  sleep 2
done
echo "health check failed after 30 s" >&2
exit 1
REMOTE
