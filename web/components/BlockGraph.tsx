'use client';
import { useMemo } from 'react';
import { layout } from '@/lib/graph-layout';
import type { Graph } from '@/lib/types';

export function BlockGraph({ graph, selected, onSelect, pcLabel }: { graph: Graph; selected: number | null; onSelect: (pc: number) => void; pcLabel: (pc: number) => string }) {
  const l = useMemo(() => layout(graph), [graph]);
  const W = 120, H = 36;
  const sel = selected == null ? null : graph.nodes.find((n) => selected >= n.start && selected <= n.end)?.id;
  return (
    <div className="overflow-auto p-3">
      {l.truncated && <p className="pb-2 text-xs text-text-mute">Showing the first 400 of {graph.nodes.length} blocks.</p>}
      <svg width={l.width + 40} height={l.height + 20} role="img" aria-label={`control-flow graph, ${graph.nodes.length} blocks`}>
        <defs><marker id="arr" viewBox="0 0 6 6" refX="5" refY="3" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0L6,3L0,6z" fill="var(--text-mute)" /></marker></defs>
        {l.edges.map((e, i) => {
          const back = e.points[0]![1] === e.points[1]![1];
          return <polyline key={i} points={e.points.map((p) => p.join(',')).join(' ')} fill="none" markerEnd="url(#arr)" strokeWidth={1.2}
            stroke={e.kind === 'taken' || e.kind === 'jump' || e.kind === 'call' ? 'var(--accent)' : 'var(--text-mute)'} strokeDasharray={back ? '4 3' : undefined} />;
        })}
        {l.nodes.map((n) => (
          <g key={n.id} transform={`translate(${n.x},${n.y})`} onClick={() => onSelect(n.start)} className="cursor-pointer">
            <rect width={W} height={H} rx={6} fill="var(--surface)" stroke={n.id === sel ? 'var(--accent)' : 'var(--border)'} strokeWidth={n.id === sel ? 2 : 1} />
            <text x={8} y={15} className="mono" fontSize={11} fill="var(--text-strong)">{pcLabel(n.start)}</text>
            <text x={8} y={29} fontSize={10} fill="var(--text-mute)">{n.term.length > 18 ? `${n.term.slice(0, 17)}…` : n.term}</text>
            <title>{`${pcLabel(n.start)}–${pcLabel(n.end)}: ${n.term}`}</title>
          </g>
        ))}
      </svg>
    </div>
  );
}
