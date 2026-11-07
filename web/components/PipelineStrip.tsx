'use client';
import { useEffect, useRef } from 'react';
import { STAGES, stageStatus, type Stage } from '@/lib/job';
import { railCaption, railFigures, railStatus, type RailInputs } from '@/lib/rail';
import type { JobView } from '@/lib/types';

/** The product in one band: the same program, stage by stage, with what it became at each.
 *  The thread between the stages fills as they finish. */
export function PipelineStrip({ job, views, focused, onFocus }: { job: JobView | null; views: RailInputs; focused: Stage; onFocus: (s: Stage) => void }) {
  const figures = railFigures(views);
  const current = useRef<HTMLButtonElement>(null);
  // On a narrow screen the rail scrolls: keep the stage in view as the job moves it along.
  useEffect(() => { current.current?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' }); }, [focused]);
  return (
    <ol className="rail" aria-label="pipeline">
      {STAGES.map((s) => {
        const status = railStatus(stageStatus(job, s.id), s.id, views.run);
        const f = figures[s.id];
        return (
          <li key={s.id} data-status={status}>
            <button ref={focused === s.id ? current : undefined} onClick={() => onFocus(s.id)} aria-current={focused === s.id ? 'step' : undefined} className="rail-stage">
              <span className="rail-title"><span className="rail-n">{String(s.n).padStart(2, '0')}<span aria-hidden> —</span></span>{s.title}</span>
              {f && status !== 'failed'
                ? <span className="rail-figure"><span className="rail-value">{f.value}</span> <span className="rail-unit">{f.unit}{f.note ? `, ${f.note}` : ''}</span></span>
                : <span className="rail-caption">{status === 'failed' && f?.note ? `Failed: ${f.note}` : railCaption(status, s.id)}</span>}
            </button>
          </li>
        );
      })}
    </ol>
  );
}
