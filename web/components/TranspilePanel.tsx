'use client';
import { useMemo, useState } from 'react';
import { AnalysePanel } from './AnalysePanel';
import { BlockGraph } from './BlockGraph';
import { CViewer } from './CViewer';
import type { CompileView, OpLine, TranspileView } from '@/lib/types';

/** The C lines an op owns: from its marker to the line before the next marker. */
export function linesOf(opMap: OpLine[], pc: number, total: number): [number, number] | null {
  const i = opMap.findIndex((o) => o.pc === pc);
  if (i < 0) return null;
  return [opMap[i]!.line, (opMap[i + 1]?.line ?? total + 1) - 1];
}
/** The op whose C contains `line`: the last marker at or before it. */
export function opAt(opMap: OpLine[], line: number): number | null {
  let lo = 0, hi = opMap.length - 1, best: number | null = null;
  while (lo <= hi) { const m = (lo + hi) >> 1; if (opMap[m]!.line <= line) { best = opMap[m]!.pc; lo = m + 1; } else hi = m - 1; }
  return best;
}

export function TranspilePanel({ view, compile, selected, onSelect }: { view: TranspileView; compile: CompileView | null; selected: number | null; onSelect: (pc: number) => void }) {
  const [tab, setTab] = useState<'blocks' | 'listing'>('blocks');
  const total = view.stats.c_lines;
  const hl = useMemo(() => (selected == null ? null : linesOf(view.op_map, selected, total)), [selected, view.op_map, total]);
  const evm = compile?.lang === 'evm';
  const pcLabel = (pc: number) => (evm ? `0x${pc.toString(16).padStart(4, '0')}` : `${pc}`);
  return (
    <div className="grid h-full grid-rows-[auto_auto_1fr]">
      <p className="mono px-4 pt-3 text-xs text-text-soft">{view.stats.blocks} blocks · {view.stats.ops} {evm ? 'opcodes' : 'instructions'}{view.stats.functions ? ` · ${view.stats.functions} functions` : ''} · {view.stats.locals ? `${view.stats.locals} locals · ` : ''}{view.stats.c_lines} lines of C</p>
      <div className="px-4 py-2">{view.warnings.map((w, i) => <p key={i} className={`text-xs ${w.fatal ? 'text-negative' : 'text-warning'}`}>{w.detail}. {w.explanation}</p>)}</div>
      <div className="grid min-h-0 grid-cols-2 border-t border-border-soft">
        <div className="flex min-h-0 flex-col border-r border-border-soft">
          <div className="flex gap-1 p-2" role="tablist">{(['blocks', 'listing'] as const).map((t) => <button key={t} role="tab" aria-selected={tab === t} onClick={() => setTab(t)} className={`h-8 rounded-sm px-3 text-xs ${tab === t ? 'bg-surface-2 text-text-strong' : 'text-text-soft'}`}>{t === 'blocks' ? 'Block graph' : evm ? 'Opcodes' : 'Instructions'}</button>)}</div>
          <div className="min-h-0 flex-1">{tab === 'blocks' ? <BlockGraph graph={view.graph} selected={selected} onSelect={onSelect} pcLabel={pcLabel} /> : compile && <AnalysePanel view={compile} selected={selected} onSelect={onSelect} />}</div>
        </div>
        <CViewer source={view.c_source} highlight={hl} onLine={(l) => { const pc = opAt(view.op_map, l); if (pc != null) onSelect(pc); }} />
      </div>
    </div>
  );
}
