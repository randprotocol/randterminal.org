import { describe, expect, it } from 'vitest';
import { cpuTotal, fittingTier, tableBars, tierSentence } from '@/lib/tables';
import type { RunSummary } from '@/lib/types';

const tier = (t: number, fits: boolean) => ({ tier: t, cpu: 2 ** t, alu: 2 ** (t + 1), memory_min: 2 ** (t + 2), memory_max: 2 ** 24, poseidon2_permutations: 2 ** (t + 2) / 32, keccak_permutations: 2 ** Math.min(t + 5, 20) / 32, sha256_compressions: 2 ** Math.min(t + 6, 20) / 64, fits });
const run = { rid: 'r', cycles: 66235, halted: true, outputs: [1, 0, 0, 0, 0, 0, 0, 0], status: 1, tier: 18,
  rows: { cpu: 66235, memory: 200000, alu: 30000, poseidon2_permutations: 4000, keccak_permutations: 0, sha256_compressions: 0, digest_rows: 3200 },
  tiers: [10, 12, 14, 16, 18, 20].map((t) => tier(t, t >= 18)), syscall_counts: [], hot_pcs: [], input_words: 921, public_words: 0, discovery: [] } as RunSummary;

describe('tableBars', () => {
  it('fills against the chosen tier and drops unused tables', () => {
    const b = tableBars(run, 18);
    expect(b.map((x) => x.key)).toEqual(['cpu', 'memory', 'alu', 'poseidon2']);
    const cpu = b[0]!;
    expect(cpu.used).toBe(cpuTotal(run));
    expect(cpu.capacity).toBe(2 ** 18);
    expect(cpu.fill).toBeCloseTo((66235 + 3200) / 2 ** 18);
    expect(cpu.over).toBe(false);
  });
  it('flags overflow at a smaller tier', () => {
    expect(tableBars(run, 16)[0]!.over).toBe(true);
  });
  it('memory is over at exactly 2^24 (strictly-less rule)', () => {
    const r = { ...run, rows: { ...run.rows, memory: 2 ** 24 } } as RunSummary;
    expect(tableBars(r, 18).find((x) => x.key === 'memory')!.over).toBe(true);
    const r2 = { ...run, rows: { ...run.rows, memory: 2 ** 24 - 1 } } as RunSummary;
    expect(tableBars(r2, 18).find((x) => x.key === 'memory')!.over).toBe(false);
  });
});

describe('boundaries', () => {
  it('CPU is over at exactly cpu_height (server needs cycles <= cpu_height - 1)', () => {
    const r = { ...run, rows: { ...run.rows, cpu: 2 ** 18 - 3200 } } as RunSummary;
    expect(cpuTotal(r)).toBe(2 ** 18);
    expect(tableBars(r, 18)[0]!.over).toBe(true);
  });
});

describe('tierSentence', () => {
  it('names the tier when it fits', () => {
    expect(tierSentence(run)).toMatch(/^This run needs tier 18: the smallest whose CPU table holds 69 435 CPU rows \(cycles plus digest rows\)/);
  });
  it('never claims a tier whose fits is false (memory over)', () => {
    const r = { ...run, rows: { ...run.rows, memory: 2 ** 24 + 5 }, tiers: run.tiers.map((t) => ({ ...t, fits: false })) } as RunSummary;
    const s = tierSentence(r);
    expect(s).not.toMatch(/needs tier/);
    expect(s).toMatch(/Tier 18 holds the cycles, but the memory table needs 16 777 221 rows, more than any tier allows/);
  });
  it('says no tier fits when tier is null (trap)', () => {
    const r = { ...run, tier: null, trap: 'x', tiers: run.tiers.map((t) => ({ ...t, fits: false })) } as RunSummary;
    expect(tierSentence(r)).toMatch(/^No tier fits/);
  });
  it('ALU over at r.tier but fitting larger: names the smallest fitting tier', () => {
    const r = { ...run, rows: { ...run.rows, alu: 2 ** 19 + 1 }, tiers: [10, 12, 14, 16, 18, 20].map((t) => ({ ...tier(t, false), fits: t === 20 })) } as RunSummary;
    expect(tierSentence(r)).toMatch(/^This run needs tier 20:/);
  });
  it('ALU over everywhere is worded per tier, not "any tier"', () => {
    const r = { ...run, rows: { ...run.rows, alu: 2 ** 22 }, tiers: run.tiers.map((t) => ({ ...t, fits: false })) } as RunSummary;
    const s = tierSentence(r);
    expect(s).toMatch(/Tier 18 does not fit: the ALU table needs 4 194 304 rows, more than its capacity of 524 288/);
    expect(s).not.toMatch(/more than any tier/);
  });
  it('Keccak over', () => {
    const r = { ...run, rows: { ...run.rows, keccak_permutations: 40000 }, tiers: run.tiers.map((t) => ({ ...t, fits: false })) } as RunSummary;
    expect(tierSentence(r)).toMatch(/the Keccak table needs 40 000 rows, more than its capacity of 32 768/);
  });
  it('trapped run with a non-null tier mentions the trap', () => {
    const r = { ...run, trap: 'illegal instruction', tiers: run.tiers.map((t) => ({ ...t, fits: false })) } as RunSummary;
    const s = tierSentence(r);
    expect(s).not.toMatch(/needs tier/);
    expect(s).toMatch(/trapped \(illegal instruction\)/);
  });
});

describe('fittingTier', () => {
  it('is the smallest tier that fits, never a server tier that does not', () => {
    expect(fittingTier(run)).toBe(18);
    expect(fittingTier({ ...run, tier: 16, tiers: [16, 18, 20].map((t) => tier(t, t >= 20)) })).toBe(20);
    expect(fittingTier({ ...run, tiers: [16, 18, 20].map((t) => tier(t, false)) })).toBeNull();
  });
});
