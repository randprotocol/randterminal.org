'use client';
import { useCallback, useEffect, useReducer, useRef } from 'react';
import { api, subscribeJob } from './api';
import type { ConsoleLine, FileIn, JobStreamEvent, JobView, Lang, NewJob, StageStatus } from './types';

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
  return Object.entries(files).map(([name, content]) => ({ name, encoding: 'text' as const, content }));
}

type Action = JobStreamEvent | { type: 'reset' } | { type: 'error'; message: string };

function reducer(s: JobState, a: Action): JobState {
  if (a.type === 'reset') return initial;
  if (a.type === 'error') return { ...s, error: a.message };
  return reduce(s, a);
}

export function useJob() {
  const [state, dispatch] = useReducer(reducer, initial);
  const unsub = useRef<(() => void) | null>(null);
  useEffect(() => () => unsub.current?.(), []);
  const submit = useCallback(async (req: NewJob) => {
    unsub.current?.();
    dispatch({ type: 'reset' });
    try {
      const id = await api.createJob(req);
      unsub.current = subscribeJob(id, dispatch);
    } catch (e) {
      dispatch({ type: 'error', message: e instanceof Error ? e.message : String(e) });
    }
  }, []);
  const reset = useCallback(() => { unsub.current?.(); dispatch({ type: 'reset' }); }, []);
  return { state, submit, reset };
}
