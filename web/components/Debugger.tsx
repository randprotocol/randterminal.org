'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '@/lib/api';
import { changedRegs, nextSourceOp, syscallFrom } from '@/lib/replay';
import { Timeline } from './Timeline';
import { Registers } from './Registers';
import { Listing } from './Listing';
import { MemoryPanel } from './MemoryPanel';
import { SyscallLog } from './SyscallLog';
import { Icon } from './ui';
import type { CompileView, ImageView, RunSummary, StateView, SyscallView, TranspileView } from '@/lib/types';

const THROTTLE_MS = 60;
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
  // The server scans the trace (up to 2^20 cycles) for the next cycle at a different mapped source op.
  const hasMap = image.source_map != null;
  const nextOp = useCallback(async () => {
    const mine = ++gen.current;
    if (hasMap) setNote('searching…');
    const r = await nextSourceOp((from, dir) => api.nextSrc(run.rid, from, dir), clk, hasMap);
    if (mine !== gen.current) return;
    if ('seek' in r) seek(r.seek); else setNote(r.note);
  }, [hasMap, clk, run.rid, seek]);

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

  const btn = 'inline-flex h-8 min-w-8 items-center justify-center rounded-sm px-2 text-xs text-text-soft hover:bg-surface-2 hover:text-text-strong';
  return (
    <div ref={root} tabIndex={0} onKeyDown={onKeyDown} aria-label="debugger" className="@container grid h-full grid-rows-[auto_auto_1fr] outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent">
      <Timeline total={total} clk={clk} syscalls={syscalls ?? []} onSeek={seek} />
      <div className="flex flex-wrap items-center gap-1 px-4 pb-2" role="toolbar" aria-label="step controls">
        <button className={btn} onClick={() => seek(0)} title="Start" aria-label="start"><Icon name="start" /></button>
        <button className={btn} onClick={() => seek(clk - 100)} title="−100 (Shift+←)">−100</button>
        <button className={btn} onClick={() => seek(clk - 1)} title="Back one cycle (←)" aria-label="back one cycle"><Icon name="back" /></button>
        <button className={btn} onClick={() => seek(clk + 1)} title="Forward one cycle (→)" aria-label="forward one cycle"><Icon name="forward" /></button>
        <button className={btn} onClick={() => seek(clk + 100)} title="+100 (Shift+→)">+100</button>
        <button className={btn} onClick={() => nextSyscall(1)} title="Next syscall (s)">Next syscall</button>
        <button className={btn} onClick={() => void nextOp()} title="Next source op (o)">Next source op</button>
        <button className={btn} onClick={() => seek(total - 1)} title="End" aria-label="end"><Icon name="end" /></button>
        {note && <span role="status" className="text-xs text-text-mute">{note}</span>}
      </div>
      <div className="min-h-0 overflow-auto">
      {stateErr && <div role="alert" className="mx-4 rounded-md border border-negative/60 p-3 text-xs"><span className="text-negative">Could not load cycle {clk}: {stateErr}</span> <button className="link ml-2 underline" onClick={() => setRetry((n) => n + 1)}>Retry</button></div>}
      {!state && !stateErr && <p className="px-4 text-xs text-text-mute">loading cycle {clk}…</p>}
      {state && (
        <div className="grid min-h-0 grid-cols-1 border-t border-border-soft @2xl:h-full @2xl:grid-cols-[minmax(0,1.2fr)_minmax(320px,1fr)] @5xl:grid-cols-[minmax(0,1fr)_330px_minmax(0,1fr)]">
          <div className="min-h-0 overflow-auto @2xl:border-r @2xl:border-border-soft"><Listing state={state} image={image} transpile={transpile} compile={compile} /></div>
          {/* One tree: stacked in the right column, or two columns of their own when the debugger is wide. */}
          <div className="min-h-0 overflow-auto @5xl:contents">
            <div className="min-h-0 @5xl:overflow-auto @5xl:border-r @5xl:border-border-soft">
              <Registers regs={state.regs} changed={changed} writes={state.cycle.writes_rd ? state.cycle.rd : null} />
              <SyscallLog syscalls={syscalls ?? []} clk={state.clk} onSeek={seek} />
            </div>
            <div className="min-h-0 @5xl:overflow-auto"><MemoryPanel rid={run.rid} state={state} /></div>
          </div>
        </div>
      )}
      </div>
    </div>
  );
}
