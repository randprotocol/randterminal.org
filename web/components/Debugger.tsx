'use client';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api } from '@/lib/api';
import { changedRegs, nextMatching, PageCache } from '@/lib/replay';
import { Timeline } from './Timeline';
import { Registers } from './Registers';
import { Listing } from './Listing';
import { MemoryPanel } from './MemoryPanel';
import { SyscallLog } from './SyscallLog';
import type { CompileView, ImageView, RunSummary, StateView, SyscallView, TranspileView } from '@/lib/types';

export function Debugger({ run, image, transpile, compile }: { run: RunSummary; image: ImageView; transpile: TranspileView | null; compile: CompileView | null }) {
  const total = run.cycles;
  const cache = useMemo(() => new PageCache((f, c) => api.cycles(run.rid, f, c).then((p) => p.cycles)), [run.rid]);
  const [clk, setClk] = useState(() => { const m = /clk=(\d+)/.exec(typeof location !== 'undefined' ? location.hash : ''); return m ? Math.min(Number(m[1]), total - 1) : 0; });
  const [state, setState] = useState<StateView | null>(null);
  const prev = useRef<number[] | null>(null);
  const [changed, setChanged] = useState<Set<number>>(new Set());
  const [syscalls, setSyscalls] = useState<SyscallView[]>([]);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => { api.syscalls(run.rid).then(setSyscalls); }, [run.rid]);
  useEffect(() => {
    let live = true;
    api.state(run.rid, clk).then((s) => { if (!live) return; setChanged(changedRegs(prev.current, s.regs)); prev.current = s.regs; setState(s); });
    history.replaceState(null, '', `#clk=${clk}`);
    return () => { live = false; };
  }, [run.rid, clk]);

  const seek = useCallback((c: number) => { setNote(null); setClk(Math.max(0, Math.min(total - 1, c))); }, [total]);
  const nextSyscall = useCallback(async (dir: 1 | -1) => { const c = await nextMatching(cache, clk, dir, (x) => x.syscall, total); c == null ? setNote('no further syscall') : seek(c); }, [cache, clk, total, seek]);
  const nextOp = useCallback(async () => {
    if (!state?.src_pc && state?.src_pc !== 0) { setNote('no source map for this build'); return; }
    for (let c = clk + 1, n = 0; c < total && n < 64; c++, n++) {
      const s = await api.state(run.rid, c);
      if (s.src_pc != null && s.src_pc !== state.src_pc) { seek(c); return; }
    }
    setNote('the next source op is more than 64 cycles away; use the timeline');
  }, [state, clk, total, run.rid, seek]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === 'INPUT') return;
      if (e.key === 'ArrowRight') seek(clk + (e.shiftKey ? 100 : 1));
      else if (e.key === 'ArrowLeft') seek(clk - (e.shiftKey ? 100 : 1));
      else if (e.key === 's') void nextSyscall(e.shiftKey ? -1 : 1);
      else if (e.key === 'o') void nextOp();
      else return;
      e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [clk, seek, nextSyscall, nextOp]);

  const btn = 'h-8 rounded-sm px-2 text-xs text-text-soft hover:bg-surface';
  return (
    <div className="grid h-full grid-rows-[auto_auto_1fr]">
      <Timeline total={total} clk={clk} syscalls={syscalls} onSeek={seek} />
      <div className="flex flex-wrap items-center gap-1 px-4 pb-2" role="toolbar" aria-label="step controls">
        <button className={btn} onClick={() => seek(0)} title="start">⏮</button>
        <button className={btn} onClick={() => seek(clk - 100)} title="−100 (Shift+←)">−100</button>
        <button className={btn} onClick={() => seek(clk - 1)} title="−1 (←)">◀</button>
        <button className={btn} onClick={() => seek(clk + 1)} title="+1 (→)">▶</button>
        <button className={btn} onClick={() => seek(clk + 100)} title="+100 (Shift+→)">+100</button>
        <button className={btn} onClick={() => nextSyscall(1)} title="next syscall (s)">next syscall</button>
        <button className={btn} onClick={nextOp} title="next source op (o)">next source op</button>
        <button className={btn} onClick={() => seek(total - 1)} title="end">⏭</button>
        {note && <span className="text-xs text-text-mute">{note}</span>}
      </div>
      {state && (
        <div className="grid min-h-0 grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)] gap-0 border-t border-border-soft">
          <div className="min-h-0 overflow-auto border-r border-border-soft"><Listing state={state} image={image} transpile={transpile} compile={compile} /></div>
          <div className="min-h-0 overflow-auto">
            <Registers regs={state.regs} changed={changed} writes={state.cycle.writes_rd ? state.cycle.rd : null} />
            <MemoryPanel rid={run.rid} state={state} />
            <SyscallLog syscalls={syscalls} clk={clk} onSeek={seek} />
          </div>
        </div>
      )}
    </div>
  );
}
