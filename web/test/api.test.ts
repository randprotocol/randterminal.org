import { afterEach, describe, expect, it, vi } from 'vitest';
import { api, ApiError } from '@/lib/api';

afterEach(() => vi.unstubAllGlobals());

function stubFetch(status: number, body: unknown) {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })));
}

describe('api', () => {
  it('returns parsed JSON', async () => {
    stubFetch(201, { id: 'ab'.repeat(16) });
    await expect(api.createJob({ lang: 'solidity', files: [], options: { chain_id: 20, stage: 2 } })).resolves.toBe('ab'.repeat(16));
    const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(call[0]).toBe('/api/v1/jobs');
    expect((call[1] as RequestInit).method).toBe('POST');
  });
  it('turns error bodies into ApiError', async () => {
    stubFetch(409, { error: 'conflict', message: 'the image is not built yet' });
    const e = await api.image('x').catch((e) => e);
    expect(e).toBeInstanceOf(ApiError);
    expect(e.status).toBe(409);
    expect(e.code).toBe('conflict');
    expect(e.message).toBe('the image is not built yet');
  });
  it('builds query strings', async () => {
    stubFetch(200, { from: 0, cycles: [], total: 0 });
    await api.cycles('r', 4096, 512);
    expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls[0]![0]).toBe('/api/v1/runs/r/cycles?from=4096&count=512');
  });
});
