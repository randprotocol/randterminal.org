import { describe, expect, it } from 'vitest';
import { createSamplePicker } from '@/lib/samples';

function setup() {
  const pending: { id: string; ok: (s: string) => void; fail: (e: Error) => void }[] = [];
  const log: string[] = [];
  const p = createSamplePicker<string>(
    (id) => new Promise((ok, fail) => pending.push({ id, ok, fail })),
    { loading: (b) => log.push(`loading:${b}`), loaded: (s) => log.push(`loaded:${s}`), failed: (m) => log.push(`failed:${m}`) },
  );
  return { p, pending, log };
}

describe('sample picker', () => {
  it('is loading from the click until the sample has been applied', async () => {
    const { p, pending, log } = setup();
    const done = p.pick('a');
    expect(log).toEqual(['loading:true']);
    pending[0]!.ok('A'); await done;
    expect(log).toEqual(['loading:true', 'loaded:A', 'loading:false']);
  });
  it('applies only the newest pick when responses arrive out of order', async () => {
    const { p, pending, log } = setup();
    const a = p.pick('a'); const b = p.pick('b');
    pending[1]!.ok('B'); await b;
    pending[0]!.ok('A'); await a;
    expect(log.filter((l) => l.startsWith('loaded'))).toEqual(['loaded:B']);
    expect(log.at(-1)).toBe('loading:false');
  });
  it('stays loading until the newest pick lands, whatever an older one does', async () => {
    const { p, pending, log } = setup();
    const a = p.pick('a'); const b = p.pick('b');
    pending[0]!.fail(new Error('old')); await a;
    expect(log).not.toContain('loading:false');
    expect(log).not.toContain('failed:old');
    pending[1]!.ok('B'); await b;
    expect(log.at(-1)).toBe('loading:false');
  });
  it('a failed pick reports and frees Compile', async () => {
    const { p, pending, log } = setup();
    const done = p.pick('a');
    pending[0]!.fail(new Error('nope')); await done;
    expect(log).toEqual(['loading:true', 'failed:nope', 'loading:false']);
  });
  it('after dispose a late response is dropped', async () => {
    const { p, pending, log } = setup();
    const done = p.pick('a'); p.dispose();
    pending[0]!.ok('A'); await done;
    expect(log).toEqual(['loading:true']);
  });
});
