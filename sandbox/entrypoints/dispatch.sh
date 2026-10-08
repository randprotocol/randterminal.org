#!/bin/sh
# The one entrypoint. Every tool step runs with HOME and CARGO_HOME on the tmpfs: the root is read-only.
#
# Ownership: the container runs as uid 10001, and the bind mount keeps that uid on whatever it
# creates, while the API on the host runs as another user (`terminal`). That user can neither
# chmod nor delete what it does not own, so everything this step leaves in the mount must be
# group/world-writable: the next step (the -g debug build) re-opens the tree, and the sweeper
# deletes jobs' target/ trees (a directory's entries can only be removed by someone who may
# write the directory). Hence `umask 000` for every file the tools create, and a closing
# `chmod -R a+rwX` over the mount for files created with explicit modes (tar, cargo, rustc).
# rand-guest forces `--target-dir <shim>/target` (it scrubs CARGO_TARGET_DIR), so the target
# tree cannot be moved onto the tmpfs instead.
set -eu
umask 000
export HOME=/tmp/home CARGO_HOME=/tmp/cargo CARGO_NET_OFFLINE=true
mkdir -p "$HOME" "$CARGO_HOME"
case "${1:-}" in
  solc) W=/work; set -- /opt/entrypoints/solc.sh ;;
  build-sbf) W=/work; set -- /opt/entrypoints/build-sbf.sh ;;
  rand-guest-build) W=/circuits/jobs/job; set -- /opt/entrypoints/rand-guest-build.sh "${2:-release}" ;;
  *) echo "usage: dispatch.sh solc|build-sbf|rand-guest-build [release|debug]" >&2; exit 64 ;;
esac
st=0
"$@" || st=$?
# Entries the host created (the mount root, the shim sources) are not ours to chmod: stay quiet.
chmod -R a+rwX "$W" 2>/dev/null || true
exit "$st"
