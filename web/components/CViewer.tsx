'use client';
import { useMemo } from 'react';
import { VirtualList } from './VirtualList';

export function CViewer({ source, highlight, onLine }: { source: string; highlight: [number, number] | null; onLine: (line: number) => void }) {
  const lines = useMemo(() => source.split('\n'), [source]);
  return (
    <VirtualList className="h-full mono text-xs" rows={lines.length} scrollTo={highlight ? highlight[0] - 1 : null} scrollToEnd={highlight ? highlight[1] - 1 : null} render={(i) => {
      const n = i + 1;
      const on = highlight && n >= highlight[0] && n <= highlight[1];
      return (
        <div onClick={() => onLine(n)} className={`flex h-5 cursor-pointer gap-3 px-3 ${on ? 'bg-hl' : ''}`}>
          <span className="w-12 select-none text-right text-text-mute">{n}</span>
          <span className="whitespace-pre">{lines[i]}</span>
        </div>
      );
    }} />
  );
}
