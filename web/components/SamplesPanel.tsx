'use client';
import { useEffect, useRef, useState } from 'react';
import { api } from '@/lib/api';
import { createSamplePicker } from '@/lib/samples';
import type { Lang, SampleMeta, SampleView } from '@/lib/types';

const KIND: Record<Lang, string> = { solidity: 'Solidity', 'solana-rust': 'Solana Rust', 'solana-elf': 'Solana .so' };

export function SamplesPanel({ onLoad, onLoading }: { onLoad: (s: SampleView) => void; onLoading: (loading: boolean) => void }) {
  const [list, setList] = useState<SampleMeta[] | null>(null);
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
      {err && <p role="alert" className="pb-2 text-negative">Could not load samples: {err}</p>}
      <ul className="samples">
        {(list ?? []).map((m) => (
          <li key={m.id}>
            <button onClick={() => void picker.current?.pick(m.id)}>
              <span className="sample-title">{m.title}</span>
              <span className="sample-kind">{KIND[m.lang]}</span>
              {m.description && <span className="sample-about">{m.description}</span>}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
