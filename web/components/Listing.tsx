'use client';
import { cLineAt } from './ImagePanel';
import { hexAddr } from '@/lib/format';
import type { CompileView, ImageView, StateView, TranspileView } from '@/lib/types';

function srcText(compile: CompileView | null, pc: number | null | undefined): string | null {
  if (compile == null || pc == null) return null;
  if (compile.lang === 'evm') { const o = compile.opcodes.find((x) => x.pc === pc); return o ? `0x${pc.toString(16).padStart(4, '0')}  ${o.mnemonic}${o.push_hex ? ` 0x${o.push_hex}` : ''}` : null; }
  for (const f of compile.functions) for (const b of f.blocks) { const i = b.insns.find((x) => x.pc === pc); if (i) return `${pc}  ${i.text}`; }
  return null;
}

export function Listing({ state, image, transpile, compile }: { state: StateView; image: ImageView; transpile: TranspileView | null; compile: CompileView | null }) {
  const idx = Math.floor((state.cycle.pc - image.base_pc) / 4);
  const rv = image.disasm.slice(Math.max(0, idx - 8), idx + 9);
  const cLines = transpile?.c_source.split('\n') ?? [];
  const line = state.c_line ?? cLineAt(image.source_map?.pc_to_c, state.cycle.pc);
  const src = srcText(compile, state.src_pc);
  const label = compile?.lang === 'sbpf' ? 'sBPF instruction' : 'EVM opcode';
  return (
    <div className="flex flex-col gap-3 p-3 text-xs">
      <section><h3 className="text-text-mute">{label}</h3><p className="mono text-sm text-text-strong">{src ?? <span className="text-text-mute">no source map for this build</span>}</p></section>
      <section><h3 className="text-text-mute">generated C{line ? ` · line ${line}` : ''}</h3>
        {line ? <pre className="mono">{cLines.slice(Math.max(0, line - 4), line + 3).map((l, k) => { const n = Math.max(0, line - 4) + k + 1; return <div key={n} className={n === line ? 'bg-hl' : 'text-text-soft'}><span className="inline-block w-10 text-right text-text-mute">{n}</span>  {l}</div>; })}</pre>
          : <p className="text-text-mute">no source map for this build</p>}</section>
      <section><h3 className="text-text-mute">RV32IM · cycle {state.clk}</h3>
        <pre className="mono">{rv.map((d) => <div key={d.pc} className={d.pc === state.cycle.pc ? 'bg-hl text-text-strong' : 'text-text-soft'}><span className="inline-block w-20 text-text-mute">{hexAddr(d.pc)}</span>{d.text}{d.ecall && <span className="text-accent"> ; {d.ecall}</span>}</div>)}</pre></section>
    </div>
  );
}
