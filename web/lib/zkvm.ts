// The /zkvm walkthrough: its steps, the arithmetic it shows, and the real numbers it shows them on.
// Every figure below is copied from the code or from a recorded run (sources inline); none is made up.

export type StepId = 'source' | 'compile' | 'transpile' | 'image' | 'execute' | 'tables' | 'air' | 'commit' | 'fri' | 'verify';
export interface Step { id: StepId; title: string; token: (s: SampleFacts) => string }

export const STEPS: Step[] = [
  { id: 'source', title: 'Source', token: (s) => s.sourceName },
  { id: 'compile', title: 'Compiler', token: (s) => s.compiled },
  { id: 'transpile', title: 'Transpile to C', token: (s) => `${group(s.cLines)} lines of C` },
  { id: 'image', title: 'RV32IM image', token: (s) => `${group(s.words)} words` },
  { id: 'execute', title: 'Execute', token: (s) => `${group(s.cycles)} cycles` },
  { id: 'tables', title: 'Trace tables', token: (s) => `tier ${s.tier}` },
  { id: 'air', title: 'Constraints', token: () => 'every row checked' },
  { id: 'commit', title: 'Commit', token: () => 'Merkle roots' },
  { id: 'fri', title: 'FRI folding', token: (s) => `${friFolds(largestLogHeight(s)).length - 1} folds` },
  { id: 'verify', title: 'Verify', token: () => 'accept' },
];

/** Moves `delta` steps from `i`, clamped to the walkthrough. */
export const stepBy = (i: number, delta: number, n = STEPS.length) => Math.max(0, Math.min(n - 1, i + delta));
/** Autoplay's next step, or null at the end (autoplay stops there rather than wrapping). */
export const autoNext = (i: number, n = STEPS.length): number | null => (i + 1 < n ? i + 1 : null);

export const group = (n: number) => n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
export const nextPow2 = (n: number) => { let p = 1; while (p < n) p *= 2; return p; };
export const log2 = (n: number) => Math.round(Math.log2(n));

// ---- the machine's parameters (circuits/research/src/machine.rs at the pinned 85b6f46) --------
// Re-read at 85b6f46: TIERS, Tier's heights, FriParameters and poseidon2::BLOCK are as they were at
// 9bf3844. The FRI rate-1/4 profile (92 queries, 24 bits) is the recursion machine's own
// (`recursion::machine::RvmFri`); `FriProfile::Production` still sizes every program's proof.

/** `machine::TIERS`: a tier `t` fixes the CPU table at 2^t rows. */
export const TIERS = [10, 12, 14, 16, 18, 20] as const;
/** `FriParameters` in `generic_config` (log_blowup 3, max_log_arity 3, log_final_poly_len 0), and
 *  `FriProfile::Production` (80 queries, 20 query PoW bits; machine.rs's own test pins both). */
export const FRI = { logBlowup: 3, maxLogArity: 3, logFinalPolyLen: 0, queries: 80, powBits: 20 } as const;
/** `poseidon2::BLOCK` (circuits/research/src/tables/poseidon2.rs): rows per Poseidon2 permutation. */
export const POSEIDON2_BLOCK = 32;

/** `Tier::for_workload`: the smallest tier whose CPU table holds the cycles (2^t − 1, one row is
 *  the halt's successor) and whose Poseidon2 table (2^(t+2) rows) holds the permutations. */
export function fittingTier(cyclesTotal: number, poseidon2Perms: number): number | null {
  return TIERS.find((t) => cyclesTotal <= 2 ** t - 1 && poseidon2Perms <= 2 ** (t + 2) / POSEIDON2_BLOCK) ?? null;
}

export interface TableBar { name: string; used: number; height: number; what: string }

/** Each table's height at tier `t` (`Tier::cpu_height`, `alu_height`, `min_mem_log_height`,
 *  `poseidon2_height`) beside the rows the run used: the gap is padding. */
export function tableBars(rows: Rows, t: number): TableBar[] {
  return [
    { name: 'CPU', used: rows.cpu + rows.digest_rows, height: 2 ** t, what: 'one row per cycle, plus the digest rows that hash the program into hc' },
    { name: 'ALU', used: rows.alu, height: 2 ** (t + 1), what: 'the arithmetic the CPU looked up' },
    { name: 'Memory', used: rows.memory, height: Math.max(2 ** (t + 2), nextPow2(rows.memory)), what: 'every load and store, sorted by address then time' },
    { name: 'Poseidon2', used: rows.poseidon2_permutations * POSEIDON2_BLOCK, height: 2 ** (t + 2), what: `${group(rows.poseidon2_permutations)} permutations, ${POSEIDON2_BLOCK} rows each` },
  ];
}

/** The largest table's log height for a sample: what FRI starts folding from. */
export function largestLogHeight(s: SampleFacts): number {
  return Math.max(...tableBars(s.rows, s.tier).map((b) => log2(b.height)));
}

/** The codeword sizes (log2) FRI walks through: the largest column's low-degree extension (height
 *  × 2^log_blowup), folded by up to 2^max_log_arity per round until the final polynomial
 *  (2^log_final_poly_len coefficients, so 2^(log_final + log_blowup) points) remains. A sketch of
 *  p3-fri's schedule: with tables of several heights it may fold by less where one joins. */
export function friFolds(logHeight: number, f: { logBlowup: number; maxLogArity: number; logFinalPolyLen: number } = FRI): number[] {
  const out = [logHeight + f.logBlowup];
  const floor = f.logFinalPolyLen + f.logBlowup;
  while (out.at(-1)! > floor) out.push(Math.max(floor, out.at(-1)! - f.maxLogArity));
  return out;
}

// ---- the AIR constraint, evaluated on recorded rows -------------------------------------------

export interface CpuRow { clk: number; pc: number; next_pc: number; is_real: 0 | 1 }

/** circuits/research/src/tables/cpu.rs, under `when_transition()`:
 *    t.assert_zero(n(IS_REAL) * (n(CLK) - v(CLK) - one));
 *    t.assert_zero(n(IS_REAL) * (n(PC) - v(NEXT_PC)));
 *  Both residuals for the pair (local, next): zero on every honest pair. */
export function transitionResiduals(v: CpuRow, n: CpuRow): { clk: number; pc: number } {
  const z = (x: number) => x || 0; // a padding row's 0 · (negative) is -0: show it as 0
  return { clk: z(n.is_real * (n.clk - v.clk - 1)), pc: z(n.is_real * (n.pc - v.next_pc)) };
}

// ---- the two samples ----------------------------------------------------------------------------

export interface Rows { cpu: number; alu: number; memory: number; poseidon2_permutations: number; keccak_permutations: number; digest_rows: number }
export interface Cycle { clk: number; pc: number; next_pc: number; text: string; rd: number; rd_val: number; store?: { addr: number; val: number } }
export interface SampleFacts {
  id: 'erc20' | 'spl';
  label: string;
  sourceName: string;
  source: string;
  compiler: string;
  compiled: string;
  compiledFacts: [string, string][];
  transpiler: string;
  pairs: { src: string; c: string }[];
  pairsNote: string;
  cLines: number;
  words: number;
  wordsSplit: [string, number][];
  hc: string;
  cycles: number;
  tier: number;
  rows: Rows;
  cyclesNote: string;
  /** What the image was built with, when that differs from the terminal's defaults. */
  buildNote?: string;
}

/** The first ten cycles of the recorded ERC-20 transfer run (web/e2e/fixtures/cycles-0.json and
 *  image.json's disassembly): the image's prologue setting up its stack and data pointers. */
export const ERC20_CYCLES: Cycle[] = [
  { clk: 0, pc: 0xf794, next_pc: 0xf798, text: 'lui t1, 0x1b', rd: 6, rd_val: 110592 },
  { clk: 1, pc: 0xf798, next_pc: 0xf79c, text: 'addi t1, t1, -468', rd: 6, rd_val: 110124 },
  { clk: 2, pc: 0xf79c, next_pc: 0xf7a0, text: 'lui t0, 0x16', rd: 5, rd_val: 90112 },
  { clk: 3, pc: 0xf7a0, next_pc: 0xf7a4, text: 'addi t0, t0, -112', rd: 5, rd_val: 90000 },
  { clk: 4, pc: 0xf7a4, next_pc: 0xf7a8, text: 'sw t0, 0(t1)', rd: 0, rd_val: 0, store: { addr: 27531 * 4, val: 90000 } },
  { clk: 5, pc: 0xf7a8, next_pc: 0xf7ac, text: 'lui t0, 0x16', rd: 5, rd_val: 90112 },
  { clk: 6, pc: 0xf7ac, next_pc: 0xf7b0, text: 'addi t0, t0, -64', rd: 5, rd_val: 90048 },
  { clk: 7, pc: 0xf7b0, next_pc: 0xf7b4, text: 'sw t0, 4(t1)', rd: 0, rd_val: 0, store: { addr: 27532 * 4, val: 90048 } },
  { clk: 8, pc: 0xf7b4, next_pc: 0xf7b8, text: 'lui t0, 0x16', rd: 5, rd_val: 90112 },
  { clk: 9, pc: 0xf7b8, next_pc: 0xf7bc, text: 'addi t0, t0, -88', rd: 5, rd_val: 90024 },
];

export const REG_NAMES = ['zero', 'ra', 'sp', 'gp', 'tp', 't0', 't1', 't2'] as const;

/** The register file after the first `k` cycles (x0 stays zero), from all-zero. */
export function registersAfter(cycles: Cycle[], k: number): number[] {
  const r = new Array<number>(32).fill(0);
  for (const c of cycles.slice(0, k)) if (c.rd !== 0) r[c.rd] = c.rd_val;
  return r;
}

export const SAMPLES: Record<SampleFacts['id'], SampleFacts> = {
  erc20: {
    id: 'erc20',
    label: 'ERC-20 transfer',
    sourceName: 'ERC20.sol',
    source: `function _transfer(address from, address to, uint256 amount) internal {
    require(to != address(0), "ERC20: transfer to the zero address");
    uint256 b = _balances[from];
    require(b >= amount, "ERC20: transfer amount exceeds balance");
    _balances[from] = b - amount;
    _balances[to] += amount;
    emit Transfer(from, to, amount);
}`,
    compiler: 'solc 0.8.37, optimizer 200 runs, EVM fork shanghai',
    compiled: '1 296 bytes',
    compiledFacts: [['Runtime bytecode', '1 296 bytes'], ['Basic blocks', '74'], ['Jump destinations', '51'], ['Opcodes', '798']],
    transpiler: 'evm2rv',
    pairs: [
      { src: '0x0000 PUSH1 0x80', c: '' },
      { src: '0x0002 PUSH1 0x40', c: '' },
      { src: '0x0004 MSTORE', c: 'evm_mstore(0x40u, &K0);' },
      { src: '0x0005 CALLVALUE', c: '' },
      { src: '0x0006 DUP1', c: '' },
      { src: '0x0007 ISZERO', c: 'u256_iszero(&r0, &evm_callvalue);' },
      { src: '0x0008 PUSH2 0x000f', c: '' },
      { src: '0x000b JUMPI', c: '{ int c_ = !u256_is_zero(&r0); sp_[0] = evm_callvalue; evm_sp += 1; if (c_) goto L_15; }' },
    ],
    pairsNote: 'Block 0x0000–0x000c: one stack check and evm_charge(30) at its head, its static gas for the whole block. Pushes and DUPs become constants and locals at translation time, so they emit no C.',
    cLines: 1231,
    words: 11686,
    wordsSplit: [['Prologue', 539], ['Code', 11147]],
    hc: 'a0feae92a7311c9562495717100eb7aea71270a38ed435d06dc31387e0ea8ff6',
    cycles: 66235,
    tier: 18,
    rows: { cpu: 66235, alu: 73159, memory: 216916, poseidon2_permutations: 3908, keccak_permutations: 17, digest_rows: 3155 },
    // cycles and rows: terminal-core tests/tiers.rs erc20_transfer_lands_in_tier_18_like_rand_guest_run
    // asserts every one at 85b6f46 (the image, contract.c and hc are byte-identical to 9bf3844's).
    cyclesNote: 'transfer(BOB, 250), the sample’s default call, on the chain-id-13 reference image.',
    // randterminal.org at 85b6f46 builds the same source for chain id 20: hc 1d9896c9…, 11 691 words, 66 240 cycles, tier 18.
    buildNote: 'This is circuits’ reference build, translated for chain id 13. The chain id is compiled into the image, so the terminal’s default of 20 gives another hc, 5 more words and 5 more cycles.',
  },
  spl: {
    id: 'spl',
    label: 'SPL Token transfer',
    sourceName: 'spl_token.so',
    source: `// SPL Token as deployed on Solana mainnet: Rust,
// built by its authors with cargo build-sbf.
// The terminal bundles the ELF itself
// (samples/spl-token/spl_token.so).
Transfer { amount: 250 }`,
    compiler: 'cargo build-sbf (upstream), bundled as the mainnet ELF',
    compiled: '108 600-byte ELF',
    compiledFacts: [['ELF', '108 600 bytes'], ['Functions', '30']],
    transpiler: 'sbpf2rv',
    pairs: [
      { src: '225 ldxb r2, [r1+0]', c: 'r2 = sbpf_ld1(r1, 0);' },
      { src: '226 jeq r2, 3, +109', c: 'if (r2 == 3) goto L_336;' },
      { src: '227 jne r2, 4, +116', c: 'if (r2 != 4) goto L_344;' },
      { src: '228 ldxdw r2, [r1+88]', c: 'r2 = sbpf_ld8(r1, 88);' },
      { src: '229 jne r2, 165, +114', c: 'if (r2 != 165) goto L_344;' },
      { src: '230 ldxb r2, [r1+10512]', c: 'r2 = sbpf_ld1(r1, 10512);' },
      { src: '231 jne r2, 255, +112', c: 'if (r2 != 255) goto L_344;' },
    ],
    pairsNote: 'Function f_225: each sBPF slot becomes a line of C over the same eleven 64-bit registers, and each block first takes its instruction count from the compute budget.',
    cLines: 22371,
    words: 65096,
    wordsSplit: [['Prologue', 277], ['Code', 64819]],
    hc: '8ca905ae3c62f503de7de86829040f323e098b53dba5fffe09b42f1aad16758b',
    cycles: 765851,
    tier: 20,
    rows: { cpu: 765851, alu: 887100, memory: 2365304, poseidon2_permutations: 32288, keccak_permutations: 0, digest_rows: 25678 },
    // cycles and rows: terminal-core tests/tiers.rs spl_transfer_lands_in_tier_20 asserts every one
    // at 85b6f46 (the image, program.c and hc are byte-identical to 9bf3844's).
    cyclesNote: 'Transfer(250) on the translated image, as terminal-core’s tiers test runs it.',
  },
};
