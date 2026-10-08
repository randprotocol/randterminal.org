'use client';
import { useMemo } from 'react';
import { VirtualList } from './VirtualList';
import type { CompileView } from '@/lib/types';

type Row = { pc: number; text: string; note?: string; shade: boolean; header?: boolean; inert?: boolean };

export function rowsOf(view: CompileView): Row[] {
  if (view.lang === 'evm') return view.opcodes.map((o) => ({ pc: o.pc, text: `${o.jumpdest ? '◆ ' : '  '}${o.mnemonic}${o.push_hex ? ` 0x${o.push_hex}` : ''}`, note: `gas ${o.static_gas}`, shade: o.block % 2 === 1 }));
  const out: Row[] = [];
  view.functions.forEach((f, fi) => {
    out.push({ pc: f.entry, text: `function @${f.entry}`, shade: false, header: true });
    f.blocks.forEach((b, bi) => { b.insns.forEach((i) => out.push({ pc: i.pc, text: i.text, shade: bi % 2 === 1 }));
      // Terminator rows are not instructions; a synthetic trap block has no insns and its start is past the last slot.
      out.push({ pc: b.end, text: `  └ ${b.term}`, shade: bi % 2 === 1, note: `fn ${fi}`, inert: true });
    });
  });
  return out;
}

export function AnalysePanel({ view, selected, onSelect }: { view: CompileView; selected: number | null; onSelect: (pc: number) => void }) {
  const rows = useMemo(() => rowsOf(view), [view]);
  const found = selected == null ? -1 : rows.findIndex((r) => r.pc === selected && !r.header && !r.inert);
  const idx = found < 0 ? null : found;
  return (
    <VirtualList className="h-full mono text-xs" rows={rows.length} scrollTo={idx} render={(i) => {
      const r = rows[i]!;
      return (
        <button onClick={() => !r.header && !r.inert && onSelect(r.pc)} className={`flex h-5 w-full items-center gap-3 px-3 text-left ${r.header ? 'font-semibold text-text-strong' : r.inert ? 'text-text-mute' : ''} ${r.pc === selected && !r.header && !r.inert ? 'bg-hl' : r.shade ? 'bg-bg-soft' : ''}`}>
          <span className="w-14 text-right text-text-mute">{r.inert ? '' : view.lang === 'evm' ? `0x${r.pc.toString(16).padStart(4, '0')}` : r.pc}</span>
          <span className="flex-1 truncate">{r.text}</span>
          {r.note && <span className="text-text-mute">{r.note}</span>}
        </button>
      );
    }} />
  );
}
