import { describe, expect, it, vi } from 'vitest';
import { abiSignature, changedRegs, evmFormFromAbi, gasError, nextSourceOp, REG_NAMES, syscallFrom } from '@/lib/replay';
import { ApiError } from '@/lib/api';

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

describe('nextSourceOp', () => {
  it('is one server request, decided by the image map alone', async () => {
    const ask = vi.fn(async (_from: number, _dir: 1 | -1) => ({ clk: 77, src_pc: 12 }));
    expect(await nextSourceOp(ask, 5, false)).toEqual({ note: 'no source map for this build' });
    expect(ask).not.toHaveBeenCalled();
    expect(await nextSourceOp(ask, 5, true)).toEqual({ seek: 77 });
    expect(ask).toHaveBeenCalledWith(5, 1);
  });
  it('says when there is no later op, and surfaces other failures', async () => {
    expect(await nextSourceOp(async () => { throw new ApiError(404, 'not_found', 'none'); }, 5, true)).toEqual({ note: 'no later source op within reach; use the timeline' });
    expect(await nextSourceOp(async () => { throw new ApiError(500, 'internal', 'boom'); }, 5, true)).toEqual({ note: 'could not search: boom' });
  });
});
