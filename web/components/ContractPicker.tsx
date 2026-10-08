'use client';

/** Picks the contract to transpile when the Solidity sources define several; picking re-submits
 *  the job with options.contract. Disabled (no `onPick`) while a job is running. */
export function ContractPicker({ contracts, value, onPick }: { contracts: string[]; value: string | null; onPick?: (c: string) => void }) {
  const v = value != null && contracts.includes(value) ? value : null;
  return (
    <select aria-label="contract" value={v ?? ''} disabled={!onPick} onChange={(e) => { if (e.target.value) onPick?.(e.target.value); }}
      className="mono h-8 rounded-sm border border-border bg-surface px-2 text-sm text-text disabled:opacity-50">
      {v == null && <option value="">choose a contract…</option>}
      {contracts.map((c) => <option key={c} value={c}>{c}</option>)}
    </select>
  );
}
