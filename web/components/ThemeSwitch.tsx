'use client';
import { useEffect, useState } from 'react';
const KEY = 'randterminal.theme';
const NEXT: Record<string, string> = { system: 'dark', dark: 'light', light: 'system' };
export function ThemeSwitch() {
  const [t, setT] = useState('system');
  useEffect(() => { setT(document.documentElement.dataset.theme ?? 'system'); }, []);
  const flip = () => { const n = NEXT[t] ?? 'system'; setT(n); document.documentElement.dataset.theme = n; try { localStorage.setItem(KEY, n); } catch { /* private mode */ } };
  return <button onClick={flip} className="h-8 rounded-sm px-2 text-xs text-text-soft hover:bg-surface max-lg:h-11" aria-label={`theme: ${t}`}>{t}</button>;
}
