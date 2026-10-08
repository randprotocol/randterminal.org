# Rand Terminal — design

**Date:** 2026-10-08
**Status:** approved in conversation; awaiting written review
**Lives at:** randterminal.org
**Repo:** `randprotocol/terminal`, mounted at `terminal/` in the monorepo

## 1. Purpose

Rand Terminal is a browser IDE, in the spirit of Remix, for Ethereum and Solana
developers. A developer pastes Solidity or Solana Rust and, without rewriting a
line for Rand, watches it become an RV32IM image and run on the Rand zkVM.

It shows five things, in order:

1. the EVM bytecode (or the Solana compiled ELF) the familiar toolchain produces;
2. what that compiled artifact looks like to our transpiler (opcodes, blocks, functions, syscalls);
3. the transpiling process: block graph, warnings, generated C with a per-opcode map;
4. the end result: the RV32IM image, its disassembly, `hc`, program id, size against the chain cap;
5. the execution on the zkVM, visually: a cycle-level debugger and the STARK tables it fills.

Success means a Solidity or Solana developer with no Rand knowledge can load the
ERC-20 or SPL Token sample, press Run, and understand where every cycle came from.

## 2. Scope

**In v1**

- Solidity source → solc 0.8.37 → `evm2rv` → `rand-guest build` → emulator trace.
- Solana Rust source → `cargo build-sbf` (platform-tools v1.54) → `sbpf2rv` → `rand-guest build` → trace.
- Uploaded Solana `.so` (skips the build).
- Call construction forms: ABI function + args + caller + initial storage for EVM; accounts + instruction data for Solana.
- Cycle-level debugger with a three-level source map (source opcode ↔ C line ↔ RV32 pc).
- STARK table fill view and tier placement. No proving.
- Bundled samples (ERC-20, Counter, SPL Token, a minimal Solana counter program).

**Out of v1** (not designed here): deploy and invoke on chain, proving, accounts,
saved workspaces, multiple solc versions, Anchor projects, multi-file Solidity imports
beyond what solc's standard-json input can resolve from the pasted files.

## 3. Architecture

Approach chosen: an axum backend that links the circuits crates in-process, with the
untrusted tool invocations (solc, cargo-build-sbf, rand-guest build) in a locked-down
container. The frontend is a Next.js app that talks only to this backend.

```
browser ──HTTPS──▶ nginx ──▶ web  (Next 15.5 standalone, :3002)
                         └──▶ api  (terminal-api, axum, :8080)
                                 ├─ in-process: evm2rv, sbpf2rv, rand_zkvm (emulator, tier maths)
                                 ├─ jobs on disk: $TERMINAL_DATA/jobs/<id>/
                                 └─ docker run terminal-sandbox:<tag>  (solc | cargo build-sbf | rand-guest build)
```

### 3.1 Repository layout

```
terminal/
  README.md
  web/                      Next 15.5 App Router, React 19, TypeScript 5.9, Tailwind 4, CodeMirror 6
  api/                      Rust 1.98.1 cargo workspace
    Cargo.toml
    rust-toolchain.toml     1.98.1
    crates/
      terminal-core/        pipeline, stage views (DTOs), source map, compact trace, tier maths
      terminal-sandbox/     container runner: image name, limits, timeouts, stdout/stderr capture
      terminal-api/         axum routes, state, errors, rate limit, SSE, job store, samples
  sandbox/
    Dockerfile              debian:bookworm + rustup 1.98.1 (riscv32im target) + clang 23.1.1 + solc 0.8.37 + platform-tools v1.54
    entrypoints/            solc.sh, build-sbf.sh, rand-guest-build.sh (one fixed command each)
  samples/                  ERC20.sol, Counter.sol, spl_token.so (+ .sha256), solana-counter/ (Rust source)
  deploy/                   deploy.sh, nginx.conf, terminal-web.service, terminal-api.service
  docs/superpowers/specs/   this file
  .github/workflows/        ci.yml (web + api), coverage.yml (pinned SHAs, Codecov)
  circuits/                 git submodule, pinned; supplies evm2rv, sbpf2rv, research, rand-guest, guests-compiled, evm-rt, sbpf-rt
```

`circuits` is a submodule so the transpilers' `../research` and `../guests-compiled`
path dependencies resolve unchanged. The api crates depend on them by path
(`../../circuits/evm2rv`, etc.). The sandbox image is built from the same pinned
checkout, so the `hc` an IDE user sees equals the `hc` the chain would compute.

### 3.2 Why in-process

Both transpilers expose the intermediate forms stage 2 and 3 need as plain Rust
values: `evm2rv::blocks::{blocks, jumpdests, warnings, static_gas}` and
`evm2rv::emit::{translate, op_c, mnemonic}`; `sbpf2rv::scan::try_scan` and
`sbpf2rv::emit::emit_program`. The emulator `rand_zkvm::emulator::execute` returns
`Execution { events: Vec<CycleEvent>, outputs, halted }`. Shelling out to the CLIs
would lose all of this. None of these types derive serde; terminal-core owns its
own DTOs and maps into them, so circuits needs no changes.

## 4. The pipeline

A **job** is one submission. It has a language, a source (text or bytes), options,
and up to four **stages**. Each stage writes its artifacts under
`$TERMINAL_DATA/jobs/<job_id>/<stage>/` and a `view.json` the API serves verbatim.
Stages run sequentially in a tokio task; status is `queued | running | done | failed`.

The sandbox runs untrusted code (a Solana job controls its `Cargo.toml`, so build scripts run in
the container). Each tool step therefore mounts only a dedicated `<stage>/sandbox/` directory and
nothing else:

```
jobs/<id>/compile/sandbox/   solc: input.json -> output.json
                             build-sbf: src/lib.rs, src/Cargo.toml -> program.so
jobs/<id>/build/sandbox/     shim/ (generated crate), shim-dbg/ (the -g copy)
                             -> image.bin, image-dbg.bin, debug-line.txt
```

After the step the host copies exactly the outputs it expects out of `sandbox/` (regular files
only, opened without following links, size-capped) into host-owned paths in the stage directory
(`compile/runtime.hex`, `compile/program.so`, `build/image.bin`, ...). The container never sees
those paths, and the API serves only the host-owned copies.

| Stage | Solidity | Solana Rust | Solana `.so` |
|---|---|---|---|
| compile | solc (sandbox) → `creation.hex`, `runtime.hex`, `abi.json` | cargo build-sbf (sandbox) → `program.so` | copy upload → `program.so` |
| transpile | `evm2rv::emit::translate` → `contract.c`, blocks view | `sbpf2rv` scan + emit → `program.c`, functions view | same |
| build | `rand-guest build` (sandbox) on the shim crate → `image.bin`, plus the `-g` side build (`image-dbg.bin`, `debug-line.txt`) | same | same |
| run | emulator in-process, on demand, many runs per job | same | same |

### 4.1 Compile

**Solidity.** The sandbox runs solc with standard-json input built by terminal-core:
`--optimize --optimize-runs 200 --evm-version shanghai`, outputs `abi`,
`evm.bytecode.object`, `evm.deployedBytecode.object`, `metadata`. These are the
flags `transpiled/ethereum/SOLC.md` and `fullnode/ci.yml` already pin, so the ERC-20
sample reproduces `erc20.runtime.hex` byte for byte. The user picks the contract to
transpile when a file defines several. Diagnostics (`errors[]` with
`sourceLocation`) are mapped to editor markers.

**Solana Rust.** The pasted source is written as `src/lib.rs` of a template crate
(`solana-program` pinned to the version platform-tools v1.54 ships against, `crate-type
= ["cdylib", "lib"]`). A `Cargo.toml` tab is editable so users can add dependencies;
the sandbox has a warmed vendor directory for `solana-program` and a few common crates and
runs `cargo build-sbf --offline`. A dependency not in the vendor set fails with a
clear message listing what is available. The `.so` goes to `program.so`.

**Compile view** (`GET /jobs/:id/compile`):

- EVM: `creation_hex`, `runtime_hex`, `abi`, `size_bytes`, and `opcodes[]` of
  `{pc, opcode, mnemonic, push_hex?, static_gas, jumpdest}` from `evm2rv::blocks`.
- Solana: `elf` summary (`entry_pc`, text size, sections), `functions[]` from the scan
  with `{entry, blocks: [{start, end, insns: [{pc, text}]}]}`, `syscalls[]`
  (name, count, supported), `warnings[]`.

### 4.2 Transpile

In-process. Options: `stage` (two by default, one selectable for teaching), `chain_id`
(default 20, the fleet chain; required if the code uses CHAINID).

**Transpile view** (`GET /jobs/:id/transpile`):

- `warnings[]`: `Trap{pc,opcode}`, `Call{pc,opcode}`, `CodeTooLarge`,
  `UnknownSyscall`, `Cpi`, … with a one-line explanation each (what the zkVM does
  instead: trap and revert, unsupported, …).
- `graph`: nodes = blocks `{id, start, end, term, static_gas, min_depth, max_growth}` (EVM)
  or `{function, start, end, term}` (sBPF); edges from `Term`.
- `c_source`: the generated C, and `op_map[]`: `{pc, c_line}` parsed from the
  `/* 0xPC MNEMONIC */` markers evm2rv emits per opcode and the `L_<pc>:` labels
  sbpf2rv emits per block (sBPF maps at block granularity; the per-instruction C
  follows in order and is attributed by position within the block).
- `stats`: blocks, opcodes/instructions, locals, C lines.

### 4.3 Build

The sandbox runs `rand-guest build <shim-dir> --lang c --out image.bin` with the
pinned clang; stdout gives words, `hc`, program id. `rand-guest check` reports fit
against the cap. The job keeps `guest.elf` and `image.bin`.

**Source map.** Production builds carry no debug info and the flag set is pinned for
`hc` reproducibility, so the build stage runs a second, side build of the same crate
with `-g` added (via `CFLAGS` the shim's `cc` invocation honours, verified in the
plan). If the side build's text words equal the production image's, its DWARF line
table (`llvm-dwarfdump --debug-line`) yields `pc → contract.c line`; composed with
`op_map` that gives `pc → source opcode`. If the words differ, the pc-level map is
dropped and the view says so; the opcode-to-C map still works.

**Image view** (`GET /jobs/:id/image`):

- `words`, `text_words`, `data_words`, `prologue_words`, `base_pc`, `hc`, `program_id`,
  `cap_words` (`TERMINAL_MAX_PROGRAM_WORDS`, default 65 535), `fits`.
- `disasm[]`: `{pc, word, text}` from `rand_zkvm::isa` decode plus a formatter in
  terminal-core (RV32IM mnemonics with ABI register names, ecall annotated with the
  syscall name when a7 is statically known from the preceding `li`).
- `source_map`: `{pc_to_c: [[pc, line]], c_to_src: [[line, pc]]}` or `null` with a reason.

Reference sizes, so the UI is designed for them: ERC-20 is 11 686 words; SPL Token is
65 096 words (22 371 C lines). The default 4 096-word deploy cap does not fit either,
which the view shows plainly against the configured cap.

### 4.4 Run

`POST /jobs/:id/runs` with a **call**. terminal-core builds the input vectors with the
existing builders in `rand_zkvm`:

- EVM: `EvmCall { code, calldata, address, caller, callvalue, gas_limit, tree, touched }`.
  The form offers the ABI's functions; args are ABI-encoded by `rand_zkvm::evm::abi_call`
  (extended for `address`, `uintN`, `bool`, `bytes32`; other types are rejected with a message).
  Initial storage is a list of `(slot, value)` with helpers for `mapping(address)` and
  `mapping(address => mapping(address))` slots. `touched` is auto-derived by a dry run
  of the `evm-core` interpreter on the host (it records every SLOAD/SSTORE slot), so the
  user never writes witnesses by hand.
- Solana: `SbpfCall { elf, input }`, where `input` is the serialized instruction built
  from the accounts form (`key, owner, lamports, data, is_signer, is_writable`) and
  instruction data, using `sbpf_core::abi`'s encoder exactly as `spl_transfer` does.

Execution runs `rand_zkvm::emulator::execute(program, &inputs, &public, max_cycles)` in
`spawn_blocking` under a global semaphore (`TERMINAL_MAX_CONCURRENT_RUNS`, default 4) with
`max_cycles` capped at `2^20 - 1` (tier 20, the largest admitted) and a wall-clock
cap of 60 s. The raw `Execution` is **not** retained: 766 k `CycleEvent`s with their
`Vec`s is hundreds of MB. It is compacted once into:

| file | record | purpose |
|---|---|---|
| `cycles.bin` | 40 B fixed: `clk, pc, next_pc, word, rd, flags, n_alu, rd_val, mem_addr, mem_val, sys_idx, mem_old` (`mem_val` = the word a load read or a store wrote; `mem_old` = the word a store replaced) | the timeline; 766 k cycles ≈ 31 MB |
| `regs.ckpt` | 32×u32 every 1 024 cycles | O(1) register state at any cycle (replay ≤ 1 023 records) |
| `memwrites.bin` | 12 B: `clk, addr, value` per RAM write | memory state at any cycle by folding writes ≤ clk |
| `syscalls.json` | `[{clk, kind, detail}]` | markers and the syscall log |
| `summary.json` | see below | the overview and the table view |

`summary.json`: `cycles`, `halted`, `trap?`, `outputs[8]`, `tier` (from
`Tier::for_workload(cycles + digest_rows, permutations)`), per-table row counts
(`cpu = cycles`, `memory = Σ accesses`, `alu = Σ alu events`, `poseidon2 = Σ hash rows +
program/input/public digest rows`, `keccak`, `sha256`), the capacity of every tier for
each table (`cpu_height`, `alu_height`, `poseidon2_height`, `min_mem_log_height`,
keccak/sha max heights), syscall counts by kind, and `hot_pcs` (top 32 pcs by hit count).

Run endpoints: `GET /runs/:rid` (summary), `GET /runs/:rid/cycles?from&count` (≤ 4 096
records, JSON), `GET /runs/:rid/state?at=N` (registers at N, the cycle record, the
instruction's source-map entry), `GET /runs/:rid/memory?at=N&addr=A&words=W` (W ≤ 256),
`GET /runs/:rid/syscalls`.

Reference workloads: ERC-20 `transfer` 66 235 cycles (tier 18), `approve` 48 119
(tier 16); SPL Token `transfer` 765 851 cycles (tier 20). All of these emulate in well
under the 60 s cap on a laptop.

## 5. The web app

Single route `/`, three columns, Remix-shaped:

- **Left rail**: Samples, Files (the editable tabs: `Contract.sol`, or `lib.rs` + `Cargo.toml`), and
  the pipeline strip: five numbered stages with status chips. Clicking a stage focuses its panel.
- **Centre**: CodeMirror 6 with `@replit/codemirror-lang-solidity` and `@codemirror/lang-rust`,
  diagnostics as gutter markers and inline messages. Language picker: Solidity / Solana (Rust) /
  Solana (.so upload).
- **Right**: the stage panel for the focused stage. **Bottom**: a console with the raw
  stdout/stderr of every tool run, collapsible.

Stage panels:

1. **Compile** — hex (creation/runtime toggle, copy), ABI table; or ELF summary + sBPF listing.
2. **Analyse** — the opcode/instruction listing with pc, gas, jumpdest marks, blocks shaded.
3. **Transpile** — block graph (inline SVG, dagre-free layered layout computed in a worker;
   blocks as nodes, `Term` edges), warnings list, and a split C viewer: click an opcode on the
   left, the C line highlights on the right and vice versa.
4. **Image** — stats card (words vs cap, hc, program id), RV32IM disassembly with the source
   map column, search by pc.
5. **Run** — the call form (functions from the ABI / accounts + data), Run button, and the
   **debugger**:
   - timeline scrubber across all cycles with syscall tick marks and the current clk;
     step ±1, ±100, to next syscall, to next source opcode;
   - listing panel: RV32 disassembly around pc, the C line above it, the source opcode above that;
   - registers `x0..x31` with ABI names, changed-since-last-step highlighting;
   - memory panel: the cycle's access, plus a 64-word window at an address;
   - syscall log and the eight output words;
   - **tables**: one bar per STARK table (cpu, memory, alu, poseidon2, keccak, sha256) showing
     rows used against the capacity of the selected tier, a tier selector (10 … 20) that greys
     the tiers the run does not fit, and a one-line explanation of how each table is fed from
     the cycle (cpu row per cycle, memory row per access, alu row per ALU event, poseidon2 rows
     per hash and digest).

Theme: the ink dark theme from `clients/design/tokens.json` (bg `#0E1220`, surface
`#161C2C`, text `#ECE9E2`), cobalt accent `#5B8DEF` as on randprotocol.org, Inter for UI
and JetBrains Mono for code, both self-hosted through `next/font`. Light theme under
`html[data-theme="light"]`. Charts and bars follow the dataviz skill's palette rules.

State: React context per job, plain `fetch` with a small hook layer, SSE for stage
progress. No state library, matching the sibling apps.

## 6. API

Base `/api/v1`. JSON everywhere; errors are `{error, message, details?}`.

| method | path | notes |
|---|---|---|
| GET | `/samples` | `[{id, title, lang, files}]` |
| GET | `/samples/:id` | file contents (text or base64 for `.so`) |
| POST | `/jobs` | `{lang: "solidity" \| "solana-rust" \| "solana-elf", files: {name: text\|base64}, options: {contract?, chain_id?, stage?}}` → `{id}` 201 |
| GET | `/jobs/:id` | `{lang, stages: {compile, transpile, build}: {status, started, ended, error?}, console: [...]}` |
| GET | `/jobs/:id/events` | SSE: `stage` events as statuses change, `console` lines, `done` |
| GET | `/jobs/:id/compile` `/transpile` `/image` | stage views (§4) |
| GET | `/jobs/:id/files/:name` | raw artifacts: `contract.c`, `image.bin`, `program.so`, `runtime.hex` |
| POST | `/jobs/:id/runs` | the call (§4.4) → `{rid}` 201; runs synchronously up to the cap |
| GET | `/runs/:rid` `/cycles` `/state` `/memory` `/syscalls` | §4.4 |
| GET | `/health` | `{ok, sandbox_image, circuits_rev}` |

Limits: source files ≤ 512 KB total, `.so` ≤ 2 MB, one job per POST, 10 jobs per
minute per IP and 60 runs per minute per IP (randscan's limiter pattern), 2 concurrent
sandbox builds globally (`TERMINAL_MAX_CONCURRENT_BUILDS`). Jobs and runs expire one
hour after last access; a disk cap (`TERMINAL_DATA_CAP_GB`, default 20) evicts
least-recently-used jobs first.

## 7. Sandbox

`terminal-sandbox` runs one fixed entrypoint per stage with the job's stage directory
bind-mounted as the only writable path:

```
docker run --rm --network none --memory 4g --memory-swap 4g --cpus 2 --pids-limit 512 \
  --read-only --tmpfs /tmp:size=1g --security-opt no-new-privileges --cap-drop ALL \
  --user 10001:10001 -v <job>/<stage>:/work terminal-sandbox:<tag> <entrypoint>
```

Timeouts: solc 30 s, build-sbf 300 s, rand-guest build 180 s (SPL Token's 22 k-line C
is the sizing case); on timeout the container is killed and the stage fails with
`timeout`. The runner captures stdout and stderr (each ≤ 1 MB) for the console. The
image is built in CI from `sandbox/Dockerfile` and pinned by digest in the api config.
The runner is a trait so tests use a fake, and `podman` works through the same CLI.

The api process itself runs as a dedicated user in the `docker` group under systemd;
it never executes user-supplied code in-process. The emulator is the only user-driven
in-process work and is bounded by cycles, time and the semaphore.

## 8. Errors

- `terminal_api::AppError` (thiserror): `BadRequest`, `NotFound`, `PayloadTooLarge`,
  `TooManyRequests{retry_after}`, `StageFailed{stage, kind}`, `Internal`; `IntoResponse`
  returns JSON. Internal errors are logged and returned without detail.
- Stage failures are data, not HTTP errors: the job view carries `{status: "failed",
  error: {kind: "compile" | "transpile" | "build" | "timeout" | "unsupported", message,
  diagnostics: [{file, line, col, severity, message}]}}`. solc's `errors[]` and rustc's
  `--message-format=json` are parsed into `diagnostics`; transpiler `Warning`s that are
  fatal (`CodeTooLarge`, `Cpi`) become `unsupported` with an explanation.
- Run traps (`ExecError`) are reported in the run summary with the trapping clk, so the
  debugger can jump there.

## 9. Testing

- **terminal-core** (always run): views built from the checked-in fixtures
  `circuits/guests-compiled/evm/contracts/erc20.runtime.hex` and
  `circuits/guests-compiled/sbpf/programs/spl_token.so` — opcode listing counts
  (74 blocks, 798 opcodes, 51 jumpdests), `op_map` round-trips every marker, the compact
  trace round-trips a synthetic `Execution`, register replay equals the emulator's state
  at random cycles, table row counts match `build_traces` log heights on a small guest
  (`guests-compiled/bin/fib.bin`), tier selection matches `rand-guest run`'s printed tier.
- **terminal-api**: router tests with `tower::ServiceExt::oneshot` and a fake sandbox
  runner that returns the fixtures' `image.bin` from `transpiled/`; limits, expiry and
  error JSON shapes.
- **Sandbox integration** (gated, skips with `eprintln!("skipping: TERMINAL_SANDBOX unset")`):
  compile `ERC20.sol`, assert the runtime hex equals the fixture; build and assert `hc`
  equals `transpiled/ethereum/out/info.txt`'s; same for SPL Token; build the Solana counter
  sample from source.
- **web**: vitest for the formatting helpers and the debugger's replay hook; Playwright
  smoke against a recorded API (load ERC-20 sample, run `transfer`, step to the first
  syscall, read the tier). An env-gated e2e against a live api.
- **CI**: `ci.yml` runs api tests and web lint/test/build; `coverage.yml` copies
  randbridge's pinned-SHA cargo-llvm-cov → Codecov job.

## 10. Deployment

One droplet, same shape as randbridge.org: `deploy/deploy.sh <ip>` rsyncs the tree,
builds web (standalone) and api on the box, builds or pulls the sandbox image, installs
`terminal-web.service`, `terminal-api.service` and `nginx.conf` (randterminal.org, TLS by
certbot, CSP, `/api/` proxied to :8080, SSE buffering off). Config by environment:
`TERMINAL_DATA`, `TERMINAL_SANDBOX_IMAGE`, `TERMINAL_MAX_PROGRAM_WORDS`,
`TERMINAL_MAX_CONCURRENT_BUILDS`, `TERMINAL_MAX_CONCURRENT_RUNS`, `TERMINAL_DATA_CAP_GB`,
`API_HOST`, `API_PORT`, `TRUST_PROXY`.

## 11. Decisions recorded

- Hosted publicly at randterminal.org; sandboxing is therefore mandatory in v1.
- Solana input is both pasted Rust (built in the sandbox) and uploaded `.so`.
- zkVM view is the step debugger plus table/tier view; no proving in v1.
- Own repo + submodule, like every sibling app.
- In-process transpilers over CLI shelling, for the intermediate forms.
- Compact trace on disk rather than retaining `Execution`, for memory.
- Side `-g` build for the pc-level source map, never touching the production flags, so `hc` stays reproducible.
- CodeMirror 6 over Monaco: smaller, no worker bundling pain in Next, Solidity and Rust modes exist.
