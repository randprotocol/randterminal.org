'use client';
import { useState } from 'react';
import { groupDigits } from '@/lib/format';
import { fittingTier, tableBars, tierSentence } from '@/lib/tables';
import type { RunSummary } from '@/lib/types';

export function TablesPanel({ run }: { run: RunSummary }) {
  const [tier, setTier] = useState(() => fittingTier(run) ?? (run.tiers.length ? Math.max(...run.tiers.map((t) => t.tier)) : 20));
  const bars = tableBars(run, tier);
  return (
    <div className="flex h-full flex-col gap-4 overflow-auto p-4">
      <p className="text-sm text-text-soft">{tierSentence(run)}</p>
      <div className="flex flex-wrap items-center gap-1" role="radiogroup" aria-label="tier">
        <span className="mr-1 text-xs text-text-mute">tier</span>
        {run.tiers.map((t) => (
          <button key={t.tier} role="radio" aria-checked={tier === t.tier} disabled={!t.fits} onClick={() => setTier(t.tier)}
            title={t.fits ? `tier ${t.tier}` : `tier ${t.tier}: does not fit`}
            className={`mono tabular-nums h-8 rounded-sm px-3 text-xs ${tier === t.tier ? 'bg-surface-2 text-text-strong' : t.fits ? 'text-text-soft' : 'cursor-not-allowed text-text-mute line-through'}`}>
            {t.tier}{!t.fits && <span className="sr-only"> does not fit</span>}
          </button>
        ))}
      </div>
      {!run.tiers.some((t) => t.fits) && <p className="text-xs text-text-mute">No tier fits, so none can be selected; the bars show the largest tier.</p>}
      <ul className="flex flex-col gap-3">
        {bars.map((b) => (
          <li key={b.key}>
            <div className="flex items-baseline justify-between gap-3 text-xs">
              <span className="text-text-strong">{b.label}</span>
              <span className="mono tabular-nums text-text-soft">
                {b.over && <span className="mr-2 font-semibold text-negative">▲ over capacity</span>}
                {groupDigits(b.used)} / {groupDigits(b.capacity)} · {(b.fill * 100).toFixed(b.fill < 0.1 ? 1 : 0)}%
              </span>
            </div>
            <div className="mt-1 h-2 overflow-hidden rounded-full bg-surface-2" role="meter" aria-valuemin={0} aria-valuemax={b.capacity} aria-valuenow={b.used} aria-label={b.label}>
              <div className={`h-2 rounded-full ${b.over ? 'bg-negative' : 'bg-accent'}`}
                style={{ width: `${Math.min(100, b.fill * 100)}%`, ...(b.over ? { backgroundImage: 'repeating-linear-gradient(135deg, transparent 0 4px, rgb(0 0 0 / .35) 4px 6px)' } : {}) }} />
            </div>
            <p className="mt-1 text-xs text-text-mute">{b.explain}</p>
          </li>
        ))}
      </ul>
      {run.hot_pcs.length > 0 && (
        <details className="text-xs"><summary className="cursor-pointer text-text-soft">Hottest instructions</summary>
          <ol className="mono mt-2">{run.hot_pcs.slice(0, 10).map(([pc, n]) => <li key={pc}>0x{pc.toString(16)} <span className="text-text-mute">× {groupDigits(n)}</span></li>)}</ol></details>
      )}
    </div>
  );
}
