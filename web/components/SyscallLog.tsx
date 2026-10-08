'use client';
import { useMemo } from 'react';
import type { SyscallView } from '@/lib/types';

export function SyscallLog({ syscalls, clk, onSeek }: { syscalls: SyscallView[]; clk: number; onSeek: (c: number) => void }) {
  const outputs = useMemo(() => {
    const o: (number | null)[] = Array(8).fill(null);
    for (const s of syscalls) { if (s.clk > clk) break; const m = /^out\[(\d)\] = (\d+)$/.exec(s.detail); if (s.kind === 'write_output' && m) o[Number(m[1])] = Number(m[2]); }
    return o;
  }, [syscalls, clk]);
  const current = syscalls.reduce((acc, s, i) => (s.clk <= clk ? i : acc), -1);
  return (
    <section className="border-t border-border-soft p-3 text-xs" aria-label="syscalls">
      <p className="mono text-text-soft">out {outputs.map((w) => (w == null ? '·' : w)).join(' ')}</p>
      <ol className="mono mt-2 max-h-48 overflow-auto">
        {syscalls.map((s, i) => (
          <li key={i}><button onClick={() => onSeek(s.clk)} className={`w-full text-left ${i === current ? 'bg-hl' : ''}`}><span className="text-text-mute">{s.clk}</span> {s.kind} <span className="text-text-mute">{s.detail}</span></button></li>
        ))}
      </ol>
    </section>
  );
}
