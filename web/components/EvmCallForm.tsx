'use client';
import { abiSignature, ALICE, evmFormFromAbi, gasError } from '@/lib/replay';
import type { AbiItem, EvmCallForm as Form, StorageSeed } from '@/lib/types';

const input = 'mono h-8 w-full rounded-sm border border-border bg-surface px-2 text-xs';

export function EvmCallForm({ abi, value, onChange }: { abi: AbiItem[]; value: Form | null; onChange: (f: Form) => void }) {
  const fns = abi.filter((a) => a.type === 'function');
  const form = value ?? evmFormFromAbi(abi, fns[0] ? abiSignature(fns[0]) : 'f()', ALICE);
  const item = fns.find((a) => abiSignature(a) === form.function);
  const storage = form.storage ?? [];
  const set = (p: Partial<Form>) => onChange({ ...form, ...p });
  const setSeed = (i: number, s: StorageSeed | null) => set({ storage: s ? storage.map((x, k) => (k === i ? s : x)) : storage.filter((_, k) => k !== i) });
  return (
    <div className="flex flex-col gap-2 text-xs">
      <label>function <select className={input} value={form.function} onChange={(e) => set({ function: e.target.value, args: evmFormFromAbi(abi, e.target.value, form.caller).args })}>{fns.map((a) => <option key={abiSignature(a)}>{abiSignature(a)}</option>)}</select></label>
      {(item?.inputs ?? []).map((p, i) => <label key={i}>{p.name || `arg${i}`}: <span className="text-text-mute">{p.type}</span><input className={input} value={form.args[i] ?? ''} placeholder="0x… or decimal" onChange={(e) => set({ args: form.args.map((a, k) => (k === i ? e.target.value : a)) })} /></label>)}
      <label>caller <input className={input} value={form.caller} onChange={(e) => set({ caller: e.target.value })} /></label>
      <div className="flex gap-2"><label className="flex-1">value (wei) <input className={input} value={form.value ?? '0'} onChange={(e) => set({ value: e.target.value })} /></label><label className="flex-1">gas limit <input className={input} type="number" value={Number.isNaN(form.gas_limit) ? '' : (form.gas_limit ?? 1_000_000)} aria-invalid={!!gasError(form.gas_limit ?? 1_000_000)} onChange={(e) => set({ gas_limit: e.target.value === '' ? NaN : Number(e.target.value) })} /></label></div>
      {gasError(form.gas_limit ?? 1_000_000) && <p role="alert" className="text-negative">{gasError(form.gas_limit ?? 1_000_000)}</p>}
      <fieldset className="rounded-sm border border-border-soft p-2"><legend className="px-1 text-text-mute">initial storage</legend>
        {storage.map((s, i) => (
          <div key={i} className="mb-1 flex gap-1">
            <select className={`${input} w-28`} value={s.kind} onChange={(e) => setSeed(i, e.target.value === 'slot' ? { kind: 'slot', slot: '0', value: '0' } : e.target.value === 'mapping' ? { kind: 'mapping', base: 0, key: '', value: '0' } : { kind: 'mapping2', base: 0, key1: '', key2: '', value: '0' })}><option value="slot">slot</option><option value="mapping">mapping</option><option value="mapping2">mapping²</option></select>
            {s.kind === 'slot' && <input className={input} value={s.slot} placeholder="slot" onChange={(e) => setSeed(i, { ...s, slot: e.target.value })} />}
            {s.kind !== 'slot' && <input className={`${input} w-14`} type="number" value={s.base} title="base slot" onChange={(e) => setSeed(i, { ...s, base: Number(e.target.value) })} />}
            {s.kind === 'mapping' && <input className={input} value={s.key} placeholder="key" onChange={(e) => setSeed(i, { ...s, key: e.target.value })} />}
            {s.kind === 'mapping2' && <><input className={input} value={s.key1} placeholder="key 1" onChange={(e) => setSeed(i, { ...s, key1: e.target.value })} /><input className={input} value={s.key2} placeholder="key 2" onChange={(e) => setSeed(i, { ...s, key2: e.target.value })} /></>}
            <input className={`${input} w-28`} value={s.value} placeholder="value" onChange={(e) => setSeed(i, { ...s, value: e.target.value })} />
            <button aria-label="remove" className="px-2 text-text-mute" onClick={() => setSeed(i, null)}>×</button>
          </div>
        ))}
        <button className="text-accent" onClick={() => set({ storage: [...storage, { kind: 'slot', slot: '0', value: '0' }] })}>+ add</button>
        <p className="mt-1 text-text-mute">Storage you don’t set reads as zero. The terminal finds the slots the call touches by dry-running the EVM interpreter, so you never write Merkle witnesses.</p>
      </fieldset>
    </div>
  );
}
