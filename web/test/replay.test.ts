import { describe, expect, it, vi } from 'vitest';
import { abiSignature, changedRegs, evmFormFromAbi, nextMatching, PageCache, REG_NAMES } from '@/lib/replay';
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
