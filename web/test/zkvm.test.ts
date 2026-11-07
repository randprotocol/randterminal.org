import { describe, expect, it } from 'vitest';
import { ERC20_CYCLES, FRI, SAMPLES, STEPS, autoNext, fittingTier, friFolds, largestLogHeight, nextPow2, registersAfter, stepBy, tableBars, transitionResiduals } from '@/lib/zkvm';

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
    expect(STEPS.map((x) => x.token(s))).toEqual(['ERC20.sol', '1 296 bytes', '1 231 lines of C', '11 686 words', '66 235 cycles', 'tier 18', 'every row checked', 'Merkle roots', '7 folds', 'accept']);
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
    const bars = tableBars(SAMPLES.erc20.rows, 18);
    expect(bars.map((b) => [b.name, b.used, b.height])).toEqual([
      ['CPU', 69390, 262144], ['ALU', 73159, 524288], ['Memory', 216916, 1048576], ['Poseidon2', 3908 * 32, 1048576],
    ]);
    expect(bars.every((b) => b.used <= b.height)).toBe(true);
    // SPL's memory outgrows the floor of 2^22 only if it passes it: 2 365 304 rows pad to 2^22.
    expect(tableBars(SAMPLES.spl.rows, 20)[2]!.height).toBe(2 ** 22);
    expect(nextPow2(1)).toBe(1);
    expect(nextPow2(216916)).toBe(262144);
  });
});

describe('FRI', () => {
  it('folds the largest low-degree extension by eight until the final polynomial is left', () => {
    expect(FRI).toMatchObject({ logBlowup: 3, maxLogArity: 3, queries: 80, powBits: 20 });
    expect(largestLogHeight(SAMPLES.erc20)).toBe(20);
    expect(friFolds(20)).toEqual([23, 20, 17, 14, 11, 8, 5, 3]);
    expect(friFolds(22)).toEqual([25, 22, 19, 16, 13, 10, 7, 4, 3]);
    expect(friFolds(0)).toEqual([3]);
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
    expect(r.slice(0, 8)).toEqual([0, 0, 0, 0, 0, 90024, 110124, 0]);
    expect(registersAfter(ERC20_CYCLES, 0).every((x) => x === 0)).toBe(true);
  });
});
