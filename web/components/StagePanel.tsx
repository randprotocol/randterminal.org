'use client';
import type { Stage } from '@/lib/job';
import type { CallForm, JobView } from '@/lib/types';

export function StagePanel({ stage, job }: { stage: Stage; job: JobView | null; defaultCall: CallForm | null; onFocus: (s: Stage) => void }) {
  if (!job) return <p className="p-6 text-text-mute">Pick a sample or write a contract, then press Compile & Run (⌘/Ctrl + Enter).</p>;
  const failed = (['compile', 'transpile', 'build'] as const).map((k) => job[k].error).find(Boolean);
  return (
    <div className="p-6 text-text-mute">
      <p>{stage}</p>
      {failed && <p className="mt-3 text-negative" role="alert">{failed.kind}: {failed.message}</p>}
    </div>
  );
}
