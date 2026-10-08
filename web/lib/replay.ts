import type { AbiItem, CycleView, EvmCallForm } from './types';

export const REG_NAMES = ['zero', 'ra', 'sp', 'gp', 'tp', 't0', 't1', 't2', 's0', 's1', 'a0', 'a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7', 's2', 's3', 's4', 's5', 's6', 's7', 's8', 's9', 's10', 's11', 't3', 't4', 't5', 't6'];

export class PageCache {
  private pages = new Map<number, CycleView[]>();
  private inflight = new Map<number, Promise<CycleView[]>>();
  constructor(private fetchPage: (from: number, count: number) => Promise<CycleView[]>, private size = 4096, private keep = 32) {}
  private async page(p: number): Promise<CycleView[]> {
    const hit = this.pages.get(p);
    if (hit) { this.pages.delete(p); this.pages.set(p, hit); return hit; }
    let f = this.inflight.get(p);
    if (!f) {
      // A rejected fetch must not stay in `inflight`, or every later get() of the page would replay the failure.
      f = this.fetchPage(p * this.size, this.size).finally(() => { this.inflight.delete(p); });
      this.inflight.set(p, f);
    }
    const rows = await f;
    this.pages.set(p, rows);
    while (this.pages.size > this.keep) this.pages.delete(this.pages.keys().next().value!);
    return rows;
  }
  async get(clk: number): Promise<CycleView> {
    const rows = await this.page(Math.floor(clk / this.size));
    const c = rows[clk % this.size];
    if (!c) throw new Error(`cycle ${clk} is past the end`);
    return c;
  }
  peek(clk: number): CycleView | undefined { return this.pages.get(Math.floor(clk / this.size))?.[clk % this.size]; }
  get pageSize() { return this.size; }
}

export async function nextMatching(cache: PageCache, from: number, dir: 1 | -1, pred: (c: CycleView) => boolean, total: number, limit = 1 << 20): Promise<number | null> {
  for (let clk = from + dir, n = 0; clk >= 0 && clk < total && n < limit; clk += dir, n++) {
    if (pred(await cache.get(clk))) return clk;
  }
  return null;
}

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
