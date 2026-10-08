'use client';
import { STAGES, stageStatus, type Stage } from '@/lib/job';
import type { JobView } from '@/lib/types';

const LABEL: Record<string, string> = { idle: 'not started', queued: 'waiting', running: 'running', done: 'done', failed: 'failed' };
const DOT: Record<string, string> = { idle: 'bg-border', queued: 'bg-text-mute', running: 'bg-accent animate-pulse', done: 'bg-positive', failed: 'bg-negative' };

export function PipelineStrip({ job, focused, onFocus }: { job: JobView | null; focused: Stage; onFocus: (s: Stage) => void }) {
  return (
    <ol className="flex flex-col gap-1" aria-label="pipeline">
      {STAGES.map((s) => {
        const st = stageStatus(job, s.id);
        return (
          <li key={s.id}>
            <button onClick={() => onFocus(s.id)} aria-current={focused === s.id ? 'step' : undefined}
              className={`flex w-full min-h-9 items-center gap-3 rounded-sm px-2 text-left max-lg:min-h-11 ${focused === s.id ? 'bg-surface-2 text-text-strong' : 'text-text-soft hover:bg-surface'}`}>
              <span className="mono w-4 text-text-mute">{s.n}</span>
              <span className="flex-1">{s.title}</span>
              <span className={`h-2 w-2 rounded-full ${DOT[st]}`} aria-hidden />
              <span className="sr-only">{LABEL[st]}</span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}
