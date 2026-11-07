'use client';
import { useState } from 'react';
import type { CompileView, WarningView } from '@/lib/types';
import { groupDigits } from '@/lib/format';
import { contractChoices } from '@/lib/job';
import { ContractPicker } from './ContractPicker';
import { CopyButton, Facts } from './ui';

export function CompilePanel({ view, onPickContract }: { view: CompileView; onPickContract?: (c: string) => void }) {
  const [which, setWhich] = useState<'runtime' | 'creation'>('runtime');
  if (view.lang === 'evm') {
    const hex = which === 'runtime' ? view.runtime_hex : view.creation_hex;
    return (
      <div className="flex flex-col gap-4 p-4">
        <Facts items={[['Contract', view.contracts.length > 1 ? <ContractPicker key="c" contracts={contractChoices(null, view)} value={view.contract} onPick={onPickContract} /> : view.contract], ['Runtime bytecode', `${groupDigits(view.size_bytes)} bytes`], ['EVM fork', view.evm_version ?? 'shanghai'], ['Blocks', view.blocks], ['Jump destinations', view.jumpdests]]} />
        <ZkvmWarnings warnings={view.warnings ?? []} />
        <div>
          <div className="mb-2 flex items-center gap-1" role="tablist" aria-label="bytecode">
            {(['runtime', 'creation'] as const).map((w) => <button key={w} role="tab" aria-selected={which === w} onClick={() => setWhich(w)} className="seg capitalize">{w}</button>)}
            <span className="ml-auto"><CopyButton text={hex} /></span>
          </div>
          <pre className="mono max-h-56 overflow-auto rounded-md bg-bg-soft p-3 text-xs break-all whitespace-pre-wrap text-text-soft">{hex || '(empty)'}</pre>
          <p className="mt-1 text-xs text-text-mute">The transpiler takes the runtime code: what the chain stores, not the constructor that deploys it.</p>
        </div>
        <table className="data-table"><caption>ABI</caption>
          <thead><tr><th>Function</th><th>Inputs</th><th>Outputs</th><th>Mutability</th></tr></thead>
          <tbody>{view.abi.filter((a) => a.type === 'function').map((a, i) => <tr key={i} className="mono"><td>{a.name}</td><td>{a.inputs?.map((p) => p.type).join(', ')}</td><td>{a.outputs?.map((p) => p.type).join(', ')}</td><td>{a.stateMutability}</td></tr>)}</tbody>
        </table>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-4 p-4">
      <Facts items={[['ELF', `${groupDigits(view.elf_bytes)} bytes`], ['.text', `${groupDigits(view.text_bytes)} bytes`], ['.rodata', `${groupDigits(view.rodata_bytes)} bytes`], ['Entry', `slot ${view.entry_pc}`], ['Functions', view.functions.length], ['Instructions', groupDigits(view.instructions)]]} />
      <table className="data-table"><caption>Syscalls</caption>
        <tbody>{view.syscalls.map((s) => <tr key={s.hash} className="mono"><td>{s.name}</td><td>{s.count} call site(s)</td><td className={s.supported ? 'text-positive' : 'text-negative'}>{s.supported ? 'supported' : 'traps on Rand'}</td></tr>)}</tbody>
      </table>
      <ZkvmWarnings warnings={view.warnings} />
    </div>
  );
}

/** What the zkVM will trap on, before anything runs. */
export function ZkvmWarnings({ warnings }: { warnings: WarningView[] }) {
  if (warnings.length === 0) return null;
  return (
    <section aria-label="zkVM warnings" className="flex flex-col gap-1.5 rounded-md border border-warning/40 p-3">
      <h3 className="text-xs text-text-mute">{warnings.length === 1 ? '1 warning' : `${warnings.length} warnings`} for the zkVM</h3>
      {warnings.map((w, i) => <p key={i} className={`text-xs ${w.fatal ? 'text-negative' : 'text-warning'}`}><span className="mono">{w.detail}</span>: {w.explanation}</p>)}
    </section>
  );
}
