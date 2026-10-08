import { groupDigits } from './format';
import type { RunSummary } from './types';

/** The CPU table budget: executed cycles plus the digest rows `Tier::for_workload` counts. */
export const cpuTotal = (r: RunSummary) => r.rows.cpu + r.rows.digest_rows;

const EXPLAIN: Record<string, string> = {
  cpu: 'one row per executed cycle, plus the rows that hash the program, inputs and public inputs',
  memory: 'one row per register or RAM access; sorted by address to prove every read returns the last write',
  alu: 'one row per arithmetic event the CPU delegates (multiplies, divides, shifts, comparisons)',
  poseidon2: 'permutations for the digests and for every POSEIDON2 syscall block',
  keccak: 'one permutation per KECCAK syscall',
  sha256: 'one compression per SHA256 syscall',
};

export interface Bar { key: string; label: string; used: number; capacity: number; fill: number; over: boolean; explain: string }

export function tableBars(r: RunSummary, tier: number): Bar[] {
  const t = r.tiers.find((x) => x.tier === tier);
  if (!t) return [];
  const rows: [string, string, number, number][] = [
    ['cpu', 'CPU', cpuTotal(r), t.cpu],
    ['memory', 'Memory', r.rows.memory, t.memory_max],
    ['alu', 'ALU', r.rows.alu, t.alu],
    ['poseidon2', 'Poseidon2', r.rows.poseidon2_permutations, t.poseidon2_permutations],
    ['keccak', 'Keccak', r.rows.keccak_permutations, t.keccak_permutations],
    ['sha256', 'SHA-256', r.rows.sha256_compressions, t.sha256_compressions],
  ];
  return rows
    .filter(([k, , used]) => used > 0 || ['cpu', 'memory', 'alu', 'poseidon2'].includes(k))
    .map(([key, label, used, capacity]) => ({
      key, label, used, capacity, fill: used / capacity,
      // The server's fits rule is strict for memory (< 2^24) and the CPU table (cycles <= cpu_height - 1); the rest may be exactly full.
      over: key === 'memory' || key === 'cpu' ? used >= capacity : used > capacity,
      explain: EXPLAIN[key]!,
    }));
}

/** The sentence above the bars. "needs tier T" only for the smallest tier `run.tiers` marks fits:true: the
 *  server's `tier` ignores the memory table (and ALU, Keccak, SHA-256), so it can name a tier that does not fit. */
export function tierSentence(r: RunSummary): string {
  const fit = fittingTier(r);
  if (fit != null) {
    return `This run needs tier ${fit}: the smallest whose CPU table holds ${groupDigits(cpuTotal(r))} CPU rows (cycles plus digest rows) and whose Poseidon2 table holds ${groupDigits(r.rows.poseidon2_permutations)} permutations. Proving cost grows with the tier.`;
  }
  const top = r.tiers.length ? Math.max(...r.tiers.map((x) => x.tier)) : 20;
  const mem = tableBars(r, top).find((b) => b.key === 'memory' && b.over);
  const at = r.tier ?? top;
  const over = tableBars(r, at).filter((b) => b.over && b.key !== 'memory');
  const list = over.map((b) => `the ${b.label} table needs ${groupDigits(b.used)} rows, more than its capacity of ${groupDigits(b.capacity)}`).join('; ');
  const trap = r.trap ? ` The run trapped (${r.trap}), and a trapped run is never proved.` : '';
  if (mem) {
    const head = over.length ? `Tier ${at} does not fit: ${list}; ` : r.tier != null ? `Tier ${at} holds the cycles, but ` : '';
    return `${head}${over.length || r.tier != null ? 'the' : 'The'} memory table needs ${groupDigits(mem.used)} rows, more than any tier allows.${trap}`;
  }
  if (r.tier != null && over.length) return `Tier ${at} does not fit: ${list}; no larger tier holds it either.${trap}`;
  if (over.length) return `No tier fits: at tier ${at}, ${list}.${trap}`;
  return `No tier fits this run.${trap}`;
}

/** The tier this run needs: the smallest one `run.tiers` marks fits:true (the server's `tier`
 *  ignores the memory, ALU, Keccak and SHA-256 tables, so it may name one that does not fit). */
export function fittingTier(r: RunSummary): number | null {
  const fit = r.tiers.filter((x) => x.fits).sort((a, b) => a.tier - b.tier)[0];
  return fit ? fit.tier : null;
}
