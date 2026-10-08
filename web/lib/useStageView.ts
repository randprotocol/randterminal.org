'use client';
import { useCallback, useEffect, useState } from 'react';
import { api } from './api';

export function useStageView<T>(jobId: string | undefined, stage: 'compile' | 'transpile' | 'image', ready: boolean) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const retry = useCallback(() => setNonce((n) => n + 1), []);
  useEffect(() => {
    setData(null); setError(null);
    if (!jobId || !ready) return;
    let live = true;
    (api[stage](jobId) as Promise<T>).then((d) => live && setData(d)).catch((e) => live && setError(e.message));
    return () => { live = false; };
  }, [jobId, stage, ready, nonce]);
  return { data, error, retry };
}
