'use client';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Editor } from './Editor';
import { SamplesPanel } from './SamplesPanel';
import { PipelineStrip } from './PipelineStrip';
import { Console } from './Console';
import { ThemeSwitch } from './ThemeSwitch';
import { StagePanel } from './StagePanel';
import { filesFor, useJob, type Stage } from '@/lib/job';
import type { CallForm, Diagnostic, JobOptions, Lang, SampleView } from '@/lib/types';

const STARTER: Record<Lang, Record<string, string>> = {
  solidity: { 'Contract.sol': '// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n\ncontract Hello {\n    uint256 public n;\n    function bump() external { n += 1; }\n}\n' },
  'solana-rust': { 'lib.rs': '// Pick the "Solana counter" sample to start from a working program.\n', 'Cargo.toml': '' },
  'solana-elf': {},
};

const WIDTH_KEY = 'randterminal.stageWidth';
const MIN_W = 480;
const DEFAULT_W = 560;

export function Shell() {
  const [lang, setLang] = useState<Lang>('solidity');
  const [files, setFiles] = useState<Record<string, string>>(STARTER.solidity);
  const [active, setActive] = useState('Contract.sol');
  const [binary, setBinary] = useState<string | undefined>();
  const [options, setOptions] = useState<JobOptions>({ chain_id: 20, stage: 2 });
  const [defaultCall, setDefaultCall] = useState<CallForm | null>(null);
  const [focused, setFocused] = useState<Stage>('compile');
  const [view, setView] = useState<'editor' | 'stage'>('editor');
  const [sheet, setSheet] = useState(false);
  const [stageW, setStageW] = useState(DEFAULT_W);
  const { state, submit } = useJob();

  useEffect(() => {
    try { const w = Number(localStorage.getItem(WIDTH_KEY)); if (w >= MIN_W) setStageW(w); } catch { /* storage blocked */ }
  }, []);

  const run = useCallback(() => {
    setFocused('compile'); setView('stage');
    void submit({ lang, files: filesFor(lang, files, binary), options });
  }, [lang, files, binary, options, submit]);

  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); runRef.current(); } };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, []);

  const load = (s: SampleView) => {
    setLang(s.meta.lang);
    setOptions(s.meta.options ?? { chain_id: 20, stage: 2 });
    setDefaultCall(s.meta.default_call ?? null);
    if (s.meta.lang === 'solana-elf') { setFiles({}); setBinary(s.files[0]?.content); setActive(''); }
    else { const f = Object.fromEntries(s.files.map((x) => [x.name, x.content])); setFiles(f); setBinary(undefined); setActive(s.files[0]?.name ?? ''); }
    setSheet(false); setView('editor');
  };

  const onUpload = async (f: File) => {
    const buf = new Uint8Array(await f.arrayBuffer());
    let s = ''; for (const b of buf) s += String.fromCharCode(b);
    setBinary(btoa(s)); setLang('solana-elf'); setFiles({}); setActive('');
  };

  const drag = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => setStageW(Math.max(MIN_W, Math.min(window.innerWidth - 240 - 240, window.innerWidth - ev.clientX)));
    const up = () => {
      window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up);
      setStageW((w) => { try { localStorage.setItem(WIDTH_KEY, String(w)); } catch { /* ignore */ } return w; });
    };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
  };
  const nudge = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const w = Math.max(MIN_W, stageW + (e.key === 'ArrowLeft' ? 24 : -24));
    setStageW(w); try { localStorage.setItem(WIDTH_KEY, String(w)); } catch { /* ignore */ }
  };

  const diagnostics: Diagnostic[] = useMemo(() => (state.job?.compile.error?.diagnostics ?? []).filter((d) => d.file === active || d.file.endsWith(`/${active}`)), [state.job, active]);
  const busy = !!state.job && !state.job.finished;
  const tab = (id: 'editor' | 'stage', label: string) => (
    <button role="tab" aria-selected={view === id} onClick={() => setView(id)} className={`h-11 flex-1 text-sm ${view === id ? 'border-b-2 border-accent text-text-strong' : 'text-text-soft'}`}>{label}</button>
  );

  return (
    <div className="grid h-screen grid-rows-[48px_auto_1fr_auto] lg:grid-rows-[48px_1fr_auto]" style={{ ['--stage-w' as string]: `${stageW}px` }}>
      <header className="flex items-center gap-3 border-b border-border-soft px-4">
        <button className="h-11 rounded-sm px-2 text-xs text-text-soft lg:hidden" aria-expanded={sheet} onClick={() => setSheet(!sheet)}>Menu</button>
        <span className="mono font-semibold text-text-strong">rand terminal</span>
        <select aria-label="language" value={lang} onChange={(e) => { const l = e.target.value as Lang; setLang(l); setFiles(STARTER[l]); setBinary(undefined); setActive(Object.keys(STARTER[l])[0] ?? ''); setDefaultCall(null); }}
          className="h-8 rounded-sm border border-border bg-surface px-2 text-text max-lg:h-11">
          <option value="solidity">Solidity</option><option value="solana-rust">Solana (Rust)</option><option value="solana-elf">Solana (.so upload)</option>
        </select>
        {lang === 'solidity' && <label className="text-xs text-text-mute max-sm:hidden">chain id <input type="number" value={options.chain_id} onChange={(e) => setOptions({ ...options, chain_id: Number(e.target.value) })} className="mono ml-1 h-8 w-20 rounded-sm border border-border bg-surface px-2 text-text" /></label>}
        <button onClick={run} disabled={busy} title="Ctrl/Cmd + Enter" className="ml-auto h-8 rounded-sm bg-accent px-4 font-medium text-on-accent disabled:opacity-50 max-lg:h-11">{busy ? 'Working…' : 'Compile & Run'}</button>
        <ThemeSwitch />
      </header>
      <div role="tablist" className="flex border-b border-border-soft lg:hidden">{tab('editor', 'Editor')}{tab('stage', 'Stages')}</div>
      <div className="relative grid min-h-0 lg:grid-cols-[240px_minmax(0,1fr)_var(--stage-w)]">
        <aside className={`${sheet ? 'max-lg:absolute max-lg:inset-x-0 max-lg:top-0 max-lg:z-10 max-lg:max-h-[70%] max-lg:border-b max-lg:bg-bg' : 'max-lg:hidden'} flex min-h-0 flex-col gap-4 overflow-auto border-r border-border-soft p-2`}>
          <SamplesPanel onLoad={load} />
          <section aria-label="files">
            <h2 className="px-2 pb-1 text-xs uppercase tracking-wide text-text-mute">Files</h2>
            {Object.keys(files).map((n) => <button key={n} onClick={() => { setActive(n); setSheet(false); }} className={`mono block min-h-8 w-full rounded-sm px-2 text-left text-xs max-lg:min-h-11 ${n === active ? 'bg-surface-2 text-text-strong' : 'text-text-soft hover:bg-surface'}`}>{n}</button>)}
            {lang === 'solana-elf' && <label className="block px-2 text-xs text-text-soft">Upload .so <input type="file" accept=".so" onChange={(e) => e.target.files?.[0] && onUpload(e.target.files[0])} className="mt-1 block w-full text-xs" /></label>}
          </section>
          <section aria-label="stages"><h2 className="px-2 pb-1 text-xs uppercase tracking-wide text-text-mute">Pipeline</h2><PipelineStrip job={state.job} focused={focused} onFocus={(s) => { setFocused(s); setView('stage'); setSheet(false); }} /></section>
        </aside>
        <main className={`min-h-0 border-r border-border-soft ${view === 'editor' ? '' : 'max-lg:hidden'}`}>
          {lang === 'solana-elf'
            ? <div className="p-6 text-text-soft">{binary ? `program.so loaded (${Math.round((binary.length * 3) / 4 / 1024)} KB). Press Compile & Run.` : 'Upload a Solana program (.so) built with cargo build-sbf, or pick the SPL Token sample.'}</div>
            : <Editor key={`${lang}:${active}`} value={files[active] ?? ''} onChange={(v) => setFiles({ ...files, [active]: v })} language={active.endsWith('.sol') ? 'solidity' : active.endsWith('.rs') ? 'rust' : 'toml'} diagnostics={diagnostics} onRun={run} />}
        </main>
        <section className={`relative min-h-0 overflow-auto ${view === 'stage' ? '' : 'max-lg:hidden'}`} aria-label="stage panel">
          <div role="separator" aria-orientation="vertical" aria-label="resize stage panel" tabIndex={0} onPointerDown={drag} onKeyDown={nudge}
            className="absolute inset-y-0 left-0 z-10 w-1.5 cursor-col-resize hover:bg-accent focus-visible:bg-accent max-lg:hidden" />
          {state.error && <p className="m-3 rounded-sm border border-negative p-3 text-negative" role="alert">{state.error}</p>}
          <StagePanel stage={focused} job={state.job} defaultCall={defaultCall} onFocus={setFocused} />
        </section>
      </div>
      <Console lines={state.console} />
    </div>
  );
}
