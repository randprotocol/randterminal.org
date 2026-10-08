'use client';
import type { AccountForm, SbpfCallForm as Form } from '@/lib/types';
const input = 'mono h-8 w-full rounded-sm border border-border bg-surface px-2 text-xs';
const blank: AccountForm = { key: '', owner: '', lamports: 0, data: '0x', is_signer: false, is_writable: true, executable: false };

export function SbpfCallForm({ value, onChange }: { value: Form | null; onChange: (f: Form) => void }) {
  const form = value ?? { lang: 'sbpf', program_id: '', accounts: [blank], data: '0x' };
  const setAcc = (i: number, a: AccountForm | null) => onChange({ ...form, accounts: a ? form.accounts.map((x, k) => (k === i ? a : x)) : form.accounts.filter((_, k) => k !== i) });
  return (
    <div className="flex flex-col gap-2 text-xs">
      <label>program id (base58) <input className={input} value={form.program_id} onChange={(e) => onChange({ ...form, program_id: e.target.value })} /></label>
      {form.accounts.map((a, i) => (
        <fieldset key={i} className="grid grid-cols-2 gap-1 rounded-sm border border-border-soft p-2"><legend className="px-1 text-text-mute">account {i}</legend>
          <input className={input} value={a.key} placeholder="key" onChange={(e) => setAcc(i, { ...a, key: e.target.value })} />
          <input className={input} value={a.owner} placeholder="owner" onChange={(e) => setAcc(i, { ...a, owner: e.target.value })} />
          <input className={input} type="number" value={a.lamports} placeholder="lamports" onChange={(e) => setAcc(i, { ...a, lamports: Number(e.target.value) })} />
          <input className={input} value={a.data} placeholder="data (hex)" onChange={(e) => setAcc(i, { ...a, data: e.target.value })} />
          <div className="col-span-2 flex gap-3">{(['is_signer', 'is_writable', 'executable'] as const).map((k) => <label key={k}><input type="checkbox" checked={!!a[k]} onChange={(e) => setAcc(i, { ...a, [k]: e.target.checked })} /> {k.replace('is_', '')}</label>)}
            <button className="ml-auto text-text-mute" onClick={() => setAcc(i, null)}>remove</button></div>
        </fieldset>
      ))}
      <button className="self-start text-accent" onClick={() => onChange({ ...form, accounts: [...form.accounts, blank] })}>+ account</button>
      <label>instruction data (hex) <input className={input} value={form.data} onChange={(e) => onChange({ ...form, data: e.target.value })} /></label>
    </div>
  );
}
