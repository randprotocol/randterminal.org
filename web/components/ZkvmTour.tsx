'use client';
import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ThemeSwitch } from './ThemeSwitch';
import { Facts, Identity } from './ui';
import { ERC20_CYCLES, FRI, REG_NAMES, SAMPLES, STEPS, autoNext, friFolds, group, largestLogHeight, log2, registersAfter, stepBy, tableBars, transitionResiduals, TIERS, type SampleFacts, type StepId } from '@/lib/zkvm';

const AUTOPLAY_MS = 5200;
const hex = (n: number) => `0x${n.toString(16)}`;
// The first bytes each compiler hands the transpiler (erc20.runtime.hex; spl_token.so's ELF header).
const LEAD_BYTES: Record<SampleFacts['id'], string> = {
  erc20: '608060405234801561000f575f5ffd5b5060043610610060575f3560e01c8063095ea7b3146100645780',
  spl: '7f454c46020101000000000000000000030007010100000028080000000000004000000000000000f8a5010000000000',
};

function useReducedMotion() {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const m = window.matchMedia('(prefers-reduced-motion: reduce)');
    const on = () => setReduced(m.matches);
    on(); m.addEventListener('change', on);
    return () => m.removeEventListener('change', on);
  }, []);
  return reduced;
}

/** A counter that advances every `ms` while `run` is true, wrapping at `n`. */
function useTick(run: boolean, ms: number, n: number) {
  const [k, setK] = useState(0);
  useEffect(() => {
    if (!run) return;
    const t = setInterval(() => setK((x) => (x + 1) % n), ms);
    return () => clearInterval(t);
  }, [run, ms, n]);
  return [k, setK] as const;
}

export function ZkvmTour() {
  const [i, setI] = useState(0);
  const [sid, setSid] = useState<SampleFacts['id']>('erc20');
  const [playing, setPlaying] = useState(false);
  const reduced = useReducedMotion();
  const s = SAMPLES[sid];
  const step = STEPS[i]!;
  const prev = i > 0 ? STEPS[i - 1]! : null;
  const threadRef = useRef<HTMLOListElement>(null);

  useEffect(() => {
    if (!playing) return;
    const t = setTimeout(() => { const n = autoNext(i); if (n == null) setPlaying(false); else setI(n); }, AUTOPLAY_MS);
    return () => clearTimeout(t);
  }, [playing, i]);
  // Keep the current stage in view where the thread scrolls (phones).
  useEffect(() => { threadRef.current?.querySelector<HTMLElement>('[aria-current="step"]')?.scrollIntoView({ block: 'nearest', inline: 'center', behavior: reduced ? 'auto' : 'smooth' }); }, [i, reduced]);

  const go = useCallback((n: number) => { setI(n); setPlaying(false); }, []);
  const keys = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowRight') { e.preventDefault(); go(stepBy(i, 1)); }
    if (e.key === 'ArrowLeft') { e.preventDefault(); go(stepBy(i, -1)); }
  };

  return (
    <div className="min-h-dvh">
      <header className="flex h-12 items-center gap-2 border-b border-border-soft px-[16px]">
        <Link href="/" className="mono font-semibold whitespace-nowrap text-text-strong hover:text-accent-2">Rand Terminal</Link>
        <Link href="/" className="ml-auto inline-flex h-8 items-center rounded-sm px-2 text-text-soft hover:bg-surface-2 hover:text-text-strong max-lg:h-11">Open the terminal</Link>
        <ThemeSwitch />
      </header>

      <main className="mx-auto max-w-[1180px] px-[16px] pt-8 pb-16 sm:px-[24px]">
        <div className="flex flex-wrap items-end justify-between gap-x-10 gap-y-4">
          <div className="max-w-[62ch]">
            <h1 className="text-[28px] leading-tight font-medium tracking-tight text-text-strong sm:text-[34px]">How the zkVM works</h1>
            <p className="mt-2 text-[15px] leading-relaxed text-text-soft">One program, from source to a verified proof, the way the terminal and the circuits it pins (85b6f46) really do it. Every number here comes from their code or a recorded run.</p>
          </div>
          <div role="radiogroup" aria-label="sample" className="flex gap-1 rounded-md border border-border-soft bg-bg-soft p-1">
            {Object.values(SAMPLES).map((x) => (
              <button key={x.id} role="radio" aria-checked={sid === x.id} onClick={() => setSid(x.id)} className="seg max-lg:h-11">{x.label}</button>
            ))}
          </div>
        </div>

        <nav aria-label="stages" className="zk-thread-wrap mt-8" onKeyDown={keys}>
          <ol ref={threadRef} className="zk-thread" style={{ ['--i' as string]: i }}>
            {STEPS.map((x, k) => (
              <li key={x.id} data-state={k < i ? 'past' : k === i ? 'now' : 'next'}>
                <button onClick={() => go(k)} aria-current={k === i ? 'step' : undefined} className="zk-node">
                  <span className="zk-n">{k + 1}</span>
                  <span className="zk-title">{x.title}</span>
                </button>
              </li>
            ))}
            <li aria-hidden className="zk-token" key={`${sid}-${i}`}>{step.token(s)}</li>
          </ol>
        </nav>

        <section aria-labelledby="zk-step-title" className="mt-6 grid gap-6 lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)] lg:gap-10">
          <div className="min-w-0">
            <p className="text-xs text-text-mute">Step {i + 1} of {STEPS.length}</p>
            <h2 id="zk-step-title" className="mt-1 text-[22px] leading-snug font-medium text-text-strong">{step.title}</h2>
            <div className="zk-prose mt-3"><Explain id={step.id} s={s} /></div>
            <div className="mt-6 flex flex-wrap items-center gap-2">
              <button onClick={() => go(stepBy(i, -1))} disabled={i === 0} className="zk-btn">Back</button>
              <button onClick={() => (playing ? setPlaying(false) : (i === STEPS.length - 1 ? (setI(0), setPlaying(true)) : setPlaying(true)))} aria-pressed={playing} className="zk-btn">{playing ? 'Pause' : i === STEPS.length - 1 ? 'Play from the start' : 'Play'}</button>
              <button onClick={() => go(stepBy(i, 1))} disabled={i === STEPS.length - 1} className="zk-btn zk-btn-primary">Next</button>
            </div>
            {playing && <div className="zk-progress mt-3" key={i} style={{ animationDuration: `${AUTOPLAY_MS}ms` }} aria-hidden />}
          </div>
          <div className="min-w-0">
            <Handoff from={prev ? prev.token(s) : null} to={step.token(s)} key={`${sid}-${i}`} />
            <div className="zk-stage mt-3" aria-live="polite">
              <Viz id={step.id} s={s} reduced={reduced} />
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}

/** What moved into this stage: the previous stage's output becoming this one's. */
function Handoff({ from, to }: { from: string | null; to: string }) {
  return (
    <div className="zk-handoff" aria-label={from ? `${from} becomes ${to}` : to}>
      {from && <span className="zk-pill zk-pill-from">{from}</span>}
      {from && <svg className="zk-flow" viewBox="0 0 120 12" preserveAspectRatio="none" aria-hidden><line x1="0" y1="6" x2="112" y2="6" /><path d="M112 1 120 6 112 11z" /></svg>}
      <span className="zk-pill zk-pill-to">{to}</span>
    </div>
  );
}

function Code({ path }: { path: string }) {
  return <p className="zk-src">In the code: <code>{path}</code></p>;
}

function Explain({ id, s }: { id: StepId; s: SampleFacts }) {
  const evm = s.id === 'erc20';
  switch (id) {
    case 'source': return <>
      <p>{evm ? 'A Solidity contract: the ERC-20 sample, whose transfer this page follows.' : 'A Solana program: SPL Token exactly as it runs on mainnet. The terminal does not upload programs; this one ships with it.'}</p>
      <p>Nothing about the program is built into the prover. The proof will say only that some program with commitment hc ran on some inputs and produced these outputs.</p>
    </>;
    case 'compile': return <>
      <p>{evm ? <>solc 0.8.37 compiles it in the sandbox for the fork you pick (shanghai unless you choose another); the zkVM’s EVM runs every fork through osaka, TLOAD, TSTORE, MCOPY and CLZ included. The transpiler takes the runtime bytecode: what the chain stores, not the constructor.</> : <>The ELF is what <code>cargo build-sbf</code> produced upstream: sBPF instructions in <code>.text</code>, constants in <code>.rodata</code>. Its compile stage is skipped.</>}</p>
      <Facts items={s.compiledFacts} />
    </>;
    case 'transpile': return <>
      <p>{evm ? <><b>evm2rv</b> splits the bytecode into basic blocks at every JUMPDEST and terminator, works out each block’s static gas and stack bounds once, and writes C that charges and checks them at the block head instead of per opcode.</> : <><b>sbpf2rv</b> scans the ELF into functions and blocks and writes C over the same eleven registers, with sBPF’s compute budget charged per block.</>}</p>
      <p className="text-text-mute">{s.pairsNote}</p>
      <Code path={evm ? 'circuits/evm2rv/src/blocks.rs, emit.rs' : 'circuits/sbpf2rv/src/emit.rs'} />
    </>;
    case 'image': return <>
      <p><b>rand-guest build</b> compiles that C with clang for RV32IM (32-bit integer RISC-V with multiply and divide), links it behind a prologue, and checks the result: no instruction outside the zkVM’s ISA, at most 65 535 words.</p>
      <p>Its digest, hc, is the zkVM’s commitment to this exact program. The proof recomputes it with Poseidon2 inside the CPU table, so a proof for one program can never pass as another’s.</p>
      <Code path="circuits/rand-guest" />
    </>;
    case 'execute': return <>
      <p>The emulator runs the image one instruction per cycle and records each cycle: pc, the instruction word, the register it writes, the memory it touches, the ALU work it hands off.</p>
      <p>{group(s.cycles)} cycles for {evm ? 'the transfer' : 'Transfer(250)'}. {s.cyclesNote}</p>
      <Code path="circuits/research/src/emulator.rs" />
    </>;
    case 'tables': return <>
      {/* Nineteen: the bus constants in circuits/research/src/tables/mod.rs at 85b6f46 (MODQ and MODQ_WIRE joined after BLIND); docs/02-tables-and-buses.md counts the same. */}
      <p>The record becomes tables, one per chip, that exchange facts over nineteen LogUp buses instead of calling each other. A tier t fixes their heights to powers of two: CPU 2<sup>t</sup>, ALU 2<sup>t+1</sup>, Memory and Poseidon2 at least 2<sup>t+2</sup>. Unused rows are padding.</p>
      <p>The smallest tier that holds every table wins: here, tier {s.tier}. Range and nibble tables are fixed at 256 rows; {evm ? `${s.rows.keccak_permutations} Keccak permutations add a Keccak table.` : 'SHA-256 compressions add a SHA-256 table.'}</p>
      <Code path="circuits/research/src/machine.rs (Tier), api/crates/terminal-core/src/tiers.rs" />
    </>;
    case 'air': return <>
      <p>Each table is an AIR: polynomial equations every row, or every pair of neighbouring rows, must satisfy. This is one of the CPU table’s transition constraints, as written:</p>
      <pre className="zk-code">t.assert_zero(n(IS_REAL) * (n(PC) - v(NEXT_PC)));</pre>
      <p>In words: if the next row is a real cycle, its pc is the next_pc this row computed. Its neighbour <code>n(IS_REAL) * (n(CLK) - v(CLK) - 1)</code> makes the clock tick by exactly one.</p>
      <Code path="circuits/research/src/tables/cpu.rs" />
    </>;
    case 'commit': return <>
      <p>Each column is interpolated over the Goldilocks field (p = 2<sup>64</sup> − 2<sup>32</sup> + 1) and evaluated on a domain eight times larger (log_blowup 3): the low-degree extension.</p>
      <p>The extended rows become the leaves of a Merkle tree hashed with Poseidon2, salted so the root reveals nothing about the trace. All tables go under one commitment; its root is all the verifier receives of the trace.</p>
      <Code path="circuits/research/src/machine.rs (generic_config: HidingFriPcs)" />
    </>;
    case 'fri': return <>
      <p>Random challenges, drawn from a Poseidon2 transcript of the commitments, fold every constraint into one quotient. FRI then proves that quotient is a low-degree polynomial: each round commits a codeword folded up to eight to one, until a constant is left.</p>
      <p>The largest table here is 2<sup>{largestLogHeight(s)}</sup> rows, so its extension is 2<sup>{largestLogHeight(s) + FRI.logBlowup}</sup> points. Challenges live in the degree-2 extension of Goldilocks.</p>
      <Code path="FriParameters in circuits/research/src/machine.rs" />
    </>;
    case 'verify': return <>
      <p>The verifier never sees the trace. It replays the transcript, checks the proof of work, checks that every bus balances and every AIR holds at one random point, and opens {FRI.queries} random positions down the FRI layers along their Merkle paths.</p>
      <p>The production profile (80 queries, 20 bits of grinding, blowup 8) gives about 86 proven bits of soundness. The recursion machine, which proves that proofs verify, uses its own profile for the same bound: blowup 4, 92 queries, 24 bits. The terminal itself stops at the tier: it runs and measures programs; proving is the node’s prover.</p>
      {/* 86.38 against 86.41 proven bits: circuits/recursion/docs/07-rvm-rate-quarter.md; RvmFri::of in circuits/recursion/src/machine.rs (85b6f46). */}
      <Code path="FriProfile::Production, circuits/research/src/machine.rs; RvmFri, circuits/recursion/src/machine.rs" />
    </>;
  }
}

function Viz({ id, s, reduced }: { id: StepId; s: SampleFacts; reduced: boolean }) {
  switch (id) {
    case 'source': return <pre className="zk-code zk-big" aria-label={`${s.sourceName} source`}>{s.source}</pre>;
    case 'compile': return <Bytes hexText={LEAD_BYTES[s.id]} caption={s.id === 'erc20' ? 'The first 40 runtime bytes: PUSH1 0x80 PUSH1 0x40 MSTORE CALLVALUE …' : 'The ELF header: 7f 45 4c 46 is “\\x7fELF”.'} />;
    case 'transpile': return <Pairs s={s} reduced={reduced} />;
    case 'image': return <Image s={s} />;
    case 'execute': return <Execute s={s} reduced={reduced} />;
    case 'tables': return <Tables s={s} />;
    case 'air': return <Air reduced={reduced} />;
    case 'commit': return <Commit />;
    case 'fri': return <Fri s={s} />;
    case 'verify': return <Verify s={s} />;
  }
}

function Bytes({ hexText, caption }: { hexText: string; caption: string }) {
  const bytes = hexText.match(/../g) ?? [];
  return (
    <figure>
      <div className="zk-bytes" role="img" aria-label={`bytes ${bytes.join(' ')}`}>
        {bytes.map((b, k) => <span key={k} style={{ animationDelay: `${k * 28}ms` }}>{b}</span>)}
      </div>
      <figcaption className="zk-cap">{caption}</figcaption>
    </figure>
  );
}

function Pairs({ s, reduced }: { s: SampleFacts; reduced: boolean }) {
  const [k] = useTick(!reduced, 1100, s.pairs.length);
  return (
    <figure>
      <table className="zk-pairs" key={s.id}>
        <thead><tr><th>{s.id === 'erc20' ? 'EVM opcode' : 'sBPF slot'}</th><th>C</th></tr></thead>
        <tbody>{s.pairs.map((p, r) => <tr key={r} data-on={!reduced && r === k ? '' : undefined}><td>{p.src}</td><td>{p.c || <span className="text-text-mute">(folded into the next line)</span>}</td></tr>)}</tbody>
      </table>
      <figcaption className="zk-cap">{s.id === 'erc20' ? 'Block L_0 of the ERC-20 sample’s contract.c, behind its head: stack check, evm_charge(30).' : 'Function f_225 of the SPL Token program.c.'}</figcaption>
    </figure>
  );
}

function Image({ s }: { s: SampleFacts }) {
  const total = s.wordsSplit.reduce((a, [, n]) => a + n, 0);
  return (
    <figure className="flex flex-col gap-4">
      <div className="zk-words" role="img" aria-label={s.wordsSplit.map(([l, n]) => `${l} ${group(n)} words`).join(', ')}>
        {s.wordsSplit.map(([l, n]) => <span key={l} style={{ flexGrow: n }} data-part={l}><b>{l}</b> {group(n)}</span>)}
      </div>
      <Facts items={[['Program', `${group(total)} RV32IM words`], ['Cap', '65 535 words'], ['Fits', 'yes']]} />
      <Identity name="hc" meaning="the zkVM's commitment to this exact program" value={s.hc} />
    </figure>
  );
}

function Execute({ s, reduced }: { s: SampleFacts; reduced: boolean }) {
  const n = ERC20_CYCLES.length;
  const [k, setK] = useTick(!reduced, 900, n + 1);
  const regs = registersAfter(ERC20_CYCLES, k);
  const cur = ERC20_CYCLES[Math.min(k, n - 1)]!;
  return (
    <figure>
      <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
        <p className="zk-clk"><span>clk</span> {k}</p>
        <p className="zk-clk"><span>pc</span> {hex(k < n ? cur.pc : ERC20_CYCLES[n - 1]!.next_pc)}</p>
        <button className="quiet ml-auto" onClick={() => setK((x) => (x + 1) % (n + 1))}>Step one cycle</button>
      </div>
      <table className="zk-pairs mt-2">
        <thead><tr><th>clk</th><th>pc</th><th>instruction</th><th>writes</th></tr></thead>
        <tbody>{ERC20_CYCLES.map((c) => <tr key={c.clk} data-on={c.clk === k ? '' : undefined} data-done={c.clk < k ? '' : undefined}>
          <td>{c.clk}</td><td>{hex(c.pc)}</td><td>{c.text}</td>
          <td>{c.store ? `mem[${hex(c.store.addr)}] ← ${group(c.store.val)}` : `${REG_NAMES[c.rd] ?? `x${c.rd}`} ← ${group(c.rd_val)}`}</td>
        </tr>)}</tbody>
      </table>
      <div className="zk-regs mt-3" aria-label="registers">{REG_NAMES.map((r, x) => <span key={r} data-hot={k > 0 && ERC20_CYCLES[k - 1]?.rd === x ? '' : undefined}><b>{r}</b>{group(regs[x]!)}</span>)}</div>
      <figcaption className="zk-cap">The first ten of the ERC-20 transfer’s {group(66235)} cycles, as recorded: the prologue pointing t1 at the data segment and storing addresses into it.{s.id === 'spl' ? ' (The SPL run is not recorded cycle by cycle here.)' : ''}</figcaption>
    </figure>
  );
}

function Tables({ s }: { s: SampleFacts }) {
  const bars = tableBars(s.rows, s.tier);
  const total = s.rows.cpu + s.rows.digest_rows;
  return (
    <figure className="flex flex-col gap-4">
      <ul className="zk-bars">{bars.map((b) => (
        <li key={b.name}>
          <div className="flex items-baseline justify-between gap-3"><b>{b.name}</b><span className="mono text-xs text-text-mute">{group(b.used)} of 2<sup>{log2(b.height)}</sup> rows</span></div>
          <div className="zk-bar"><span style={{ width: `${Math.max(0.6, (b.used / b.height) * 100)}%` }} /></div>
          <p className="text-xs text-text-mute">{b.what}</p>
        </li>
      ))}</ul>
      <div>
        <p className="text-xs text-text-mute">Which tier holds {group(total)} CPU rows (2<sup>t</sup> − 1 at most)?</p>
        <ol className="zk-tiers mt-1.5">{TIERS.map((t) => { const fits = total <= 2 ** t - 1; return <li key={t} data-fit={fits ? '' : undefined} data-pick={t === s.tier ? '' : undefined}>{t}<span>{t === s.tier ? 'chosen' : fits ? 'fits' : 'too small'}</span></li>; })}</ol>
      </div>
    </figure>
  );
}

function Air({ reduced }: { reduced: boolean }) {
  const [tamper, setTamper] = useState(false);
  const n = ERC20_CYCLES.length - 1;
  const [k] = useTick(!reduced, 1000, n);
  const rows = ERC20_CYCLES.map((c, r) => ({ clk: c.clk, pc: tamper && r === 5 ? c.pc + 4 : c.pc, next_pc: c.next_pc, is_real: 1 as const }));
  return (
    <figure>
      <table className="zk-pairs">
        <thead><tr><th>row → next</th><th>next_pc</th><th>pc′</th><th>is_real′ · (pc′ − next_pc)</th></tr></thead>
        <tbody>{rows.slice(0, n).map((v, r) => { const nx = rows[r + 1]!; const res = transitionResiduals(v, nx).pc; return (
          <tr key={r} data-on={!reduced && r === k ? '' : undefined} data-bad={res !== 0 ? '' : undefined}>
            <td>{r} → {r + 1}</td><td>{hex(v.next_pc)}</td><td>{hex(nx.pc)}</td><td className={res === 0 ? 'text-positive' : 'text-negative'}>{res}</td>
          </tr>); })}</tbody>
      </table>
      <div className="mt-3 flex flex-wrap items-center gap-3">
        <button className="zk-btn" aria-pressed={tamper} onClick={() => setTamper(!tamper)}>{tamper ? 'Restore the trace' : 'Skip an instruction at row 5'}</button>
        <p className="text-xs text-text-soft" role="status">{tamper ? 'Pair 4 → 5 no longer gives 0. The constraint fails on that row, no low-degree quotient exists, and no proof can be made for this trace.' : 'Zero on every pair of the recorded ERC-20 rows.'}</p>
      </div>
    </figure>
  );
}

function Commit() {
  // A column of 8 values, its blowup-8 extension (64 points), and a binary tree over 8 leaf groups.
  const leaves = 8;
  const levels = [8, 4, 2, 1];
  return (
    <figure>
      <svg className="zk-commit" viewBox="0 0 400 230" role="img" aria-label="A column of 8 trace values extends to 64 points; groups of extended rows hash up a Merkle tree to one root">
        <text x="6" y="14" className="zk-svg-label">trace column, 8 rows</text>
        {Array.from({ length: 8 }, (_, r) => <rect key={r} className="zk-cell" x={6 + r * 14} y={22} width={12} height={12} style={{ animationDelay: `${r * 30}ms` }} />)}
        <text x="6" y="58" className="zk-svg-label">low-degree extension ×8, 64 points</text>
        {Array.from({ length: 64 }, (_, r) => <rect key={r} className={`zk-cell ${r % 8 === 0 ? '' : 'zk-cell-ext'}`} x={6 + r * 6} y={66} width={5} height={12} style={{ animationDelay: `${300 + r * 8}ms` }} />)}
        {levels.map((count, l) => Array.from({ length: count }, (_, j) => {
          const w = 384 / count, cx = 6 + w * j + w / 2, cy = 210 - l * 36;
          const child = l > 0 ? [cx - w / 4, cx + w / 4] : null;
          return <g key={`${l}-${j}`} className="zk-node-g" style={{ animationDelay: `${900 + l * 260}ms` }}>
            {child && child.map((x2, c) => <line key={c} x1={cx} y1={cy + 6} x2={x2} y2={cy + 30} className="zk-edge" />)}
            {l === 0 && <line x1={cx} y1={cy - 6} x2={cx} y2={84} className="zk-edge zk-edge-soft" />}
            <circle cx={cx} cy={cy} r={l === levels.length - 1 ? 8 : 5} className={l === levels.length - 1 ? 'zk-root' : 'zk-hash'} />
          </g>;
        }))}
        <text x="214" y="106" className="zk-svg-label">root</text>
      </svg>
      <figcaption className="zk-cap">Drawn small: {leaves} leaf hashes here, one per extended row in the real tree. Poseidon2 over Goldilocks hashes each level.</figcaption>
    </figure>
  );
}

function Fri({ s }: { s: SampleFacts }) {
  const folds = friFolds(largestLogHeight(s));
  const top = folds[0]!;
  return (
    <figure>
      <ol className="zk-fri">{folds.map((f, r) => (
        <li key={r} style={{ animationDelay: `${r * 160}ms` }}>
          <span className="zk-fri-bar" style={{ width: `${(f / top) * 100}%` }} />
          <span className="mono text-xs">2<sup>{f}</sup> points{r === 0 ? ', the extension' : r === folds.length - 1 ? ', the final polynomial’s codeword' : `, fold ${r} (÷${2 ** (folds[r - 1]! - f)})`}</span>
        </li>
      ))}</ol>
      <Facts items={[['Queries', String(FRI.queries)], ['Proof of work', `${FRI.powBits} bits`], ['Blowup', `${2 ** FRI.logBlowup}`], ['Fold', 'up to 8 to 1']]} />
      <figcaption className="zk-cap">Bar length is the codeword’s log size. A sketch of p3-fri’s schedule from FriParameters; where smaller tables join a round, it can fold by less.</figcaption>
    </figure>
  );
}

function Verify({ s }: { s: SampleFacts }) {
  const checks = [
    'Replay the Fiat–Shamir transcript: same commitments, same challenges',
    `Check the ${FRI.powBits}-bit proof-of-work nonce`,
    'Every one of the nineteen LogUp buses balances across the tables',
    'Every AIR, through its quotient, holds at the random point',
    `${FRI.queries} queries open the commitments along Merkle paths and fold down to the final polynomial`,
    'The public values match: hc and the outputs',
  ];
  return (
    <figure>
      <ol className="zk-checks">{checks.map((c, r) => <li key={r} style={{ animationDelay: `${r * 220}ms` }}>{c}</li>)}</ol>
      <p className="zk-accept" style={{ animationDelay: `${checks.length * 220}ms` }}>Accept: program {s.hc.slice(0, 8)}… ran and produced these outputs</p>
    </figure>
  );
}
