'use client';
import { useEffect, useState } from 'react';
import { api } from '@/lib/api';
import { byteAddr, hex32, hexAddr } from '@/lib/format';
import type { StateView } from '@/lib/types';

export function MemoryPanel({ rid, state }: { rid: string; state: StateView }) {
  const c = state.cycle;
  const touched = c.mem_read || c.mem_write;
  const [base, setBase] = useState<number>(0);
  const [words, setWords] = useState<number[]>([]);
  useEffect(() => { if (touched) setBase(c.mem_addr & ~15); }, [touched, c.mem_addr]);
  useEffect(() => { let live = true; api.memory(rid, state.clk, base, 64).then((m) => live && setWords(m.words)); return () => { live = false; }; }, [rid, state.clk, base]);
  return (
    <section className="border-t border-border-soft p-3 text-xs" aria-label="memory">
      <p className="mono">{touched ? <>{c.mem_write ? 'store' : 'load'} {hexAddr(byteAddr(c.mem_addr))} {c.mem_write ? '←' : '→'} {hex32(c.mem_val)}</> : <span className="text-text-mute">no memory access this cycle</span>}</p>
      <label className="mt-2 block text-text-mute">window at byte address <input className="mono ml-1 h-7 w-32 rounded-sm border border-border bg-surface px-2" defaultValue={hexAddr(byteAddr(base))} key={base}
        onBlur={(e) => { const v = parseInt(e.target.value.replace(/^0x/, ''), 16); if (Number.isFinite(v)) setBase((v >>> 2) & ~15); }} /></label>
      <div className="mono mt-2 grid grid-cols-[auto_repeat(4,max-content)] gap-x-3 overflow-x-auto">
        {Array.from({ length: 16 }, (_, r) => (
          <div key={r} className="contents">
            <span className="text-text-mute">{hexAddr(byteAddr(base + r * 4))}</span>
            {words.slice(r * 4, r * 4 + 4).map((w, k) => <span key={k} className={touched && base + r * 4 + k === c.mem_addr ? 'bg-hl' : ''}>{hex32(w)}</span>)}
          </div>
        ))}
      </div>
    </section>
  );
}
