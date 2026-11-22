import { describe, expect, it } from 'vitest';
import { ERC20_CYCLES, FRI, SAMPLES, STEPS, autoNext, fittingTier, friFolds, largestLogHeight, nextPow2, poseidon2LogHeight, registersAfter, stepBy, tableBars, transitionResiduals } from '@/lib/zkvm';

describe('the walkthrough steps', () => {
  it('runs source to verifier, ten steps', () => {
    expect(STEPS.map((s) => s.id)).toEqual(['source', 'compile', 'transpile', 'image', 'execute', 'tables', 'air', 'commit', 'fri', 'verify']);
  });
  it('steps are clamped at both ends', () => {
    expect(stepBy(0, -1)).toBe(0);
    expect(stepBy(0, 1)).toBe(1);
    expect(stepBy(9, 1)).toBe(9);
    expect(stepBy(4, -10)).toBe(0);
  });
  it('autoplay stops at the verifier instead of wrapping', () => {
    expect(autoNext(0)).toBe(1);
    expect(autoNext(8)).toBe(9);
    expect(autoNext(9)).toBeNull();
  });
  it('each step names what the program is there', () => {
    const s = SAMPLES.erc20;
    expect(STEPS.map((x) => x.token(s))).toEqual(['ERC20.sol', '1 296 bytes', '1 233 lines of C', '7 965 words', '59 949 cycles', 'tier 16', 'every row checked', 'Merkle roots', '6 folds', 'accept']);
  });
});

describe('tiers and tables', () => {
  it('lands each sample in the tier the API reports', () => {
    for (const s of Object.values(SAMPLES)) expect(fittingTier(s.rows.cpu + s.rows.digest_rows, s.rows.poseidon2_permutations)).toBe(s.tier);
    // 2^16 − 1 = 65 535 cycles is the most tier 16 holds.
    expect(fittingTier(65535, 0)).toBe(16);
    expect(fittingTier(65536, 0)).toBe(18);
    expect(fittingTier(2 ** 20, 0)).toBeNull();
  });
  it('pads every table to its tier height', () => {
    const bars = tableBars(SAMPLES.erc20.rows, 16);
    expect(bars.map((b) => [b.name, b.used, b.height])).toEqual([
      ['CPU', 62304, 65536], ['ALU', 64423, 131072], ['Memory', 190956, 262144], ['Poseidon2', 3758 * 32, 262144],
    ]);
    expect(bars.every((b) => b.used <= b.height)).toBe(true);
    // SPL's memory outgrows the floor of 2^22 only if it passes it: 790 159 rows pad to the floor.
    expect(tableBars(SAMPLES.spl.rows, 20)[2]!.height).toBe(2 ** 22);
    expect(nextPow2(1)).toBe(1);
    expect(nextPow2(190956)).toBe(262144);
  });
  it('declares the smallest admitted Poseidon2 height (constraint set 11)', () => {
    // Admitted at tier t: 2^8, 2^10, …, 2^(t+2). SPL's 18 619 permutations (595 808 rows) take 2^20 at tier 20, not 2^22.
    expect(poseidon2LogHeight(20, SAMPLES.spl.rows.poseidon2_permutations)).toBe(20);
    expect(tableBars(SAMPLES.spl.rows, 20)[3]!.height).toBe(2 ** 20);
    // 120 256 rows: 2^17 would hold them but is not admitted, so 2^18.
    expect(poseidon2LogHeight(16, 3758)).toBe(18);
    expect(poseidon2LogHeight(10, 0)).toBe(8);
    expect(poseidon2LogHeight(10, 2 ** 12 / 32 + 1)).toBeNull();
  });
});

describe('FRI', () => {
  it('folds the largest low-degree extension by eight until the final polynomial is left', () => {
    expect(FRI).toMatchObject({ logBlowup: 2, maxLogArity: 3, queries: 92, powBits: 24 });
    expect(largestLogHeight(SAMPLES.erc20)).toBe(18);
    expect(friFolds(18)).toEqual([20, 17, 14, 11, 8, 5, 2]);
    expect(friFolds(22)).toEqual([24, 21, 18, 15, 12, 9, 6, 3, 2]);
    expect(friFolds(0)).toEqual([2]);
  });
});

describe('the transition constraint on the recorded rows', () => {
  const row = (i: number) => ({ clk: ERC20_CYCLES[i]!.clk, pc: ERC20_CYCLES[i]!.pc, next_pc: ERC20_CYCLES[i]!.next_pc, is_real: 1 as const });
  it('is zero on every honest pair', () => {
    for (let i = 0; i + 1 < ERC20_CYCLES.length; i++) expect(transitionResiduals(row(i), row(i + 1))).toEqual({ clk: 0, pc: 0 });
  });
  it('catches a skipped instruction and a skipped cycle', () => {
    expect(transitionResiduals(row(0), { ...row(1), pc: row(1).pc + 4 }).pc).toBe(4);
    expect(transitionResiduals(row(0), { ...row(1), clk: 2 }).clk).toBe(1);
    // A padding row (is_real = 0) is not held to it.
    expect(transitionResiduals(row(0), { ...row(1), pc: 0, is_real: 0 })).toEqual({ clk: 0, pc: 0 });
  });
  it('the register file follows the recorded writes', () => {
    const r = registersAfter(ERC20_CYCLES, 10);
    expect(r.slice(0, 8)).toEqual([0, 0, 0, 0, 0, 76300, 94776, 0]);
    expect(registersAfter(ERC20_CYCLES, 0).every((x) => x === 0)).toBe(true);
  });
});
