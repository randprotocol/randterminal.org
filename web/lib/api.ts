import type { CompileView, CyclePage, ImageView, JobStreamEvent, JobView, MemoryView, NewJob, RunRequest, RunSummary, SampleMeta, SampleView, StateView, SyscallView, TranspileView } from './types';

const BASE = '/api/v1';

export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) { super(message); this.name = 'ApiError'; }
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { ...init, headers: { 'content-type': 'application/json', ...(init?.headers ?? {}) } });
  const text = await res.text();
  let body: any = null;
  try { body = text ? JSON.parse(text) : null; } catch { /* non-JSON error page */ }
  if (!res.ok) throw new ApiError(res.status, body?.error ?? 'http_error', body?.message ?? res.statusText);
  return body as T;
}

const q = (o: Record<string, number>) => Object.entries(o).map(([k, v]) => `${k}=${v}`).join('&');

export const api = {
  samples: () => req<SampleMeta[]>('/samples'),
  sample: (id: string) => req<SampleView>(`/samples/${id}`),
  createJob: async (j: NewJob) => (await req<{ id: string }>('/jobs', { method: 'POST', body: JSON.stringify(j) })).id,
  job: (id: string) => req<JobView>(`/jobs/${id}`),
  compile: (id: string) => req<CompileView>(`/jobs/${id}/compile`),
  transpile: (id: string) => req<TranspileView>(`/jobs/${id}/transpile`),
  image: (id: string) => req<ImageView>(`/jobs/${id}/image`),
  fileUrl: (id: string, name: string) => `${BASE}/jobs/${id}/files/${name}`,
  createRun: (id: string, r: RunRequest) => req<RunSummary>(`/jobs/${id}/runs`, { method: 'POST', body: JSON.stringify(r) }),
  run: (rid: string) => req<RunSummary>(`/runs/${rid}`),
  cycles: (rid: string, from: number, count: number) => req<CyclePage>(`/runs/${rid}/cycles?${q({ from, count })}`),
  state: (rid: string, at: number) => req<StateView>(`/runs/${rid}/state?${q({ at })}`),
  memory: (rid: string, at: number, addr: number, words: number) => req<MemoryView>(`/runs/${rid}/memory?${q({ at, addr, words })}`),
  syscalls: (rid: string) => req<SyscallView[]>(`/runs/${rid}/syscalls`),
};

/** Server-sent events for a job (`job` snapshot, `stage`, `console`, `done`); returns the unsubscribe function. */
export function subscribeJob(id: string, onEvent: (e: JobStreamEvent) => void): () => void {
  const es = new EventSource(`${BASE}/jobs/${id}/events`);
  const data = (m: Event) => JSON.parse((m as MessageEvent).data);
  es.addEventListener('job', (m) => onEvent({ type: 'job', job: data(m) }));
  es.addEventListener('stage', (m) => { const d = data(m); onEvent({ type: 'stage', stage: d.stage, state: d.state }); });
  es.addEventListener('console', (m) => { const { type: _t, ...line } = data(m); onEvent({ type: 'console', line }); });
  es.addEventListener('done', () => { onEvent({ type: 'done' }); es.close(); });
  es.onerror = () => { /* EventSource reconnects by itself; the snapshot event resynchronises state. */ };
  return () => es.close();
}
