'use client';
import { useState } from 'react';
import type { CompileView } from '@/lib/types';
import { groupDigits } from '@/lib/format';

export function CompilePanel({ view }: { view: CompileView }) {
  const [which, setWhich] = useState<'runtime' | 'creation'>('runtime');
  if (view.lang === 'evm') {
    const hex = which === 'runtime' ? view.runtime_hex : view.creation_hex;
    return (
      <div className="flex flex-col gap-4 p-4">
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="text-text-mute">contract</dt><dd>{view.contract}{view.contracts.length > 1 && <span className="text-text-mute"> (of {view.contracts.join(', ')})</span>}</dd>
          <dt className="text-text-mute">runtime bytecode</dt><dd className="mono">{groupDigits(view.size_bytes)} bytes · {view.blocks} blocks · {view.jumpdests} jump destinations</dd>
        </dl>
        <div>
          <div className="mb-2 flex gap-1" role="tablist">
            {(['runtime', 'creation'] as const).map((w) => <button key={w} role="tab" aria-selected={which === w} onClick={() => setWhich(w)} className={`h-8 rounded-sm px-3 text-xs ${which === w ? 'bg-surface-2 text-text-strong' : 'text-text-soft'}`}>{w}</button>)}
            <button onClick={() => { try { void navigator.clipboard.writeText(hex).catch(() => {}); } catch { /* clipboard unavailable */ } }} className="ml-auto h-8 rounded-sm px-3 text-xs text-text-soft hover:bg-surface">Copy</button>
          </div>
          <pre className="mono max-h-64 overflow-auto rounded-sm bg-bg-soft p-3 text-xs break-all whitespace-pre-wrap">{hex || '(empty)'}</pre>
          <p className="mt-1 text-xs text-text-mute">The transpiler takes the runtime code: what the chain stores, not the constructor that deploys it.</p>
        </div>
        <table className="text-xs"><caption className="pb-1 text-left text-text-mute">ABI</caption>
          <thead><tr className="text-left text-text-mute"><th>function</th><th>inputs</th><th>outputs</th><th>mutability</th></tr></thead>
          <tbody>{view.abi.filter((a) => a.type === 'function').map((a, i) => <tr key={i} className="mono"><td>{a.name}</td><td>{a.inputs?.map((p) => p.type).join(', ')}</td><td>{a.outputs?.map((p) => p.type).join(', ')}</td><td>{a.stateMutability}</td></tr>)}</tbody>
        </table>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-4 p-4">
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm mono">
        <dt className="text-text-mute">ELF</dt><dd>{groupDigits(view.elf_bytes)} bytes</dd>
        <dt className="text-text-mute">.text / .rodata</dt><dd>{groupDigits(view.text_bytes)} / {groupDigits(view.rodata_bytes)} bytes</dd>
        <dt className="text-text-mute">entry</dt><dd>slot {view.entry_pc}</dd>
        <dt className="text-text-mute">functions</dt><dd>{view.functions.length} · {groupDigits(view.instructions)} instructions</dd>
      </dl>
      <table className="text-xs"><caption className="pb-1 text-left text-text-mute">Syscalls</caption>
        <tbody>{view.syscalls.map((s) => <tr key={s.hash} className="mono"><td>{s.name}</td><td>{s.count} call site(s)</td><td className={s.supported ? 'text-positive' : 'text-negative'}>{s.supported ? 'supported' : 'traps on Rand'}</td></tr>)}</tbody>
      </table>
      {view.warnings.map((w, i) => <p key={i} className="text-xs text-warning">{w.detail}: {w.explanation}</p>)}
    </div>
  );
}
