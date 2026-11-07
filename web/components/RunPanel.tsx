'use client';
import { useState } from 'react';
import { api } from '@/lib/api';
import { groupDigits } from '@/lib/format';
import { fittingTier } from '@/lib/tables';
import { abiSignature, ALICE, evmFormFromAbi, gasError } from '@/lib/replay';
import { EvmCallForm } from './EvmCallForm';
import { SbpfCallForm } from './SbpfCallForm';
import { Debugger } from './Debugger';
import { Facts } from './ui';
import { TablesPanel } from './TablesPanel';
import type { CallForm, CompileView, EvmCallForm as EF, ImageView, JobView, RunSummary, SbpfCallForm as SF, TranspileView } from '@/lib/types';

const STATUS: Record<number, string> = { 1: 'success', 0: 'reverted', 2: 'error' };

export function RunPanel({ job, compile, transpile, image, defaultCall, onRun }: { job: JobView; compile: CompileView | null; transpile: TranspileView | null; image: ImageView; defaultCall: CallForm | null; onRun: (r: RunSummary) => void }) {
  const [picked, setForm] = useState<CallForm | null>(defaultCall);
  const [run, setRun] = useState<RunSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [tab, setTab] = useState<'debugger' | 'tables'>('debugger');
  const evm = job.lang === 'solidity';
  // With no sample call, start from the ABI's first function (or one blank writable account), so a zero-argument call can run at once.
  const abi = compile?.lang === 'evm' ? compile.abi : [];
  const first = abi.find((a) => a.type === 'function');
  const form: CallForm = picked ?? (evm
    ? evmFormFromAbi(abi, first ? abiSignature(first) : 'f()', ALICE)
    : { lang: 'sbpf', program_id: '', accounts: [{ key: '', owner: '', lamports: 0, data: '0x', is_signer: false, is_writable: true, executable: false }], data: '0x' });
  const invalid = form.lang === 'evm' && gasError(form.gas_limit ?? 1_000_000) !== null;
  const go = async () => {
    if (invalid) return;
    setBusy(true); setErr(null);
    try {
      const r = await api.createRun(job.id, { call: form });
      // A new run starts at cycle 0: the #clk= hash belongs to the previous run only.
      history.replaceState(null, '', location.pathname + location.search);
      setRun(r); onRun(r);
    } catch (e) { setErr(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); }
  };
  return (
    <div className="flex h-full min-w-0 flex-col">
      <details open={!run} className="border-b border-border-soft px-4 py-3">
        <summary className="cursor-pointer text-text-soft">{run ? 'Change the call' : 'Call'}</summary>
        <div className="mt-3 max-w-2xl">
          {evm ? <EvmCallForm abi={abi} value={form as EF} onChange={setForm} /> : <SbpfCallForm value={form as SF} onChange={setForm} />}
          <button onClick={go} disabled={busy || invalid} className="mt-3 h-8 rounded-sm bg-accent px-4 font-medium text-on-accent hover:brightness-110 disabled:opacity-50">{busy ? 'Running…' : 'Run on the zkVM'}</button>
          {err && <p role="alert" className="mt-2 text-xs text-negative">{err}</p>}
        </div>
      </details>
      {run && (
        <>
          <div className="flex flex-col gap-2 border-b border-border-soft px-4 py-3">
            <Facts items={[
              ['Result', run.trap ? <span key="r" className="text-negative">trap: {run.trap}</span> : <span key="r" className={run.status === 1 ? 'text-positive' : 'text-warning'}>{STATUS[run.status] ?? `status ${run.status}`}</span>],
              ['Cycles', groupDigits(run.cycles)],
              ['Proof tier', fittingTier(run) ?? 'none fits'],
              ['Input words', `${groupDigits(run.input_words)}${run.public_words ? ` + ${groupDigits(run.public_words)} public` : ''}`],
            ]} />
            <p className="min-w-0 text-xs text-text-mute" title="the eight public output words">Public outputs <span className="mono text-text-soft">{run.outputs.join(' ')}</span></p>
            {(run.discovery ?? []).map((d) => <p key={d.run} className="text-xs text-text-mute">Found storage slot <span className="mono">{d.slot.slice(0, 10)}…</span> on dry run {d.run + 1}.</p>)}
          </div>
          {!run.trap && (
            <>
              <div className="flex gap-1 px-4 pt-2" role="tablist" aria-label="run view">{(['debugger', 'tables'] as const).map((t) => <button key={t} role="tab" aria-selected={tab === t} onClick={() => setTab(t)} className="seg">{t === 'debugger' ? 'Step through' : 'STARK tables'}</button>)}</div>
              <div className="min-h-0 flex-1">{tab === 'debugger' ? <Debugger key={run.rid} run={run} image={image} transpile={transpile} compile={compile} /> : <TablesPanel run={run} />}</div>
            </>
          )}
        </>
      )}
    </div>
  );
}
