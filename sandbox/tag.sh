#!/usr/bin/env bash
# Prints the sandbox image tag: terminal-sandbox:<circuits rev>-<first 8 hex of a sha256 over
# every file under sandbox/ (Dockerfile, entrypoints, template, these scripts)>. A change to an
# entrypoint therefore gets a new tag, and deploy.sh rebuilds instead of reusing a stale image.
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v sha256sum >/dev/null 2>&1; then SHA=(sha256sum); else SHA=(shasum -a 256); fi
REV=$(git -C circuits rev-parse --short=7 HEAD)
# One "<sha256>  ./<path>" line per file (names count too), byte-sorted, hashed again.
HASH=$(cd sandbox && find . -type f ! -path '*/target/*' -print0 | LC_ALL=C sort -z | xargs -0 "${SHA[@]}" | "${SHA[@]}" | cut -c1-8)
echo "terminal-sandbox:$REV-$HASH"
