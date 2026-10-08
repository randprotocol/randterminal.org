'use client';
import { useRef } from 'react';
import type { SyscallView } from '@/lib/types';

const TICK: Record<string, string> = { read_input: 'var(--text-mute)', read_public: 'var(--text-mute)', poseidon2: 'var(--accent)', poseidon2_len: 'var(--accent)', keccak: 'var(--accent)', sha256: 'var(--accent)', write_output: 'var(--positive)', halt: 'var(--positive)' };

export function Timeline({ total, clk, syscalls, onSeek }: { total: number; clk: number; syscalls: SyscallView[]; onSeek: (c: number) => void }) {
  const el = useRef<HTMLDivElement>(null);
  const seek = (x: number) => { const r = el.current!.getBoundingClientRect(); onSeek(Math.max(0, Math.min(total - 1, Math.round(((x - r.left) / r.width) * (total - 1))))); };
  return (
    <div className="flex items-center gap-2 px-4 py-2">
      <div ref={el} role="slider" aria-label="cycle" aria-valuemin={0} aria-valuemax={total - 1} aria-valuenow={clk} tabIndex={0}
        onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); seek(e.clientX); }} onPointerMove={(e) => e.buttons && seek(e.clientX)}
        className="relative h-7 flex-1 cursor-pointer rounded-sm bg-surface-2">
        <svg className="absolute inset-0 h-full w-full" preserveAspectRatio="none" viewBox={`0 0 ${Math.max(total, 1)} 28`}>
          {syscalls.map((s, i) => <rect key={i} x={s.clk} y={4} width={Math.max(1, total / 800)} height={20} fill={TICK[s.kind] ?? 'var(--text-mute)'} />)}
        </svg>
        <div className="absolute top-0 h-full w-0.5 bg-text-strong" style={{ left: `${(clk / Math.max(total - 1, 1)) * 100}%` }} />
      </div>
      <input aria-label="go to cycle" type="number" min={0} max={total - 1} value={clk} onChange={(e) => { if (e.target.value !== '' && Number.isFinite(Number(e.target.value))) onSeek(Math.max(0, Math.min(total - 1, Math.trunc(Number(e.target.value))))); }} className="mono h-8 w-28 rounded-sm border border-border bg-surface px-2 text-xs" />
    </div>
  );
}
