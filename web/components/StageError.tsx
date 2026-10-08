import type { StageError as E } from '@/lib/types';
export function StageError({ title, error }: { title: string; error: E }) {
  return (
    <div role="alert" className="m-4 rounded-md border border-negative/60 bg-surface p-4">
      <p className="font-medium text-negative">{title} failed: {error.kind}</p>
      <pre className="mono mt-2 whitespace-pre-wrap text-xs text-text-soft">{error.message}</pre>
      {error.diagnostics.length > 0 && <ul className="mt-2 text-xs">{error.diagnostics.map((d, i) => <li key={i} className="mono"><span className={d.severity === 'error' ? 'text-negative' : 'text-warning'}>{d.severity}</span> {d.file}:{d.line}:{d.col} {d.message}</li>)}</ul>}
    </div>
  );
}
