'use client';
import { useEffect, useState } from 'react';
import { CompilePanel } from './CompilePanel';
import { AnalysePanel } from './AnalysePanel';
import { TranspilePanel } from './TranspilePanel';
import { ImagePanel } from './ImagePanel';
import { RunPanel } from './RunPanel';
import { StageError } from './StageError';
import { useStageView } from '@/lib/useStageView';
import type { Stage } from '@/lib/job';
import type { CallForm, CompileView, ImageView, JobView, TranspileView } from '@/lib/types';

export function StagePanel({ stage, job, defaultCall }: { stage: Stage; job: JobView | null; defaultCall: CallForm | null; onFocus: (s: Stage) => void }) {
  const [selected, setSelected] = useState<number | null>(null);
  useEffect(() => { setSelected(null); }, [job?.id]);
  const compile = useStageView<CompileView>(job?.id, 'compile', job?.compile.status === 'done');
  const transpile = useStageView<TranspileView>(job?.id, 'transpile', job?.transpile.status === 'done');
  const image = useStageView<ImageView>(job?.id, 'image', job?.build.status === 'done');
  if (!job) return <p className="p-6 text-text-mute">Pick a sample or write a contract, then press Compile &amp; Run (⌘/Ctrl + Enter).</p>;
  // A stage shows its own failure, or the failure of an earlier stage that kept it from ever running.
  const upto = { compile: 1, analyse: 1, transpile: 2, image: 3, run: 3 }[stage];
  const failed = (['compile', 'transpile', 'build'] as const).slice(0, upto).find((s) => job[s].status === 'failed');
  if (failed && job[failed].error) return <StageError title={failed} error={job[failed].error!} />;
  const wait = (what: string, v: { error: string | null; retry: () => void }) => v.error
    ? <div role="alert" className="m-4 rounded-md border border-negative/60 bg-surface p-4"><p className="text-negative">Could not load {what}: {v.error}</p><button onClick={v.retry} className="mt-2 h-8 rounded-sm bg-surface-2 px-3 text-xs text-text-strong">Retry</button></div>
    : <p className="p-6 text-text-mute">Waiting for {what}…</p>;
  switch (stage) {
    case 'compile': return compile.data ? <CompilePanel view={compile.data} /> : wait('the compiler view', compile);
    case 'analyse': return compile.data ? <AnalysePanel view={compile.data} selected={selected} onSelect={setSelected} /> : wait('the compiler view', compile);
    case 'transpile': return transpile.data ? <TranspilePanel view={transpile.data} compile={compile.data} selected={selected} onSelect={setSelected} /> : wait('the transpiler view', transpile);
    case 'image': return image.data ? <ImagePanel jobId={job.id} view={image.data} /> : wait('the RV32IM image', image);
    case 'run': return image.data ? <RunPanel job={job} compile={compile.data} transpile={transpile.data} image={image.data} defaultCall={defaultCall} /> : wait('the RV32IM image', image);
  }
}
