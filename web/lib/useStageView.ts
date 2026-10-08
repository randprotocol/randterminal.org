'use client';
import { useEffect, useState } from 'react';
import { api } from './api';

export function useStageView<T>(jobId: string | undefined, stage: 'compile' | 'transpile' | 'image', ready: boolean) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setData(null); setError(null);
    if (!jobId || !ready) return;
    let live = true;
    (api[stage](jobId) as Promise<T>).then((d) => live && setData(d)).catch((e) => live && setError(e.message));
    return () => { live = false; };
  }, [jobId, stage, ready]);
  return { data, error };
}
