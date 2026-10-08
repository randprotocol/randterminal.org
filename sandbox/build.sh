#!/usr/bin/env bash
# Build the sandbox image tagged by sandbox/tag.sh (circuits revision + hash of sandbox/).
set -euo pipefail
cd "$(dirname "$0")/.."
TAG=$(sandbox/tag.sh)
docker build --platform linux/amd64 -f sandbox/Dockerfile -t "$TAG" .
echo "$TAG"
