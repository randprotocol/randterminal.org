'use client';
import { useEffect, useRef, useState } from 'react';
import { Icon } from './ui';
import type { ConsoleLine } from '@/lib/types';

export function Console({ lines }: { lines: ConsoleLine[] }) {
  const [open, setOpen] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => { if (open) end.current?.scrollIntoView({ block: 'end' }); }, [lines, open]);
  const last = lines.at(-1)?.text.trim().split('\n').at(-1);
  return (
    <section className="border-t border-border-soft bg-bg-soft" aria-label="console">
      <button onClick={() => setOpen(!open)} aria-expanded={open} className="flex h-8 w-full items-center gap-2 px-3 text-xs text-text-soft max-lg:h-11">
        <span className={open ? '' : '-rotate-90'}><Icon name="chevron" /></span> Console <span className="text-text-mute">{lines.length}</span>
        {!open && last && <span className="mono min-w-0 flex-1 truncate text-left text-text-mute">{last}</span>}
      </button>
      {open && (
        <div className="mono h-48 overflow-auto px-3 pb-2 text-xs">
          {lines.map((l, i) => (
            <pre key={i} className={`whitespace-pre-wrap ${l.stream === 'stderr' ? 'text-warning' : 'text-text-soft'}`}><span className="text-text-mute">{l.stage} </span>{l.text}</pre>
          ))}
          <div ref={end} />
        </div>
      )}
    </section>
  );
}
