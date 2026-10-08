#!/bin/sh
# The one entrypoint. Every tool step runs with HOME and CARGO_HOME on the tmpfs: the root is read-only.
set -eu
export HOME=/tmp/home CARGO_HOME=/tmp/cargo CARGO_NET_OFFLINE=true
mkdir -p "$HOME" "$CARGO_HOME"
case "${1:-}" in
  solc) exec /opt/entrypoints/solc.sh ;;
  build-sbf) exec /opt/entrypoints/build-sbf.sh ;;
  rand-guest-build) exec /opt/entrypoints/rand-guest-build.sh "${2:-release}" ;;
  *) echo "usage: dispatch.sh solc|build-sbf|rand-guest-build [release|debug]" >&2; exit 64 ;;
esac
