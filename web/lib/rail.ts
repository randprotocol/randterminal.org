import { groupDigits } from './format';
import { fittingTier } from './tables';
import type { Stage } from './job';
import type { CompileView, ImageView, RunSummary, StageStatus, TranspileView } from './types';

/** What the program is at one stage: the rail's whole point. */
export interface RailFigure { value: string; unit: string; note?: string }
export interface RailInputs { compile: CompileView | null; transpile: TranspileView | null; image: ImageView | null; run: RunSummary | null }
export type RailStatus = StageStatus | 'idle' | 'ready';

export function railFigures({ compile, transpile, image, run }: RailInputs): Record<Stage, RailFigure | null> {
  const n = (v: number, unit: string): RailFigure => ({ value: groupDigits(v), unit });
  const tier = run && !run.trap ? fittingTier(run) : null;
  return {
    compile: !compile ? null : compile.lang === 'evm' ? { value: compile.contract, unit: 'contract' } : n(compile.elf_bytes, 'byte ELF'),
    analyse: !compile ? null : compile.lang === 'evm' ? n(compile.size_bytes, 'bytes') : n(compile.instructions, 'instructions'),
    transpile: transpile ? n(transpile.stats.c_lines, 'lines of C') : null,
    image: image ? n(image.words, 'words') : null,
    run: run ? { ...n(run.cycles, 'cycles'), note: run.trap ? 'trapped' : tier == null ? 'fits no tier' : `needs tier ${tier}` } : null,
  };
}

/** The job says nothing about runs: a built program is ready, a run decides how the last stage ended. */
export function railStatus(status: StageStatus | 'idle', stage: Stage, run: RunSummary | null): RailStatus {
  if (stage !== 'run' || status === 'idle') return status;
  if (run) return run.trap ? 'failed' : 'done';
  return 'ready';
}

const PLAN: Record<Stage, string> = { compile: 'Source to bytecode', analyse: 'Opcodes and gas', transpile: 'Bytecode to C', image: 'The committed program', run: 'Trace and proof tier' };

export function railCaption(status: RailStatus, stage: Stage): string {
  switch (status) {
    case 'idle': return PLAN[stage];
    case 'queued': return 'Waiting';
    case 'running': return 'Working…';
    case 'failed': return 'Failed';
    case 'ready': return 'Ready to run';
    case 'done': return 'Done';
  }
}
