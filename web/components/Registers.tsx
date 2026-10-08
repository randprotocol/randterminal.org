'use client';
import { REG_NAMES } from '@/lib/replay';
import { hex32 } from '@/lib/format';

export function Registers({ regs, changed, writes }: { regs: number[]; changed: Set<number>; writes: number | null }) {
  return (
    <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 p-3 mono text-xs" aria-label="registers">
      {regs.map((v, i) => (
        <div key={i} title={`x${i} = ${v >>> 0}`} className={`flex justify-between rounded-sm px-1 ${changed.has(i) ? 'bg-hl' : ''} ${writes === i ? 'outline outline-1 outline-accent' : ''}`}>
          <span className="text-text-mute">{REG_NAMES[i]}</span><span>{hex32(v)}</span>
        </div>
      ))}
    </div>
  );
}
