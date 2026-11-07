'use client';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import Link from 'next/link';
import { Editor } from './Editor';
import { SamplesPanel } from './SamplesPanel';
import { PipelineStrip } from './PipelineStrip';
import { Console } from './Console';
import { ThemeSwitch } from './ThemeSwitch';
import { StagePanel } from './StagePanel';
import { Icon } from './ui';
import { newJob, useJob, type Stage } from '@/lib/job';
import { EVM_VERSIONS, isEvmVersion, jobOptions, loadFork, saveFork, type EvmVersion } from '@/lib/evm';
import { useStageView } from '@/lib/useStageView';
import type { CallForm, CompileView, Diagnostic, ImageView, JobOptions, Lang, RunSummary, SampleView, TranspileView } from '@/lib/types';

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

// The workspace follows the work: reading the C or stepping a run takes the width from the source.
// Each mode remembers its own split.
type Mode = 'reading' | 'wide';
const WIDTH_KEY: Record<Mode, string> = { reading: 'randterminal.stageWidth', wide: 'randterminal.stageWidthWide' };
const MIN_W = 460;
const MIN_EDITOR = 340;
const WIDE_EDITOR = 400;
const DEFAULT_W = 620;
const maxStage = () => Math.max(MIN_W, window.innerWidth - MIN_EDITOR);
const clampW = (w: number) => Math.max(MIN_W, Math.min(w, maxStage()));
const saveW = (m: Mode, w: number) => { try { localStorage.setItem(WIDTH_KEY[m], String(w)); } catch { /* ignore */ } };
const modeOf = (s: Stage): Mode => (s === 'run' || s === 'transpile' ? 'wide' : 'reading');

export function Shell() {
  const [lang, setLang] = useState<Lang>('solidity');
  const [files, setFiles] = useState<Record<string, string>>(STARTER.solidity);
  const [active, setActive] = useState('Contract.sol');
  // A bundled Solana program is run by its sample id; there is no upload.
  const [program, setProgram] = useState<{ id: string; title: string } | null>(null);
  const [options, setOptions] = useState<JobOptions>({ chain_id: 20, stage: 2 });
  // The fork lives apart from the options: it outlives samples and language switches, for the session.
  const [fork, setForkState] = useState<EvmVersion>('shanghai');
  useEffect(() => { setForkState(loadFork()); }, []);
  const setFork = (v: EvmVersion) => { setForkState(v); saveFork(v); };
  const [defaultCall, setDefaultCall] = useState<CallForm | null>(null);
  const [focused, setFocused] = useState<Stage>('compile');
  // Phones open on the samples; the editor tab is one tap away.
  const [view, setView] = useState<'editor' | 'stage'>('stage');
  const [menu, setMenu] = useState(false);
  const [widths, setWidths] = useState<Record<Mode, number>>({ reading: DEFAULT_W, wide: 960 });
  const [dragging, setDragging] = useState(false);
  const [sampleLoading, setSampleLoading] = useState(false);
  const [run_, setRun] = useState<RunSummary | null>(null);
  const [maxW, setMaxW] = useState(1200);
  const { state, creating, submit, reset } = useJob();
  const mode = modeOf(focused);
  const stageW = widths[mode];
  const stageWRef = useRef(stageW);
  stageWRef.current = stageW;
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const job = state.job;
  const compile = useStageView<CompileView>(job?.id, 'compile', job?.compile.status === 'done');
  const transpile = useStageView<TranspileView>(job?.id, 'transpile', job?.transpile.status === 'done');
  const image = useStageView<ImageView>(job?.id, 'image', job?.build.status === 'done');
  useEffect(() => { setRun(null); }, [job?.id]);
  const views = useMemo(() => ({ compile: compile.data, transpile: transpile.data, image: image.data, run: run_ }), [compile.data, transpile.data, image.data, run_]);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    const out = (e: PointerEvent) => { if (!menuRef.current?.contains(e.target as Node)) setMenu(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') setMenu(false); };
    window.addEventListener('pointerdown', out); window.addEventListener('keydown', esc);
    return () => { window.removeEventListener('pointerdown', out); window.removeEventListener('keydown', esc); };
  }, [menu]);

  useEffect(() => {
    const stored = (m: Mode, fallback: number) => { try { const w = Number(localStorage.getItem(WIDTH_KEY[m])); if (w >= MIN_W) return clampW(w); } catch { /* storage blocked */ } return clampW(fallback); };
    setWidths({ reading: stored('reading', DEFAULT_W), wide: stored('wide', window.innerWidth - WIDE_EDITOR) });
    const fit = () => { setMaxW(maxStage()); setWidths((w) => ({ reading: clampW(w.reading), wide: clampW(w.wide) })); };
    fit();
    window.addEventListener('resize', fit);
    return () => window.removeEventListener('resize', fit);
  }, []);

  const imageReady = !!state.job?.finished && state.job.build.status === 'done';
  useEffect(() => { if (imageReady) setFocused('image'); }, [imageReady]);

  // A sample on its way replaces the source and resets the job, so Compile waits for it.
  const busy = creating || sampleLoading || (!!job && !job.finished);
  const run = useCallback(() => {
    if (busy) return;
    setFocused('compile'); setView('stage');
    void submit(newJob(lang, files, jobOptions(lang, options, fork), program?.id));
  }, [busy, lang, files, program, options, fork, submit]);

  // Re-submits the same sources with the contract the developer picked (several contracts in the file).
  const pickContract = useCallback((contract: string) => {
    if (busy) return;
    const o = { ...options, contract };
    setOptions(o); setFocused('compile'); setView('stage');
    void submit(newJob(lang, files, jobOptions(lang, o, fork), program?.id));
  }, [busy, lang, files, program, options, fork, submit]);
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
    if (isEvmVersion(s.meta.options?.evm_version)) setFork(s.meta.options.evm_version);
    setDefaultCall(s.meta.default_call ?? null);
    if (s.meta.lang === 'solana-elf') { setFiles({}); setProgram({ id: s.meta.id, title: s.meta.title }); setActive(''); }
    else { const f = Object.fromEntries(s.files.map((x) => [x.name, x.content])); setFiles(f); setProgram(null); setActive(s.files[0]?.name ?? ''); }
    reset(); setMenu(false); setView('editor');
  };

  const drag = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setDragging(true);
    const m = modeRef.current;
    const move = (ev: PointerEvent) => setWidths((w) => ({ ...w, [m]: clampW(window.innerWidth - ev.clientX) }));
    const up = () => {
      window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up);
      setDragging(false); saveW(m, stageWRef.current);
    };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
  };
  const nudge = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const w = clampW(stageW + (e.key === 'ArrowLeft' ? 24 : -24));
    setWidths((x) => ({ ...x, [mode]: w })); saveW(mode, w);
  };
  const tabKeys = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const next = view === 'editor' ? 'stage' : 'editor';
    setView(next);
    document.getElementById(`tab-${next}`)?.focus();
  };

  const diagnostics: Diagnostic[] = useMemo(() => (state.job?.compile.error?.diagnostics ?? []).filter((d) => d.file === active || d.file.endsWith(`/${active}`)), [state.job, active]);
  const langSelect = (
    <select aria-label="language" value={lang} onChange={(e) => { const l = e.target.value as Lang; setLang(l); setFiles(STARTER[l]); setProgram(null); setActive(Object.keys(STARTER[l])[0] ?? ''); setOptions((o) => ({ chain_id: o.chain_id, stage: 2 })); setDefaultCall(null); reset(); }}
      className="h-7 rounded-sm border border-border bg-surface px-1.5 text-xs text-text max-lg:h-11">
      <option value="solidity">Solidity</option><option value="solana-rust">Solana (Rust)</option>
      {/* Only a bundled sample puts the shell in solana-elf; it cannot be picked. */}
      {lang === 'solana-elf' && <option value="solana-elf" hidden>Bundled program</option>}
    </select>
  );
  const forkSelect = lang === 'solidity' && (
    <select aria-label="EVM fork" title="The EVM fork solc compiles for" value={fork} onChange={(e) => { if (isEvmVersion(e.target.value)) setFork(e.target.value); }}
      className="mono h-7 rounded-sm border border-border bg-surface px-1.5 text-xs text-text max-lg:h-11">
      {EVM_VERSIONS.map((v) => <option key={v} value={v}>{v}</option>)}
    </select>
  );
  const tab = (id: 'editor' | 'stage', label: string) => (
    <button role="tab" id={`tab-${id}`} aria-controls={`panel-${id}`} tabIndex={view === id ? 0 : -1} aria-selected={view === id} onKeyDown={tabKeys} onClick={() => setView(id)} className={`h-11 flex-1 text-sm ${view === id ? 'border-b-2 border-accent text-text-strong' : 'text-text-soft'}`}>{label}</button>
  );

  return (
    <div className="grid h-dvh grid-cols-[minmax(0,1fr)] grid-rows-[48px_auto_auto_minmax(0,1fr)_auto] lg:grid-rows-[48px_auto_minmax(0,1fr)_auto]" style={{ ['--stage-w' as string]: `${Math.min(stageW, maxW)}px` }}>
      <header className="relative z-20 flex items-center gap-2 border-b border-border-soft px-4 max-sm:px-2">
        <span className="mono mr-2 font-semibold whitespace-nowrap text-text-strong">Rand Terminal</span>
        <div ref={menuRef} className="relative">
          <button aria-expanded={menu} onClick={() => setMenu(!menu)} className="inline-flex h-8 items-center gap-1 rounded-sm px-2 text-text-soft hover:bg-surface-2 hover:text-text-strong max-lg:h-11">Samples <Icon name="chevron" /></button>
          {menu && <div className="absolute top-full left-0 mt-1 w-[min(380px,calc(100vw-16px))] rounded-lg border border-border bg-surface p-1"><SamplesPanel onLoad={load} onLoading={setSampleLoading} /></div>}
        </div>
        <Link href="/zkvm" className="inline-flex h-8 items-center rounded-sm px-2 whitespace-nowrap text-text-soft hover:bg-surface-2 hover:text-text-strong max-lg:h-11 max-sm:hidden">How the zkVM works</Link>
        <button onClick={run} disabled={busy} title="Ctrl/Cmd + Enter" className="ml-auto h-8 rounded-sm bg-accent px-4 font-medium whitespace-nowrap text-on-accent hover:brightness-110 disabled:opacity-50 max-lg:h-11 max-sm:px-3">{busy ? 'Working…' : 'Compile & Run'}</button>
        <ThemeSwitch />
      </header>
      <PipelineStrip job={job} views={views} focused={focused} onFocus={(s) => { setFocused(s); setView('stage'); }} />
      <div role="tablist" className="flex border-b border-border-soft lg:hidden">{tab('editor', 'Editor')}{tab('stage', 'Stages')}</div>
      <div data-dragging={dragging || undefined} className="workspace relative grid min-h-0 lg:grid-cols-[minmax(0,1fr)_var(--stage-w)]">
        <main id="panel-editor" role="tabpanel" aria-labelledby="tab-editor" className={`grid min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)] overflow-hidden ${view === 'editor' ? '' : 'max-lg:hidden'}`}>
          <div className="min-w-0 border-b border-border-soft">
            <div className="flex min-w-0 items-center gap-2 pr-2">
              <div className="flex min-w-0 flex-1 overflow-x-auto" aria-label="files">
                {Object.keys(files).map((n) => <button key={n} aria-pressed={n === active} onClick={() => setActive(n)} className="file-tab max-lg:h-11">{n}</button>)}
              </div>
              {lang === 'solidity' && <label className="flex items-center gap-1 text-xs whitespace-nowrap text-text-mute">Chain <input aria-label="chain id" type="number" value={options.chain_id} onChange={(e) => setOptions({ ...options, chain_id: Number(e.target.value) })} className="mono h-7 w-14 rounded-sm border border-border bg-surface px-1.5 text-text max-lg:h-11" /></label>}
              {forkSelect}
              {langSelect}
            </div>
          </div>
          {lang === 'solana-elf'
            ? <div className="p-6"><p className="text-text-soft">{program ? `${program.title}: a program bundled with the terminal, run as deployed. Press Compile & Run.` : 'Pick a Solana sample.'}</p>
                <p className="mt-2 text-xs text-text-mute">Programs are not uploaded: write Solana Rust to build your own.</p></div>
            : <div className="min-h-0"><Editor key={`${lang}:${active}`} value={files[active] ?? ''} onChange={onEdit} language={active.endsWith('.sol') ? 'solidity' : active.endsWith('.rs') ? 'rust' : 'toml'} diagnostics={diagnostics} onRun={run} /></div>}
        </main>
        <section id="panel-stage" role="tabpanel" aria-labelledby="tab-stage" className={`relative min-h-0 min-w-0 overflow-auto border-l border-border-soft bg-bg max-lg:border-l-0 ${view === 'stage' ? '' : 'max-lg:hidden'}`} aria-label="stage panel">
          <div role="separator" aria-orientation="vertical" aria-label="resize stage panel" aria-valuenow={stageW} aria-valuemin={MIN_W} aria-valuemax={maxW} tabIndex={0} onPointerDown={drag} onKeyDown={nudge}
            className="absolute inset-y-0 left-0 z-10 w-1.5 cursor-col-resize hover:bg-accent focus-visible:bg-accent focus-visible:outline-2 focus-visible:outline-accent max-lg:hidden" />
          {state.error && <p className="m-3 rounded-sm border border-negative p-3 text-negative" role="alert">{state.error}</p>}
          {job
            ? <StagePanel stage={focused} job={job} compile={compile} transpile={transpile} image={image} defaultCall={defaultCall} onRun={setRun} onPickContract={busy ? undefined : pickContract} />
            : <div className="mx-auto max-w-lg p-6">
                <h1 className="text-2xl leading-tight font-medium text-text-strong">Follow a contract down to zkVM cycles.</h1>
                <p className="mt-2 mb-5 text-text-soft">Pick a sample below or write your own in the editor, then press Compile &amp; Run (⌘ or Ctrl + Enter). New to it? <Link href="/zkvm" className="text-accent-2 underline-offset-2 hover:underline">See how the zkVM works</Link>, step by step.</p>
                {!menu && <SamplesPanel onLoad={load} onLoading={setSampleLoading} />}
              </div>}
        </section>
      </div>
      <Console lines={state.console} />
    </div>
  );
}
