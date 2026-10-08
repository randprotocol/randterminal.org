# Rand Terminal

randterminal.org: a browser IDE for Ethereum and Solana developers. Paste Solidity or a Solana
program, watch it become an RV32IM image through `evm2rv` / `sbpf2rv`, and step through its run
on the Rand zkVM.

- `api/` — Rust service (axum). `cd api && cargo test`.
  Run it with `TERMINAL_SAMPLES=../samples cargo run -p terminal-api` from `api/`.
- `web/` — Next.js app. `cd web && npm ci && npm run dev`.
- `sandbox/` — the container image the untrusted tool steps run in.
- `circuits/` — pinned submodule supplying the transpilers and the zkVM.

Design: `docs/superpowers/specs/2026-10-08-terminal-design.md`.
