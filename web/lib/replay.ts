import { ApiError } from './api';
import type { AbiItem, EvmCallForm } from './types';

export const REG_NAMES = ['zero', 'ra', 'sp', 'gp', 'tp', 't0', 't1', 't2', 's0', 's1', 'a0', 'a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7', 's2', 's3', 's4', 's5', 's6', 's7', 's8', 's9', 's10', 's11', 't3', 't4', 't5', 't6'];

export function changedRegs(prev: number[] | null, next: number[]): Set<number> {
  const s = new Set<number>();
  if (!prev) return s;
  next.forEach((v, i) => { if (prev[i] !== v) s.add(i); });
  return s;
}

export const abiSignature = (a: AbiItem) => `${a.name}(${(a.inputs ?? []).map((p) => p.type).join(',')})`;

export function evmFormFromAbi(abi: AbiItem[], fn: string, caller: string): EvmCallForm {
  const item = abi.find((a) => a.type === 'function' && abiSignature(a) === fn);
  return { lang: 'evm', function: fn, args: (item?.inputs ?? []).map(() => ''), caller, value: '0', gas_limit: 1_000_000, storage: [] };
}

export const ALICE = '0x0000000000000000000000001111111111111111111111111111111111111111';
export const MAX_GAS = 30_000_000;
export const gasError = (g: number | undefined): string | null =>
  g !== undefined && Number.isInteger(g) && g >= 1 && g <= MAX_GAS ? null : `gas limit must be a whole number from 1 to ${MAX_GAS.toLocaleString('en-US')}`;

/** The nearest syscall strictly after (dir 1) or before (dir -1) `clk`, by binary search over the clk-sorted list. */
export function syscallFrom(syscalls: { clk: number }[], clk: number, dir: 1 | -1): number | null {
  let lo = 0, hi = syscalls.length;
  if (dir === 1) { while (lo < hi) { const m = (lo + hi) >> 1; if (syscalls[m]!.clk <= clk) lo = m + 1; else hi = m; } return syscalls[lo]?.clk ?? null; }
  while (lo < hi) { const m = (lo + hi) >> 1; if (syscalls[m]!.clk < clk) lo = m + 1; else hi = m; }
  return syscalls[lo - 1]?.clk ?? null;
}

/** "Next source op": one server request (`/runs/:rid/next_src`), and only when the image has a
 *  pc-level source map. Returns where to seek, or a note for the toolbar. */
export async function nextSourceOp(ask: (from: number, dir: 1 | -1) => Promise<{ clk: number }>, clk: number, hasMap: boolean): Promise<{ seek: number } | { note: string }> {
  if (!hasMap) return { note: 'no source map for this build' };
  try { return { seek: (await ask(clk, 1)).clk }; }
  catch (e) {
    if (e instanceof ApiError && e.status === 404) return { note: 'no later source op within reach; use the timeline' };
    return { note: `could not search: ${e instanceof Error ? e.message : String(e)}` };
  }
}
