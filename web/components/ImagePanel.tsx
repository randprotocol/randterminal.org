'use client';
import { useMemo, useState } from 'react';
import { VirtualList } from './VirtualList';
import { Facts, Identity } from './ui';
import { api } from '@/lib/api';
import { groupDigits, hex32, hexAddr } from '@/lib/format';
import type { ImageView } from '@/lib/types';

export function cLineAt(map: [number, number][] | undefined, pc: number): number | null {
  if (!map?.length) return null;
  let lo = 0, hi = map.length - 1, best: number | null = null;
  while (lo <= hi) { const m = (lo + hi) >> 1; if (map[m]![0] <= pc) { best = map[m]![1]; lo = m + 1; } else hi = m - 1; }
  return best;
}

/** Index of the last disasm row whose pc is at or below `pc` (rows are sorted by pc); null before the first. */
export function rowAt(disasm: { pc: number }[], pc: number): number | null {
  let lo = 0, hi = disasm.length - 1, best: number | null = null;
  while (lo <= hi) { const m = (lo + hi) >> 1; if (disasm[m]!.pc <= pc) { best = m; lo = m + 1; } else hi = m - 1; }
  return best;
}
export function parseHexPc(q: string): number | null {
  const m = /^\s*(?:0x)?([0-9a-f]{1,8})\s*$/i.exec(q);
  return m ? parseInt(m[1]!, 16) : null;
}

export function ImagePanel({ jobId, view }: { jobId: string; view: ImageView }) {
  const [q, setQ] = useState('');
  const target = useMemo(() => { const pc = parseHexPc(q); return pc == null ? null : rowAt(view.disasm, pc); }, [q, view.disasm]);
  const pct = Math.min(100, (view.words / view.cap_words) * 100);
  return (
    <div className="grid h-full grid-rows-[auto_1fr]">
      <div className="flex flex-col gap-3 p-4">
        <div className="flex items-baseline gap-2"><span className="mono text-text-strong [word-spacing:-.25em]">{groupDigits(view.words)} words</span><span className="text-text-mute">of a {groupDigits(view.cap_words)}-word cap</span>
          <span className={view.fits ? 'text-positive' : 'text-negative'}>{view.fits ? 'fits' : 'does not fit'}</span>
          <a href={api.fileUrl(jobId, 'image.bin')} className="quiet ml-auto">Download image.bin</a></div>
        <div className="h-1 w-full rounded-full bg-surface-2"><div className={`h-1 rounded-full ${view.fits ? 'bg-accent' : 'bg-negative'}`} style={{ width: `${pct}%` }} /></div>
        <Facts items={[['Text', groupDigits(view.text_words)], ['Data', groupDigits(view.data_words)], ['Prologue', view.prologue_words], ['Base pc', hexAddr(view.base_pc)]]} />
        <Identity name="hc" meaning="the zkVM's commitment to this exact program" value={view.hc} />
        <Identity name="Program id" meaning="how the chain names it" value={view.program_id} />
        {view.findings.map((f, i) => <p key={i} className="mono text-xs text-warning">{hexAddr(f.addr)} {f.rule}: {f.what}</p>)}
        {view.source_map_note && <p className="text-xs text-text-mute">{view.source_map_note}</p>}
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Jump to pc (hex)" aria-label="jump to pc" className="mono h-8 w-48 rounded-sm border border-border bg-surface px-2 text-xs" />
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
