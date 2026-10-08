import { describe, expect, it, vi } from 'vitest';
import { createJobController } from '@/lib/job';
import type { JobStreamEvent } from '@/lib/types';

const req = { lang: 'solidity' as const, files: [] };
function setup() {
  const resolvers: ((id: string) => void)[] = [];
  const subs: { id: string; emit: (e: JobStreamEvent) => void; close: ReturnType<typeof vi.fn> }[] = [];
  const dispatched: any[] = [];
  const c = createJobController((a) => dispatched.push(a), {
    createJob: () => new Promise<string>((r) => resolvers.push(r)),
    subscribe: (id, emit) => { const close = vi.fn(); subs.push({ id, emit, close }); return close; },
  });
  return { c, resolvers, subs, dispatched };
}

describe('job controller', () => {
  it('a superseded submit never subscribes', async () => {
    const { c, resolvers, subs } = setup();
    const p1 = c.submit(req); const p2 = c.submit(req);
    resolvers[1]!('b'); await p2;
    resolvers[0]!('a'); await p1;
    expect(subs.map((s) => s.id)).toEqual(['b']);
  });
  it('a new submit closes the previous subscription and drops its late events', async () => {
    const { c, resolvers, subs, dispatched } = setup();
    const p1 = c.submit(req); resolvers[0]!('a'); await p1;
    const p2 = c.submit(req); resolvers[1]!('b'); await p2;
    expect(subs[0]!.close).toHaveBeenCalled();
    dispatched.length = 0;
    subs[0]!.emit({ type: 'done' });
    expect(dispatched).toEqual([]);
    subs[1]!.emit({ type: 'done' });
    expect(dispatched).toEqual([{ type: 'done' }]);
  });
  it('dispose during createJob does not subscribe; dispose closes live subscription', async () => {
    const a = setup();
    const p = a.c.submit(req); a.c.dispose(); a.resolvers[0]!('x'); await p;
    expect(a.subs).toEqual([]);
    const b = setup();
    const q = b.c.submit(req); b.resolvers[0]!('y'); await q;
    b.c.dispose();
    expect(b.subs[0]!.close).toHaveBeenCalled();
  });
  it('reports a createJob failure only for the current request', async () => {
    const dispatched: any[] = [];
    const c = createJobController((a) => dispatched.push(a), { createJob: () => Promise.reject(new Error('boom')), subscribe: () => () => {} });
    await c.submit(req);
    expect(dispatched.at(-1)).toEqual({ type: 'error', message: 'boom' });
  });
});
