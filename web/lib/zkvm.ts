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

// ---- the machine's parameters (circuits/research/src/machine.rs at the pinned 41d64acd) --------
// Re-read at 41d64acd (constraint set 11): TIERS, the CPU/ALU/memory heights and poseidon2::BLOCK
// are as they were at 85b6f46. Set 11 moved every program proof to rate 1/4 (ZKVM-111:
// `FriProfile::Production` is blowup 4, 92 queries, 24 bits, the recursion machine's profile) and
// made the Poseidon2 table's height proof-declared (ZKVM-112, `Tier::poseidon2_log_height_for`).

/** `machine::TIERS`: a tier `t` fixes the CPU table at 2^t rows. */
export const TIERS = [10, 12, 14, 16, 18, 20] as const;
/** `FriParameters` in `generic_config` (`LOG_BLOWUP` 2, max_log_arity 3, log_final_poly_len 0), and
 *  `FriProfile::Production` (92 queries, 24 query PoW bits; machine.rs's own test pins both). */
export const FRI = { logBlowup: 2, maxLogArity: 3, logFinalPolyLen: 0, queries: 92, powBits: 24 } as const;
/** `poseidon2::BLOCK` (circuits/research/src/tables/poseidon2.rs): rows per Poseidon2 permutation. */
export const POSEIDON2_BLOCK = 32;
/** `tables::MIN_PRIVATE_TABLE_LOG_HEIGHT` and `machine::POSEIDON2_HEIGHT_STEP`. */
const MIN_PRIVATE_TABLE_LOG_HEIGHT = 7, POSEIDON2_HEIGHT_STEP = 2;

/** `Tier::poseidon2_log_height_for`: the Poseidon2 height a prover declares by default at tier `t`,
 *  the smallest of t + 2, t, t − 2, … (down to the private-data floor) whose table holds the
 *  permutations, one 32-row block each. */
export function poseidon2LogHeight(t: number, poseidon2Perms: number): number | null {
  const admitted: number[] = [];
  for (let h = t + 2; h >= MIN_PRIVATE_TABLE_LOG_HEIGHT; h -= POSEIDON2_HEIGHT_STEP) admitted.unshift(h);
  return admitted.find((h) => poseidon2Perms * POSEIDON2_BLOCK <= 2 ** h) ?? null;
}

/** `Tier::for_workload`: the smallest tier whose CPU table holds the cycles (2^t − 1, one row is
 *  the halt's successor) and whose Poseidon2 table (2^(t+2) rows) holds the permutations. */
export function fittingTier(cyclesTotal: number, poseidon2Perms: number): number | null {
  return TIERS.find((t) => cyclesTotal <= 2 ** t - 1 && poseidon2Perms <= 2 ** (t + 2) / POSEIDON2_BLOCK) ?? null;
}

export interface TableBar { name: string; used: number; height: number; what: string }

/** Each table's height at tier `t` (`Tier::cpu_height`, `alu_height`, `min_mem_log_height`, and
 *  the declared Poseidon2 height) beside the rows the run used: the gap is padding. */
export function tableBars(rows: Rows, t: number): TableBar[] {
  return [
    { name: 'CPU', used: rows.cpu + rows.digest_rows, height: 2 ** t, what: 'one row per cycle, plus the digest rows that hash the program into hc' },
    { name: 'ALU', used: rows.alu, height: 2 ** (t + 1), what: 'the arithmetic the CPU looked up' },
    { name: 'Memory', used: rows.memory, height: Math.max(2 ** (t + 2), nextPow2(rows.memory)), what: 'every load and store, sorted by address then time' },
    { name: 'Poseidon2', used: rows.poseidon2_permutations * POSEIDON2_BLOCK, height: 2 ** (poseidon2LogHeight(t, rows.poseidon2_permutations) ?? t + 2), what: `${group(rows.poseidon2_permutations)} permutations, ${POSEIDON2_BLOCK} rows each` },
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
  { clk: 0, pc: 0xf5c4, next_pc: 0xf5c8, text: 'lui t1, 0x17', rd: 6, rd_val: 94208 },
  { clk: 1, pc: 0xf5c8, next_pc: 0xf5cc, text: 'addi t1, t1, 568', rd: 6, rd_val: 94776 },
  { clk: 2, pc: 0xf5cc, next_pc: 0xf5d0, text: 'lui t0, 0x13', rd: 5, rd_val: 77824 },
  { clk: 3, pc: 0xf5d0, next_pc: 0xf5d4, text: 'addi t0, t0, -1548', rd: 5, rd_val: 76276 },
  { clk: 4, pc: 0xf5d4, next_pc: 0xf5d8, text: 'sw t0, 0(t1)', rd: 0, rd_val: 0, store: { addr: 23694 * 4, val: 76276 } },
  { clk: 5, pc: 0xf5d8, next_pc: 0xf5dc, text: 'lui t0, 0x13', rd: 5, rd_val: 77824 },
  { clk: 6, pc: 0xf5dc, next_pc: 0xf5e0, text: 'addi t0, t0, -1500', rd: 5, rd_val: 76324 },
  { clk: 7, pc: 0xf5e0, next_pc: 0xf5e4, text: 'sw t0, 4(t1)', rd: 0, rd_val: 0, store: { addr: 23695 * 4, val: 76324 } },
  { clk: 8, pc: 0xf5e4, next_pc: 0xf5e8, text: 'lui t0, 0x13', rd: 5, rd_val: 77824 },
  { clk: 9, pc: 0xf5e8, next_pc: 0xf5ec, text: 'addi t0, t0, -1524', rd: 5, rd_val: 76300 },
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
    cLines: 1233,
    words: 7965,
    wordsSplit: [['Prologue', 655], ['Code', 7310]],
    hc: 'fd8dd3afdd785a991993c0f17dcca9a7c2c4b4c3bf213e2354d9aa1ffbf6e400',
    cycles: 59949,
    tier: 16,
    rows: { cpu: 59949, alu: 64423, memory: 190956, poseidon2_permutations: 3758, keccak_permutations: 10, digest_rows: 2355 },
    // cycles and rows: terminal-core tests/tiers.rs erc20_transfer_lands_in_tier_16_like_rand_guest_run
    // asserts every one at 41d64acd (constraint set 11; the hc is evm2rv/README.md's stage-two ERC-20).
    cyclesNote: 'transfer(BOB, 250), the sample’s default call, on circuits’ reference image.',
    // Measured at 41d64acd: the chain-id-22 build of this source is byte-identical to the chain-id-13 one
    // (hc fd8dd3af…, 7 965 words, 59 949 cycles, tier 16). At 85b6f46 the chain-20 build was hc 1d9896c9…, 5 words more.
    buildNote: 'This is circuits’ reference build, translated for chain id 13. The contract never reads CHAINID, so the terminal’s default chain id, 22, builds the same image and hc.',
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
    cLines: 22522,
    words: 63968,
    wordsSplit: [['Prologue', 2124], ['Code', 61844]],
    hc: '77919dab5ec22e58729064e0711fbf57dc040785eb0612a73e6ad0c4370a42b7',
    cycles: 250171,
    tier: 20,
    rows: { cpu: 250171, alu: 279807, memory: 790159, poseidon2_permutations: 18619, keccak_permutations: 0, digest_rows: 18612 },
    // cycles and rows: terminal-core tests/tiers.rs spl_transfer_lands_in_tier_20 asserts every one
    // at 41d64acd (constraint set 11; the hc is sbpf2rv/tests/parity.rs's SPL_TOKEN_HC). The image
    // carries the program (ZKVM-117): its prologue writes the read-only data, no ELF is read or hashed.
    cyclesNote: 'Transfer(250) on the translated image, as terminal-core’s tiers test runs it.',
  },
};
