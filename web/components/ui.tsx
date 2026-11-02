'use client';
import { useEffect, useRef, useState, type ReactNode } from 'react';

/** Labelled values: what a stage produced, set as a row of facts rather than a dotted string. */
export function Facts({ items }: { items: [label: string, value: ReactNode][] }) {
  return (
    <dl className="facts">
      {items.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}
    </dl>
  );
}

export function CopyButton({ text, label = 'Copy' }: { text: string; label?: string }) {
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle');
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  const show = (s: 'copied' | 'failed') => { setState(s); if (timer.current) clearTimeout(timer.current); timer.current = setTimeout(() => setState('idle'), 1600); };
  const copy = async () => { try { await navigator.clipboard.writeText(text); show('copied'); } catch { show('failed'); } };
  return <button onClick={() => void copy()} className="quiet" aria-live="polite">{state === 'copied' ? 'Copied' : state === 'failed' ? 'Copy failed' : label}</button>;
}

/** A long identifier in full: wrapped, selectable, with what it means beside it. */
export function Identity({ name, value, meaning }: { name: string; value: string; meaning: string }) {
  return (
    <div className="identity">
      <div><span className="text-text-strong">{name}</span> <span className="text-text-mute">{meaning}</span></div>
      <div className="flex items-start gap-2"><code className="mono min-w-0 flex-1 break-all text-text-soft">{value}</code><CopyButton text={value} /></div>
    </div>
  );
}

const ICONS = {
  start: 'M4 3v10M13 3 6 8l7 5z', back: 'M11 3 5 8l6 5z', forward: 'M5 3l6 5-6 5z', end: 'M12 3v10M3 3l7 5-7 5z',
  chevron: 'M4 6l4 4 4-4', sun: 'M8 5a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5 13 13M13 3l-1.5 1.5M4.5 11.5 3 13',
  moon: 'M13 9.5A5.5 5.5 0 1 1 6.5 3 4.5 4.5 0 0 0 13 9.5z', auto: 'M8 2a6 6 0 1 0 0 12A6 6 0 0 0 8 2zM8 2v12',
} as const;
export function Icon({ name }: { name: keyof typeof ICONS }) {
  return <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden><path d={ICONS[name]} /></svg>;
}
