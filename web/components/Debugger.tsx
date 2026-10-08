'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '@/lib/api';
import { changedRegs, syscallFrom } from '@/lib/replay';
import { Timeline } from './Timeline';
import { Registers } from './Registers';
import { Listing } from './Listing';
import { MemoryPanel } from './MemoryPanel';
import { SyscallLog } from './SyscallLog';
import type { CompileView, ImageView, RunSummary, StateView, SyscallView, TranspileView } from '@/lib/types';

const THROTTLE_MS = 60;
const OP_SCAN = 64;
const msg = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function Debugger({ run, image, transpile, compile }: { run: RunSummary; image: ImageView; transpile: TranspileView | null; compile: CompileView | null }) {
  const total = run.cycles;
  const [clk, setClk] = useState(() => { const m = /clk=(\d+)/.exec(typeof location !== 'undefined' ? location.hash : ''); return m ? Math.min(Number(m[1]), total - 1) : 0; });
  const [state, setState] = useState<StateView | null>(null);
  const [stateErr, setStateErr] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const prev = useRef<number[] | null>(null);
  const [changed, setChanged] = useState<Set<number>>(new Set());
  const [syscalls, setSyscalls] = useState<SyscallView[] | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const gen = useRef(0);      // bumped by every seek: a search that finishes after the reader moved must not seek
  const noMap = useRef(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => { root.current?.focus({ preventScroll: true }); }, []);
  useEffect(() => { let live = true; api.syscalls(run.rid).then((s) => live && setSyscalls(s)).catch((e) => live && setNote(`could not load the syscall log: ${msg(e)}`)); return () => { live = false; }; }, [run.rid]);

  // /state is fetched at most every THROTTLE_MS while the reader scrubs; the last position always lands.
  const lastFetch = useRef(0);
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      lastFetch.current = Date.now();
      api.state(run.rid, clk).then((s) => { if (!live) return; setStateErr(null); setChanged(changedRegs(prev.current, s.regs)); prev.current = s.regs; setState(s); })
        .catch((e) => { if (live) setStateErr(msg(e)); });
    }, Math.max(0, THROTTLE_MS - (Date.now() - lastFetch.current)));
    return () => { live = false; clearTimeout(t); };
  }, [run.rid, clk, retry]);
  useEffect(() => { history.replaceState(null, '', `#clk=${clk}`); }, [clk]);

  const seek = useCallback((c: number) => { gen.current++; setNote(null); setClk(Math.max(0, Math.min(total - 1, c))); }, [total]);
  const nextSyscall = useCallback((dir: 1 | -1) => {
    if (!syscalls) { setNote('loading the syscall log…'); return; }
    const c = syscallFrom(syscalls, clk, dir);
    if (c == null) setNote(dir === 1 ? 'no further syscall' : 'no earlier syscall'); else seek(c);
  }, [syscalls, clk, seek]);
  const nextOp = useCallback(async () => {
    if (!state) { setNote('loading…'); return; }
    if (noMap.current) { setNote('no source map for this build'); return; }
    const mine = ++gen.current;
    setNote('searching…');
    try {
      let sawMapped = state.src_pc != null;
      for (let c = clk + 1, n = 0; c < total && n < OP_SCAN; c++, n++) {
        const s = await api.state(run.rid, c);
        if (mine !== gen.current) return;
        if (s.src_pc != null) { sawMapped = true; if (s.src_pc !== state.src_pc) { seek(c); return; } }
      }
      if (!sawMapped) { noMap.current = true; setNote('no source map for this build'); }
      else setNote(`the next source op is more than ${OP_SCAN} cycles away; use the timeline`);
    } catch (e) { if (mine === gen.current) setNote(`could not search: ${msg(e)}`); }
  }, [state, clk, total, run.rid, seek]);

  // Scoped to the debugger so typing in the editor or any field never steps the run.
  const onKeyDown = (e: React.KeyboardEvent) => {
    const t = e.target as HTMLElement;
    if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || t.isContentEditable || ['INPUT', 'SELECT', 'TEXTAREA'].includes(t.tagName)) return;
    if (e.key === 'ArrowRight') seek(clk + (e.shiftKey ? 100 : 1));
    else if (e.key === 'ArrowLeft') seek(clk - (e.shiftKey ? 100 : 1));
    else if (e.key === 's' || e.key === 'S') nextSyscall(e.shiftKey ? -1 : 1);
    else if (e.key === 'o') void nextOp();
    else return;
    e.preventDefault();
  };

  const btn = 'h-8 rounded-sm px-2 text-xs text-text-soft hover:bg-surface';
  return (
    <div ref={root} tabIndex={0} onKeyDown={onKeyDown} aria-label="debugger" className="grid h-full grid-rows-[auto_auto_1fr] outline-none">
      <Timeline total={total} clk={clk} syscalls={syscalls ?? []} onSeek={seek} />
      <div className="flex flex-wrap items-center gap-1 px-4 pb-2" role="toolbar" aria-label="step controls">
        <button className={btn} onClick={() => seek(0)} title="start">⏮</button>
        <button className={btn} onClick={() => seek(clk - 100)} title="−100 (Shift+←)">−100</button>
        <button className={btn} onClick={() => seek(clk - 1)} title="−1 (←)">◀</button>
        <button className={btn} onClick={() => seek(clk + 1)} title="+1 (→)">▶</button>
        <button className={btn} onClick={() => seek(clk + 100)} title="+100 (Shift+→)">+100</button>
        <button className={btn} onClick={() => nextSyscall(1)} title="next syscall (s)">next syscall</button>
        <button className={btn} onClick={() => void nextOp()} title="next source op (o)">next source op</button>
        <button className={btn} onClick={() => seek(total - 1)} title="end">⏭</button>
        {note && <span role="status" className="text-xs text-text-mute">{note}</span>}
      </div>
      <div className="min-h-0">
      {stateErr && <div role="alert" className="mx-4 rounded-md border border-negative/60 p-3 text-xs"><span className="text-negative">Could not load cycle {clk}: {stateErr}</span> <button className="ml-2 text-accent" onClick={() => setRetry((n) => n + 1)}>Retry</button></div>}
      {!state && !stateErr && <p className="px-4 text-xs text-text-mute">loading cycle {clk}…</p>}
      {state && (
        <div className="grid min-h-0 grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)] gap-0 border-t border-border-soft">
          <div className="min-h-0 overflow-auto border-r border-border-soft"><Listing state={state} image={image} transpile={transpile} compile={compile} /></div>
          <div className="min-h-0 overflow-auto">
            <Registers regs={state.regs} changed={changed} writes={state.cycle.writes_rd ? state.cycle.rd : null} />
            <MemoryPanel rid={run.rid} state={state} />
            <SyscallLog syscalls={syscalls ?? []} clk={state.clk} onSeek={seek} />
          </div>
        </div>
      )}
      </div>
    </div>
  );
}
