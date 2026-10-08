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

const TEMPLATE_CARGO = `[package]
name = "program"
version = "0.1.0"
edition = "2021"
rust-version = "1.89"
resolver = "3"

[lib]
crate-type = ["cdylib", "lib"]

[dependencies]
solana-program = "=2.3.0"

[features]
no-entrypoint = []
`;

const STARTER: Record<Lang, Record<string, string>> = {
  solidity: { 'Contract.sol': '// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n\ncontract Hello {\n    uint256 public n;\n    function bump() external { n += 1; }\n}\n' },
  // Cargo.toml is the sandbox template's own (sandbox/solana-template/Cargo.toml): only the crates
  // vendored there resolve, and an empty file would have replaced it.
  'solana-rust': { 'lib.rs': '// Pick the "Solana counter" sample to start from a working program.\n', 'Cargo.toml': TEMPLATE_CARGO },
  'solana-elf': {},
};

const WIDTH_KEY = 'randterminal.stageWidth';
const MIN_W = 480;
const DEFAULT_W = 560;
const RAIL_W = 240;
const MAX_UPLOAD = 2 * 1024 * 1024;
const maxStage = () => Math.max(MIN_W, window.innerWidth - RAIL_W - MIN_W);
const clampW = (w: number) => Math.max(MIN_W, Math.min(w, maxStage()));
const saveW = (w: number) => { try { localStorage.setItem(WIDTH_KEY, String(w)); } catch { /* ignore */ } };

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
  const [upErr, setUpErr] = useState<string | null>(null);
  const [maxW, setMaxW] = useState(1200);
  const stageWRef = useRef(DEFAULT_W);
  stageWRef.current = stageW;
  const { state, creating, submit, reset } = useJob();

  useEffect(() => {
    try { const w = Number(localStorage.getItem(WIDTH_KEY)); if (w >= MIN_W) setStageW(clampW(w)); } catch { /* storage blocked */ }
    const fit = () => { setMaxW(maxStage()); setStageW((w) => clampW(w)); };
    fit();
    window.addEventListener('resize', fit);
    return () => window.removeEventListener('resize', fit);
  }, []);

  const imageReady = !!state.job?.finished && state.job.build.status === 'done';
  useEffect(() => { if (imageReady) setFocused('image'); }, [imageReady]);

  const busy = creating || (!!state.job && !state.job.finished);
  const run = useCallback(() => {
    if (busy) return;
    setFocused('compile'); setView('stage');
    void submit({ lang, files: filesFor(lang, files, binary), options });
  }, [busy, lang, files, binary, options, submit]);

  // Re-submits the same sources with the contract the developer picked (several contracts in the file).
  const pickContract = useCallback((contract: string) => {
    if (busy) return;
    const o = { ...options, contract };
    setOptions(o); setFocused('compile'); setView('stage');
    void submit({ lang, files: filesFor(lang, files, binary), options: o });
  }, [busy, lang, files, binary, options, submit]);
  // An edit to the source may rename or remove the picked contract: forget it (keep chain id, stage).
  const onEdit = (v: string) => {
    if (v === files[active]) return;
    setFiles({ ...files, [active]: v });
    setOptions((o) => (o.contract != null ? { ...o, contract: null } : o));
  };

  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const h = (e: KeyboardEvent) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !e.defaultPrevented) { e.preventDefault(); runRef.current(); } };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, []);

  const load = (s: SampleView) => {
    setLang(s.meta.lang);
    setOptions(s.meta.options ?? { chain_id: 20, stage: 2 });
    setDefaultCall(s.meta.default_call ?? null);
    if (s.meta.lang === 'solana-elf') { setFiles({}); setBinary(s.files[0]?.content); setActive(''); }
    else { const f = Object.fromEntries(s.files.map((x) => [x.name, x.content])); setFiles(f); setBinary(undefined); setActive(s.files[0]?.name ?? ''); }
    reset(); setUpErr(null); setSheet(false); setView('editor');
  };

  const onUpload = async (input: HTMLInputElement) => {
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    if (f.size > MAX_UPLOAD) { setUpErr(`${f.name} is ${(f.size / 1048576).toFixed(1)} MB; the limit is 2 MB.`); return; }
    try {
      const buf = new Uint8Array(await f.arrayBuffer());
      let s = '';
      for (let i = 0; i < buf.length; i += 0x8000) s += String.fromCharCode(...buf.subarray(i, i + 0x8000));
      setBinary(btoa(s)); setLang('solana-elf'); setFiles({}); setActive(''); setUpErr(null); reset();
    } catch (e) { setUpErr(`Could not read ${f.name}: ${e instanceof Error ? e.message : String(e)}`); }
  };

  const drag = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => setStageW(clampW(window.innerWidth - ev.clientX));
    const up = () => {
      window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up);
      saveW(stageWRef.current);
    };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
  };
  const nudge = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const w = clampW(stageW + (e.key === 'ArrowLeft' ? 24 : -24));
    setStageW(w); saveW(w);
  };
  const tabKeys = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const next = view === 'editor' ? 'stage' : 'editor';
    setView(next);
    document.getElementById(`tab-${next}`)?.focus();
  };

  const diagnostics: Diagnostic[] = useMemo(() => (state.job?.compile.error?.diagnostics ?? []).filter((d) => d.file === active || d.file.endsWith(`/${active}`)), [state.job, active]);
  const langSelect = (
        <select aria-label="language" value={lang} onChange={(e) => { const l = e.target.value as Lang; setLang(l); setFiles(STARTER[l]); setBinary(undefined); setActive(Object.keys(STARTER[l])[0] ?? ''); setOptions((o) => ({ chain_id: o.chain_id, stage: 2 })); setDefaultCall(null); reset(); setUpErr(null); }}
          className="h-8 rounded-sm border border-border bg-surface px-2 text-text max-lg:h-11">
          <option value="solidity">Solidity</option><option value="solana-rust">Solana (Rust)</option><option value="solana-elf">Solana (.so upload)</option>
        </select>
  );
  const tab = (id: 'editor' | 'stage', label: string) => (
    <button role="tab" id={`tab-${id}`} aria-controls={`panel-${id}`} tabIndex={view === id ? 0 : -1} aria-selected={view === id} onKeyDown={tabKeys} onClick={() => setView(id)} className={`h-11 flex-1 text-sm ${view === id ? 'border-b-2 border-accent text-text-strong' : 'text-text-soft'}`}>{label}</button>
  );

  return (
    <div className="grid h-dvh grid-rows-[48px_auto_1fr_auto] lg:grid-rows-[48px_1fr_auto]" style={{ ['--stage-w' as string]: `${Math.min(stageW, maxW)}px` }}>
      <header className="flex items-center gap-3 border-b border-border-soft px-4 max-sm:gap-2 max-sm:px-2">
        <button className="h-11 rounded-sm px-2 text-xs text-text-soft lg:hidden" aria-expanded={sheet} onClick={() => setSheet(!sheet)}>Menu</button>
        <span className="mono whitespace-nowrap font-semibold text-text-strong">rand terminal</span>
        <span className="max-sm:hidden">{langSelect}</span>
        {lang === 'solidity' && <label className="text-xs text-text-mute max-sm:hidden">chain id <input type="number" value={options.chain_id} onChange={(e) => setOptions({ ...options, chain_id: Number(e.target.value) })} className="mono ml-1 h-8 w-20 rounded-sm border border-border bg-surface px-2 text-text" /></label>}
        <button onClick={run} disabled={busy} title="Ctrl/Cmd + Enter" className="ml-auto h-8 whitespace-nowrap rounded-sm bg-accent px-4 font-medium max-sm:px-3 text-on-accent disabled:opacity-50 max-lg:h-11">{busy ? 'Working…' : 'Compile & Run'}</button>
        <ThemeSwitch />
      </header>
      <div role="tablist" className="flex border-b border-border-soft lg:hidden">{tab('editor', 'Editor')}{tab('stage', 'Stages')}</div>
      <div className="relative grid min-h-0 lg:grid-cols-[240px_minmax(0,1fr)_var(--stage-w)]">
        <aside className={`${sheet ? 'max-lg:absolute max-lg:inset-x-0 max-lg:top-0 max-lg:z-10 max-lg:max-h-[70%] max-lg:border-b max-lg:bg-bg' : 'max-lg:hidden'} flex min-h-0 flex-col gap-4 overflow-auto border-r border-border-soft p-2`}>
          <div className="sm:hidden">{langSelect}</div>
          <SamplesPanel onLoad={load} />
          <section aria-label="files">
            <h2 className="px-2 pb-1 text-xs uppercase tracking-wide text-text-mute">Files</h2>
            {Object.keys(files).map((n) => <button key={n} onClick={() => { setActive(n); setSheet(false); }} className={`mono block min-h-8 w-full rounded-sm px-2 text-left text-xs max-lg:min-h-11 ${n === active ? 'bg-surface-2 text-text-strong' : 'text-text-soft hover:bg-surface'}`}>{n}</button>)}
            {lang === 'solana-elf' && <label className="block px-2 text-xs text-text-soft">Upload .so <input type="file" accept=".so" onChange={(e) => void onUpload(e.target)} className="mt-1 block w-full text-xs" /></label>}
            {upErr && <p className="px-2 text-xs text-negative" role="alert">{upErr}</p>}
          </section>
          <section aria-label="stages"><h2 className="px-2 pb-1 text-xs uppercase tracking-wide text-text-mute">Pipeline</h2><PipelineStrip job={state.job} focused={focused} onFocus={(s) => { setFocused(s); setView('stage'); setSheet(false); }} /></section>
        </aside>
        <main id="panel-editor" role="tabpanel" aria-labelledby="tab-editor" className={`min-h-0 border-r border-border-soft ${view === 'editor' ? '' : 'max-lg:hidden'}`}>
          {lang === 'solana-elf'
            ? <div className="p-6 text-text-soft">{binary ? `program.so loaded (${Math.round((binary.length * 3) / 4 / 1024)} KB). Press Compile & Run.` : 'Upload a Solana program (.so) built with cargo build-sbf, or pick the SPL Token sample.'}</div>
            : <Editor key={`${lang}:${active}`} value={files[active] ?? ''} onChange={onEdit} language={active.endsWith('.sol') ? 'solidity' : active.endsWith('.rs') ? 'rust' : 'toml'} diagnostics={diagnostics} onRun={run} />}
        </main>
        <section id="panel-stage" role="tabpanel" aria-labelledby="tab-stage" className={`relative min-h-0 overflow-auto ${view === 'stage' ? '' : 'max-lg:hidden'}`} aria-label="stage panel">
          <div role="separator" aria-orientation="vertical" aria-label="resize stage panel" aria-valuenow={stageW} aria-valuemin={MIN_W} aria-valuemax={maxW} tabIndex={0} onPointerDown={drag} onKeyDown={nudge}
            className="absolute inset-y-0 left-0 z-10 w-1.5 cursor-col-resize hover:bg-accent focus-visible:bg-accent focus-visible:outline-2 focus-visible:outline-accent max-lg:hidden" />
          {state.error && <p className="m-3 rounded-sm border border-negative p-3 text-negative" role="alert">{state.error}</p>}
          <StagePanel stage={focused} job={state.job} defaultCall={defaultCall} onFocus={setFocused} onPickContract={busy ? undefined : pickContract} />
        </section>
      </div>
      <Console lines={state.console} />
    </div>
  );
}
