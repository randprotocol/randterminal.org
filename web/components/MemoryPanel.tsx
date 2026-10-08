'use client';
import { useEffect, useRef, useState } from 'react';
import { api } from '@/lib/api';
import { byteAddr, hex32, hexAddr, memAccess } from '@/lib/format';
import type { StateView } from '@/lib/types';

const QUIET_MS = 60;

export function MemoryPanel({ rid, state }: { rid: string; state: StateView }) {
  const c = state.cycle;
  const touched = c.mem_read || c.mem_write;
  // The window follows the latest memory access unless the reader typed an address; a typed address holds until the next access moves.
  const lastAuto = useRef(0);
  if (touched) lastAuto.current = c.mem_addr & ~15;
  const auto = lastAuto.current;
  const [typed, setTyped] = useState<{ base: number; auto: number } | null>(null);
  const base = typed && typed.auto === auto ? typed.base : auto;
  const key = `${rid}:${state.clk}:${base}`;
  const [got, setGot] = useState<{ key: string; words: number[] } | null>(null);
  const [err, setErr] = useState<{ key: string; message: string } | null>(null);
  const [retry, setRetry] = useState(0);
  // Trailing debounce: while the reader scrubs (a new position within QUIET_MS) no window is
  // fetched; the position the reader stops on always is. Keeps the debugger inside its rate limit.
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      api.memory(rid, state.clk, base, 64).then((m) => { if (live) { setErr(null); setGot({ key, words: m.words }); } }).catch((e) => { if (live) setErr({ key, message: e instanceof Error ? e.message : String(e) }); });
    }, QUIET_MS);
    return () => { live = false; clearTimeout(t); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, retry]);
  const words = got?.key === key ? got.words : null;
  const failed = err?.key === key ? err.message : null;
  return (
    <section className="border-t border-border-soft p-3 text-xs" aria-label="memory">
      <p className="mono">{memAccess(c) ?? <span className="text-text-mute">no memory access this cycle</span>}</p>
      <label className="mt-2 block text-text-mute">window at byte address <input className="mono ml-1 h-7 w-32 rounded-sm border border-border bg-surface px-2" defaultValue={hexAddr(byteAddr(base))} key={base}
        onBlur={(e) => { const v = parseInt(e.target.value.replace(/^0x/, ''), 16); if (Number.isFinite(v)) setTyped({ base: (v >>> 2) & ~15, auto }); }} /></label>
      {failed && <p role="alert" className="mt-2 text-negative">Could not load memory: {failed} <button className="text-accent" onClick={() => { setErr(null); setRetry((n) => n + 1); }}>Retry</button></p>}
      {!words && !failed && <p className="mt-2 text-text-mute">loading memory…</p>}
      {words && (
        <div className="mono mt-2 grid grid-cols-[auto_repeat(4,max-content)] gap-x-3 overflow-x-auto">
          {Array.from({ length: 16 }, (_, r) => (
            <div key={r} className="contents">
              <span className="text-text-mute">{hexAddr(byteAddr(base + r * 4))}</span>
              {words.slice(r * 4, r * 4 + 4).map((w, k) => <span key={k} className={touched && base + r * 4 + k === c.mem_addr ? 'bg-hl' : ''}>{hex32(w)}</span>)}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
