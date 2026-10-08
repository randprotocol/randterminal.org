#!/bin/sh
# /work/src/{lib.rs,Cargo.toml} → /work/program.so. Only the vendored crates resolve (no network).
set -eu
rm -rf /tmp/crate && cp -r /opt/solana-template /tmp/crate
cp /work/src/lib.rs /tmp/crate/src/lib.rs
[ -f /work/src/Cargo.toml ] && cp /work/src/Cargo.toml /tmp/crate/Cargo.toml
cd /tmp/crate
# HOME stays on the build-time tree: a different cache path makes cargo-build-sbf re-link the
# rustup toolchain, which writes the read-only /opt/rustup. CARGO_HOME is still the tmpfs.
if ! HOME=/opt/sbf-home cargo-build-sbf --tools-version v1.54 --offline --sbf-out-dir /tmp/out -- --message-format=json; then
  echo "build failed. The sandbox has no network; only these crates are available:" >&2
  ls /opt/solana-vendor | sed 's/^/  /' >&2
  exit 1
fi
set -- /tmp/out/*.so
[ "$#" -eq 1 ] && [ -f "$1" ] || { echo "expected exactly one .so in the output, got: $*" >&2; exit 65; }
cp "$1" /work/program.so
