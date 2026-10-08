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
      // The server's rule for memory is strictly less than 2^24; every other table may be exactly full.
      over: key === 'memory' ? used >= capacity : used > capacity,
      explain: EXPLAIN[key]!,
    }));
}

/** The sentence above the bars. "needs tier T" only when `run.tiers` confirms T fits: the memory table is
 *  tier-independent, so the server's `tier` (cycles and Poseidon2 only) can name a tier that does not fit. */
export function tierSentence(r: RunSummary): string {
  const named = r.tier != null ? r.tiers.find((x) => x.tier === r.tier) : undefined;
  if (named?.fits) {
    return `This run needs tier ${named.tier}: the smallest whose CPU table holds ${groupDigits(cpuTotal(r))} cycles and whose Poseidon2 table holds ${groupDigits(r.rows.poseidon2_permutations)} permutations. Proving cost grows with the tier.`;
  }
  const top = r.tiers.length ? Math.max(...r.tiers.map((x) => x.tier)) : 20;
  const over = tableBars(r, r.tier ?? top).filter((b) => b.over);
  if (r.tier != null) {
    const mem = over.find((b) => b.key === 'memory');
    if (mem) return `Tier ${r.tier} holds the cycles, but the memory table needs ${groupDigits(mem.used)} rows, more than any tier allows.`;
    if (over.length) return `Tier ${r.tier} holds the cycles, but the ${over.map((b) => b.label).join(' and ')} table${over.length > 1 ? 's need' : ' needs'} more rows than any tier allows.`;
  }
  // Largest tier is the most permissive: tables over there are over everywhere.
  const names = tableBars(r, top).filter((b) => b.over).map((b) => `${b.label} (${groupDigits(b.used)} rows)`);
  return `No tier fits this run${names.length ? `: ${names.join(', ')} exceed${names.length > 1 ? '' : 's'} the largest tier` : ''}.${r.trap ? ' The run trapped.' : ''}`;
}
