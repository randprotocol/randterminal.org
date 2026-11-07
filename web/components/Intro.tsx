'use client';
import Link from 'next/link';
import { useRef, type ReactNode } from 'react';
import type { Lang } from '@/lib/types';

const KIND: Record<Lang, string> = { solidity: 'Solidity', 'solana-rust': 'Solana Rust', 'solana-elf': 'Bundled Solana program' };

// Every line here restates what the workspace already does (README, the rail's stage plans);
// nothing is claimed that the pipeline does not show.
const PILLARS = [
  { n: '01', label: 'Write', title: 'Source', body: 'Solidity for any EVM fork through Osaka, or a Solana program in Rust. Start from a sample or the editor.' },
  { n: '02', label: 'Compile', title: 'Bytecode to RV32IM', body: 'solc or cargo build-sbf in a sandbox, then evm2rv or sbpf2rv translate the bytecode to C and the committed RV32IM image.' },
  { n: '03', label: 'Prove', title: 'Trace and proof tier', body: 'Run the image on the Rand zkVM, step through every cycle, and read the STARK table heights against each proof tier.' },
] as const;

/** The empty workspace: what the terminal is, the state it is in, and the way in. The IDE stays
 *  beside it (desktop) or one tab away (phone); the samples list is the same one as before. */
export function Intro({ lang, source, chainId, busy, onNew, samples }: {
  lang: Lang; source: string; chainId: number; busy: boolean; onNew: () => void; samples: ReactNode;
}) {
  const list = useRef<HTMLDivElement>(null);
  return (
    <div className="intro">
      <section className="px-6 pt-14 pb-12 max-sm:px-4 max-sm:pt-10">
        <p className="eyebrow eyebrow-sq">Rand Terminal · RV32IM · Rand zkVM</p>
        <h1 className="intro-h1 mt-5 max-w-[16ch]">Follow a contract down to zkVM cycles.</h1>
        <p className="intro-lede mt-5 max-w-[56ch]">
          Paste Solidity or a Solana program in Rust, watch it become an RV32IM image through evm2rv or sbpf2rv, and step through its run on the Rand zkVM.
        </p>
        <div className="mt-8 flex flex-wrap gap-3">
          <button onClick={onNew} className="btn btn-primary max-lg:h-11">New program</button>
          <button onClick={() => list.current?.scrollIntoView({ block: 'start' })} className="btn btn-secondary max-lg:h-11">Browse samples</button>
          <Link href="/zkvm" className="btn btn-secondary max-lg:h-11">How the zkVM works</Link>
        </div>
        <dl className="panel mt-10" aria-label="workspace">
          <div className="panel-row"><dt>Source</dt><dd>{source || 'bundled program'}</dd><dd>{KIND[lang]}</dd></div>
          <div className="panel-row"><dt>Target</dt><dd>RV32IM</dd><dd>Rand zkVM</dd></div>
          {lang === 'solidity' && <div className="panel-row"><dt>Chain id</dt><dd>{chainId}</dd><dd>EVM</dd></div>}
          <div className="panel-row"><dt>Pipeline</dt><dd>5 stages</dd><dd>{busy ? 'working' : 'idle'}</dd></div>
        </dl>
      </section>

      <section className="border-t border-rule px-6 py-12 max-sm:px-4" aria-labelledby="pillars-h">
        <p id="pillars-h" className="eyebrow">Specification</p>
        <ol className="pillars mt-5">
          {PILLARS.map((p) => (
            <li key={p.n}>
              <p className="eyebrow">{p.n} — {p.label}</p>
              <h2 className="pillar-h">{p.title}</h2>
              <p className="pillar-p">{p.body}</p>
            </li>
          ))}
        </ol>
      </section>

      <section ref={list} className="scroll-mt-0 border-t border-rule px-6 py-12 max-sm:px-4" aria-labelledby="samples-h">
        <p id="samples-h" className="eyebrow">Samples</p>
        <p className="mt-2 mb-4 text-text-mute">Load one into the editor, then press Compile &amp; Run.</p>
        <div className="border-y border-rule">{samples}</div>
      </section>

      <section className="band-invert px-6 py-12 max-sm:px-4">
        <p className="eyebrow">The pipeline</p>
        <p className="mt-4 max-w-[34ch] text-[22px] leading-[1.2] font-medium tracking-[-.04em]">Source to bytecode, bytecode to C, C to the committed image, the image to a trace.</p>
        <div className="mt-6 flex flex-wrap items-center gap-4">
          <button onClick={onNew} className="btn btn-invert max-lg:h-11">Open the editor</button>
          <span className="text-[13px] opacity-80">Compile &amp; Run with <span className="kbd">⌘</span> or <span className="kbd">Ctrl</span> + <span className="kbd">Enter</span></span>
        </div>
      </section>
    </div>
  );
}
