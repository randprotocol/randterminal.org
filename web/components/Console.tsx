'use client';
import { useEffect, useRef, useState } from 'react';
import type { ConsoleLine } from '@/lib/types';

export function Console({ lines }: { lines: ConsoleLine[] }) {
  const [open, setOpen] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => { if (open) end.current?.scrollIntoView({ block: 'end' }); }, [lines, open]);
  return (
    <section className="border-t border-border-soft bg-bg-soft" aria-label="console">
      <button onClick={() => setOpen(!open)} aria-expanded={open} className="flex h-8 w-full items-center gap-2 px-3 text-xs text-text-soft max-lg:h-11">
        <span>{open ? '▾' : '▸'}</span> Console <span className="text-text-mute">({lines.length})</span>
      </button>
      {open && (
        <div className="mono h-40 overflow-auto px-3 pb-2 text-xs">
          {lines.map((l, i) => (
            <pre key={i} className={`whitespace-pre-wrap ${l.stream === 'stderr' ? 'text-warning' : 'text-text-soft'}`}><span className="text-text-mute">[{l.stage}] </span>{l.text}</pre>
          ))}
          <div ref={end} />
        </div>
      )}
    </section>
  );
}
