#!/usr/bin/env bash
# Build the sandbox image tagged with the circuits revision it embeds.
set -euo pipefail
cd "$(dirname "$0")/.."
REV=$(git -C circuits rev-parse --short=7 HEAD)
docker build --platform linux/amd64 -f sandbox/Dockerfile -t "terminal-sandbox:$REV" .
echo "terminal-sandbox:$REV"
