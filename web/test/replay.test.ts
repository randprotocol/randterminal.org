import { describe, expect, it, vi } from 'vitest';
import { abiSignature, changedRegs, evmFormFromAbi, gasError, nextMatching, PageCache, REG_NAMES, syscallFrom } from '@/lib/replay';
import type { CycleView } from '@/lib/types';

const cyc = (clk: number, extra: Partial<CycleView> = {}): CycleView => ({ clk, pc: 0x10000 + 4 * clk, next_pc: 0, word: 0, rd: 0, writes_rd: false, rd_val: 0, mem_read: false, mem_write: false, mem_addr: 0, mem_val: 0, syscall: false, hash_row: false, n_alu: 0, sys_idx: 0xffffffff, ...extra });

function fakeFetch(total: number, syscalls: number[]) {
  return vi.fn(async (from: number, count: number) => Array.from({ length: Math.max(0, Math.min(count, total - from)) }, (_, k) => cyc(from + k, { syscall: syscalls.includes(from + k) })));
}

describe('PageCache', () => {
  it('fetches a page once and serves cycles from it', async () => {
    const f = fakeFetch(10_000, []);
    const c = new PageCache(f, 4096);
    expect((await c.get(5)).clk).toBe(5);
    expect((await c.get(4095)).clk).toBe(4095);
    expect(f).toHaveBeenCalledTimes(1);
    expect((await c.get(4096)).clk).toBe(4096);
    expect(f).toHaveBeenCalledTimes(2);
    expect(f.mock.calls[1]).toEqual([4096, 4096]);
    expect(c.peek(4100)?.clk).toBe(4100);
  });
});

describe('nextMatching', () => {
  it('finds the next and previous syscall across pages', async () => {
    const c = new PageCache(fakeFetch(10_000, [100, 5000, 9999]), 4096);
    expect(await nextMatching(c, 100, 1, (x) => x.syscall, 10_000)).toBe(5000);
    expect(await nextMatching(c, 5000, -1, (x) => x.syscall, 10_000)).toBe(100);
    expect(await nextMatching(c, 9999, 1, (x) => x.syscall, 10_000)).toBeNull();
  });
});

describe('registers', () => {
  it('names and diffs', () => {
    expect(REG_NAMES[10]).toBe('a0');
    expect([...changedRegs([0, 1, 2], [0, 1, 3])]).toEqual([2]);
    expect(changedRegs(null, [1]).size).toBe(0);
  });
});

describe('ABI forms', () => {
  const abi = [{ type: 'function', name: 'transfer', inputs: [{ name: 'to', type: 'address' }, { name: 'amount', type: 'uint256' }], stateMutability: 'nonpayable' }];
  it('signs and fills', () => {
    expect(abiSignature(abi[0]!)).toBe('transfer(address,uint256)');
    const f = evmFormFromAbi(abi, 'transfer(address,uint256)', '0x11');
    expect(f).toMatchObject({ lang: 'evm', function: 'transfer(address,uint256)', args: ['', ''], caller: '0x11', storage: [] });
  });
});

describe('PageCache robustness', () => {
  it('recovers after a failed fetch', async () => {
    const ok = fakeFetch(10_000, []);
    let n = 0;
    const f = vi.fn((from: number, count: number) => (n++ === 0 ? Promise.reject(new Error('boom')) : ok(from, count)));
    const c = new PageCache(f, 4096);
    await expect(c.get(5)).rejects.toThrow('boom');
    expect((await c.get(5)).clk).toBe(5);
    expect(f).toHaveBeenCalledTimes(2);
  });
  it('shares one in-flight fetch', async () => {
    const f = fakeFetch(10_000, []);
    const c = new PageCache(f, 4096);
    const [a, b] = await Promise.all([c.get(1), c.get(2)]);
    expect([a.clk, b.clk]).toEqual([1, 2]);
    expect(f).toHaveBeenCalledTimes(1);
  });
  it('evicts the least recently used page past 32', async () => {
    const f = fakeFetch(100 * 16, []);
    const c = new PageCache(f, 16, 32);
    for (let p = 0; p < 33; p++) await c.get(p * 16);
    expect(c.peek(0)).toBeUndefined();
    expect(c.peek(32 * 16)).toBeDefined();
    expect(c.peek(16)).toBeDefined();
  });
  it('rejects a cycle past the end', async () => {
    const c = new PageCache(fakeFetch(10, []), 4096);
    await expect(c.get(10)).rejects.toThrow('past the end');
  });
});

describe('syscallFrom and gasError', () => {
  const s = [{ clk: 10 }, { clk: 20 }, { clk: 30 }];
  it('searches both ways', () => {
    expect(syscallFrom(s, 0, 1)).toBe(10);
    expect(syscallFrom(s, 10, 1)).toBe(20);
    expect(syscallFrom(s, 30, 1)).toBeNull();
    expect(syscallFrom(s, 30, -1)).toBe(20);
    expect(syscallFrom(s, 25, -1)).toBe(20);
    expect(syscallFrom(s, 10, -1)).toBeNull();
    expect(syscallFrom([], 5, 1)).toBeNull();
  });
  it('bounds gas', () => {
    expect(gasError(1_000_000)).toBeNull();
    expect(gasError(30_000_001)).not.toBeNull();
    expect(gasError(0)).not.toBeNull();
    expect(gasError(NaN)).not.toBeNull();
    expect(gasError(1.5)).not.toBeNull();
  });
});
