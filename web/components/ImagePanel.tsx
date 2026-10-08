'use client';
import { useMemo, useState } from 'react';
import { VirtualList } from './VirtualList';
import { api } from '@/lib/api';
import { groupDigits, hex32, hexAddr, short } from '@/lib/format';
import type { ImageView } from '@/lib/types';

export function cLineAt(map: [number, number][] | undefined, pc: number): number | null {
  if (!map?.length) return null;
  let lo = 0, hi = map.length - 1, best: number | null = null;
  while (lo <= hi) { const m = (lo + hi) >> 1; if (map[m]![0] <= pc) { best = map[m]![1]; lo = m + 1; } else hi = m - 1; }
  return best;
}

export function ImagePanel({ jobId, view, focusPc }: { jobId: string; view: ImageView; focusPc?: number | null }) {
  const [q, setQ] = useState('');
  const target = useMemo(() => { const pc = focusPc ?? (q ? parseInt(q.replace(/^0x/, ''), 16) : NaN); return Number.isFinite(pc) ? Math.floor((pc - view.base_pc) / 4) : null; }, [q, focusPc, view.base_pc]);
  const pct = Math.min(100, (view.words / view.cap_words) * 100);
  return (
    <div className="grid h-full grid-rows-[auto_1fr]">
      <div className="flex flex-col gap-2 p-4 text-sm">
        <div className="flex items-center gap-3"><span className="mono">{groupDigits(view.words)} words</span><span className="text-text-mute">of a {groupDigits(view.cap_words)}-word cap</span>
          <span className={view.fits ? 'text-positive' : 'text-negative'}>{view.fits ? 'fits' : 'does not fit'}</span>
          <a href={api.fileUrl(jobId, 'image.bin')} className="ml-auto text-xs text-accent">download image.bin</a></div>
        <div className="h-1.5 w-full rounded-full bg-surface-2"><div className={`h-1.5 rounded-full ${view.fits ? 'bg-accent' : 'bg-negative'}`} style={{ width: `${pct}%` }} /></div>
        <p className="mono text-xs text-text-soft">text {groupDigits(view.text_words)} · data {groupDigits(view.data_words)} · prologue {view.prologue_words} · base pc {hexAddr(view.base_pc)}</p>
        <p className="text-xs"><span className="text-text-mute">hc</span> <span className="mono" title={view.hc}>{short(view.hc, 16)}</span> <button className="text-accent" onClick={() => navigator.clipboard.writeText(view.hc)}>copy</button> <span className="text-text-mute">· the zkVM's commitment to this exact program</span></p>
        <p className="text-xs"><span className="text-text-mute">program id</span> <span className="mono" title={view.program_id}>{short(view.program_id, 16)}</span> <button className="text-accent" onClick={() => navigator.clipboard.writeText(view.program_id)}>copy</button> <span className="text-text-mute">· how the chain names it</span></p>
        {view.findings.map((f, i) => <p key={i} className="mono text-xs text-warning">{hexAddr(f.addr)} {f.rule}: {f.what}</p>)}
        {view.source_map_note && <p className="text-xs text-text-mute">{view.source_map_note}</p>}
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="jump to pc (hex)" aria-label="jump to pc" className="mono h-8 w-48 rounded-sm border border-border bg-surface px-2 text-xs" />
      </div>
      <VirtualList className="mono border-t border-border-soft text-xs" rows={view.disasm.length} scrollTo={target} render={(i) => {
        const d = view.disasm[i]!;
        const line = cLineAt(view.source_map?.pc_to_c, d.pc);
        return (
          <div className={`flex h-5 gap-4 px-3 ${i === target ? 'bg-hl' : ''}`}>
            <span className="w-20 text-text-mute">{hexAddr(d.pc)}</span><span className="w-24 text-text-mute">{hex32(d.word)}</span>
            <span className="flex-1">{d.text}{d.ecall && <span className="text-accent"> ; {d.ecall}</span>}</span>
            <span className="w-16 text-right text-text-mute">{line ? `C:${line}` : ''}</span>
          </div>
        );
      }} />
    </div>
  );
}
