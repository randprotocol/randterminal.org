'use client';
import { useEffect, useRef, useState } from 'react';
import { api } from '@/lib/api';
import { createSamplePicker } from '@/lib/samples';
import type { SampleMeta, SampleView } from '@/lib/types';

export function SamplesPanel({ onLoad, onLoading }: { onLoad: (s: SampleView) => void; onLoading: (loading: boolean) => void }) {
  const [list, setList] = useState<SampleMeta[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const handlers = useRef({ onLoad, onLoading });
  handlers.current = { onLoad, onLoading };
  const picker = useRef<ReturnType<typeof createSamplePicker<SampleView>> | null>(null);
  useEffect(() => { api.samples().then(setList).catch((e) => setErr(String(e.message ?? e))); }, []);
  useEffect(() => {
    picker.current = createSamplePicker(api.sample, {
      loading: (b) => { if (b) setErr(null); handlers.current.onLoading(b); },
      loaded: (s) => handlers.current.onLoad(s),
      failed: setErr,
    });
    return () => { picker.current?.dispose(); picker.current = null; handlers.current.onLoading(false); };
  }, []);
  return (
    <section aria-label="samples">
      <h2 className="px-2 pb-1 text-xs uppercase tracking-wide text-text-mute">Samples</h2>
      {err && <p className="px-2 text-xs text-negative">{err}</p>}
      <ul className="flex flex-col gap-1">
        {list.map((m) => (
          <li key={m.id}>
            <button title={m.description} onClick={() => void picker.current?.pick(m.id)} className="min-h-9 w-full rounded-sm px-2 text-left text-text-soft hover:bg-surface max-lg:min-h-11">
              {m.title} <span className="text-xs text-text-mute">{m.lang === 'solidity' ? 'Solidity' : m.lang === 'solana-rust' ? 'Solana Rust' : 'Solana .so'}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
