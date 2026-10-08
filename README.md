# Rand Terminal

[![ci](https://github.com/randprotocol/randterminal.org/actions/workflows/ci.yml/badge.svg)](https://github.com/randprotocol/randterminal.org/actions/workflows/ci.yml)
[![coverage](https://codecov.io/gh/randprotocol/randterminal.org/graph/badge.svg)](https://codecov.io/gh/randprotocol/randterminal.org)

randterminal.org: a browser IDE for Ethereum and Solana developers. Paste Solidity or a Solana
program, watch it become an RV32IM image through `evm2rv` / `sbpf2rv`, and step through its run
on the Rand zkVM.

- `api/` — Rust service (axum). `cd api && cargo test`.
  Run it with `TERMINAL_SAMPLES=../samples cargo run -p terminal-api` from `api/`.
- `web/` — Next.js app. `cd web && npm ci && npm run dev`.
- `sandbox/` — the container image the untrusted tool steps run in.
- `circuits/` — pinned submodule supplying the transpilers and the zkVM.

Design: `docs/superpowers/specs/2026-10-08-terminal-design.md`.

## CI

`.github/workflows/ci.yml` (api, web, hermetic e2e, on-demand sandbox build) and `coverage.yml` check out the
private `circuits` submodule with the repository secret `CIRCUITS_TOKEN` (a fine-grained PAT with read access to
`dendisuhubdy/zkp-circuits`). Create it, and `CODECOV_TOKEN`, before CI can pass.

## Deploy

See `deploy/README.md`.
