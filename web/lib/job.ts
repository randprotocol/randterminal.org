'use client';
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import { api, subscribeJob } from './api';
import type { ConsoleLine, FileIn, JobStreamEvent, JobView, Lang, NewJob, StageError, StageState, StageStatus } from './types';

export type Stage = 'compile' | 'analyse' | 'transpile' | 'image' | 'run';
export const STAGES: { id: Stage; n: number; title: string }[] = [
  { id: 'compile', n: 1, title: 'Compile' },
  { id: 'analyse', n: 2, title: 'Bytecode' },
  { id: 'transpile', n: 3, title: 'Transpile' },
  { id: 'image', n: 4, title: 'RV32IM image' },
  { id: 'run', n: 5, title: 'Run on zkVM' },
];

export interface JobState { job: JobView | null; console: ConsoleLine[]; error: string | null }
export const initial: JobState = { job: null, console: [], error: null };

export function reduce(s: JobState, e: JobStreamEvent): JobState {
  switch (e.type) {
    case 'job': return { ...s, job: e.job, console: e.job.console };
    case 'stage': return s.job ? { ...s, job: { ...s.job, [e.stage]: e.state } } : s;
    case 'console': return { ...s, console: [...s.console, e.line] };
    case 'done': return s.job ? { ...s, job: { ...s.job, finished: true } } : s;
    // The stream is gone for good: surface it and free Run (a live job is marked finished).
    case 'error': {
      if (!s.job) return { ...s, error: e.message };
      const j = s.job;
      const dead = (st: StageState): StageState => st.status === 'done' || st.status === 'failed' ? st : { ...st, status: 'failed', error: { kind: 'connection', message: e.message, diagnostics: [] } };
      return { ...s, error: e.message, job: { ...j, compile: dead(j.compile), transpile: dead(j.transpile), build: dead(j.build), finished: true } };
    }
  }
}

export function stageStatus(job: JobView | null, s: Stage): StageStatus | 'idle' {
  if (!job) return 'idle';
  switch (s) {
    case 'compile': case 'analyse': return job.compile.status;
    case 'transpile': return job.transpile.status;
    case 'image': return job.build.status;
    case 'run': return job.build.status === 'done' ? 'queued' : 'idle';
  }
}

export function filesFor(lang: Lang, files: Record<string, string>, binary?: string): FileIn[] {
  if (lang === 'solana-elf') return [{ name: 'program.so', encoding: 'base64', content: binary ?? '' }];
  // An empty file (the starter's blank Cargo.toml, a cleared tab) is left out: the server would
  // take it as the file's whole content.
  return Object.entries(files).filter(([, content]) => content.trim() !== '').map(([name, content]) => ({ name, encoding: 'text' as const, content }));
}

/** The contracts the developer can pick between: from a compile error that lists them ("choose a
 *  contract: the file defines A, B", "no contract named X; the file defines A, B"), or from a
 *  compile view that defines several. Empty when there is nothing to choose. */
export function contractChoices(error: StageError | null | undefined, view: { lang: string; contracts?: string[] } | null | undefined): string[] {
  const m = error?.kind === 'unsupported' ? /the file defines (.+)$/.exec(error.message) : null;
  if (m) return m[1]!.split(',').map((s) => s.trim()).filter(Boolean);
  const listed = view?.lang === 'evm' ? view.contracts ?? [] : [];
  return listed.length > 1 ? listed : [];
}

type Action = JobStreamEvent | { type: 'reset' };

function reducer(s: JobState, a: Action): JobState {
  if (a.type === 'reset') return initial;
  return reduce(s, a);
}

export interface JobDeps {
  createJob: (req: NewJob) => Promise<string>;
  subscribe: (id: string, onEvent: (e: JobStreamEvent) => void) => () => void;
}

/** The submit/subscribe lifecycle, free of React: only the newest submit may subscribe, and a
 *  superseded or disposed subscription's events are dropped. */
export function createJobController(dispatch: (a: Action) => void, deps: JobDeps) {
  let seq = 0;
  let unsub: (() => void) | null = null;
  const close = () => { unsub?.(); unsub = null; };
  return {
    async submit(req: NewJob) {
      const mine = ++seq;
      close();
      dispatch({ type: 'reset' });
      try {
        const id = await deps.createJob(req);
        if (mine !== seq) return;
        unsub = deps.subscribe(id, (e) => { if (mine === seq) dispatch(e); });
      } catch (e) {
        if (mine === seq) dispatch({ type: 'error', message: e instanceof Error ? e.message : String(e) });
      }
    },
    reset() { seq++; close(); dispatch({ type: 'reset' }); },
    dispose() { seq++; close(); },
  };
}

export function useJob() {
  const [state, dispatch] = useReducer(reducer, initial);
  const [creating, setCreating] = useState(false);
  const ctl = useRef<ReturnType<typeof createJobController> | null>(null);
  useEffect(() => {
    ctl.current = createJobController(dispatch, { createJob: api.createJob, subscribe: subscribeJob });
    return () => { ctl.current?.dispose(); ctl.current = null; };
  }, []);
  const submit = useCallback(async (req: NewJob) => {
    setCreating(true);
    try { await ctl.current?.submit(req); } finally { setCreating(false); }
  }, []);
  const reset = useCallback(() => { ctl.current?.reset(); setCreating(false); }, []);
  return { state, creating, submit, reset };
}
