import { describe, expect, it } from 'vitest';
import { DEFAULT_EVM_VERSION, EVM_VERSIONS, isEvmVersion, jobOptions, loadFork, saveFork, warnedPcs } from '@/lib/evm';

describe('EVM forks', () => {
  it('lists the fourteen forks solc 0.8.37 accepts, shanghai by default', () => {
    expect(EVM_VERSIONS).toEqual(['homestead', 'tangerineWhistle', 'spuriousDragon', 'byzantium', 'constantinople', 'petersburg', 'istanbul', 'berlin', 'london', 'paris', 'shanghai', 'cancun', 'prague', 'osaka']);
    expect(DEFAULT_EVM_VERSION).toBe('shanghai');
    expect(isEvmVersion('cancun')).toBe(true);
    expect(isEvmVersion('amsterdam')).toBe(false);
  });
  it('puts the fork into a Solidity request and keeps it out of every other', () => {
    expect(jobOptions('solidity', { chain_id: 20, stage: 2 }, 'cancun')).toEqual({ chain_id: 20, stage: 2, evm_version: 'cancun' });
    expect(jobOptions('solana-rust', { chain_id: 20, stage: 2, evm_version: 'cancun' }, 'cancun')).toEqual({ chain_id: 20, stage: 2 });
  });
  it('remembers the fork for the session, and falls back when storage is empty, junk or blocked', () => {
    const mem = new Map<string, string>();
    const store = { getItem: (k: string) => mem.get(k) ?? null, setItem: (k: string, v: string) => void mem.set(k, v) };
    expect(loadFork(store)).toBe('shanghai');
    saveFork('osaka', store);
    expect(loadFork(store)).toBe('osaka');
    mem.set('randterminal.evmVersion', 'frontier');
    expect(loadFork(store)).toBe('shanghai');
    const blocked = { getItem: () => { throw new Error('denied'); }, setItem: () => { throw new Error('denied'); } };
    expect(loadFork(blocked)).toBe('shanghai');
    expect(() => saveFork('cancun', blocked)).not.toThrow();
    expect(loadFork(undefined)).toBe('shanghai');
  });
  it('collects the pcs a zkVM warning points at', () => {
    expect([...warnedPcs([{ kind: 'unsupported_opcode', pc: 2, detail: '', explanation: '', fatal: false }, { kind: 'code_too_large', pc: null, detail: '', explanation: '', fatal: true }])]).toEqual([2]);
    expect(warnedPcs(undefined).size).toBe(0);
  });
});
