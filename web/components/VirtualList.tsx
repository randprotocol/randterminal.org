'use client';
import { useEffect, useRef, useState } from 'react';
import { visibleRange } from '@/lib/virtual';

export function VirtualList({ rows, rowHeight = 20, render, scrollTo, scrollToEnd, className = '' }: { rows: number; rowHeight?: number; render: (i: number) => React.ReactNode; scrollTo?: number | null; scrollToEnd?: number | null; className?: string }) {
  const el = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(0);
  const [vh, setVh] = useState(600);
  useEffect(() => { const e = el.current; if (!e) return; const ro = new ResizeObserver(() => setVh(e.clientHeight)); ro.observe(e); return () => ro.disconnect(); }, []);
  useEffect(() => {
    const e = el.current;
    if (e == null || scrollTo == null) return;
    // Scroll only when no row of [scrollTo, scrollToEnd] is in view.
    const y = scrollTo * rowHeight, yEnd = ((scrollToEnd ?? scrollTo) + 1) * rowHeight;
    if (yEnd <= e.scrollTop || y >= e.scrollTop + e.clientHeight) e.scrollTop = Math.max(0, y - e.clientHeight / 3);
  }, [scrollTo, scrollToEnd, rowHeight]);
  const [a, b] = visibleRange(top, vh, rowHeight, rows);
  return (
    <div ref={el} onScroll={(e) => setTop(e.currentTarget.scrollTop)} className={`relative overflow-auto ${className}`}>
      <div style={{ height: rows * rowHeight }} />
      <div style={{ position: 'absolute', top: a * rowHeight, left: 0, right: 0 }}>
        {Array.from({ length: b - a }, (_, k) => <div key={a + k} style={{ height: rowHeight }}>{render(a + k)}</div>)}
      </div>
    </div>
  );
}
