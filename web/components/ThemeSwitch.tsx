'use client';
import { useEffect, useState } from 'react';
import { Icon } from './ui';
const KEY = 'randterminal.theme';
const NEXT: Record<string, string> = { system: 'dark', dark: 'light', light: 'system' };
const ICON = { system: 'auto', dark: 'moon', light: 'sun' } as const;
export function ThemeSwitch() {
  const [t, setT] = useState('system');
  useEffect(() => { setT(document.documentElement.dataset.theme ?? 'system'); }, []);
  const flip = () => { const n = NEXT[t] ?? 'system'; setT(n); document.documentElement.dataset.theme = n; try { localStorage.setItem(KEY, n); } catch { /* private mode */ } };
  return <button onClick={flip} title={`Theme: ${t}`} className="icon-btn" aria-label={`theme: ${t}`}><Icon name={ICON[t as keyof typeof ICON] ?? 'auto'} /></button>;
}
