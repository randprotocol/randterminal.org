import { describe, expect, it } from 'vitest';
import { railCaption, railFigures, railStatus } from '@/lib/rail';
import type { CompileView, ImageView, RunSummary, TranspileView } from '@/lib/types';

const evm = { lang: 'evm', contract: 'ERC20', size_bytes: 1296 } as CompileView;
const sbpf = { lang: 'sbpf', elf_bytes: 18432, instructions: 2047 } as CompileView;
const transpile = { stats: { c_lines: 1243 } } as TranspileView;
const image = { words: 11686 } as ImageView;
const run = { cycles: 66235, trap: null, tiers: [{ tier: 16, fits: false }, { tier: 18, fits: true }, { tier: 20, fits: true }] } as unknown as RunSummary;

describe('rail figures', () => {
  it('nothing is known before a job has produced a view', () => {
    expect(railFigures({ compile: null, transpile: null, image: null, run: null })).toEqual({ compile: null, analyse: null, transpile: null, image: null, run: null });
  });
  it('an EVM program reads as what it becomes at each stage', () => {
    const f = railFigures({ compile: evm, transpile, image, run });
    expect(f.compile).toEqual({ value: 'ERC20', unit: 'contract' });
    expect(f.analyse).toEqual({ value: '1 296', unit: 'bytes' });
    expect(f.transpile).toEqual({ value: '1 243', unit: 'lines of C' });
    expect(f.image).toEqual({ value: '11 686', unit: 'words' });
    expect(f.run).toEqual({ value: '66 235', unit: 'cycles', note: 'needs tier 18' });
  });
  it('an sBPF program reports its ELF and instructions', () => {
    const f = railFigures({ compile: sbpf, transpile: null, image: null, run: null });
    expect(f.compile).toEqual({ value: '18 432', unit: 'byte ELF' });
    expect(f.analyse).toEqual({ value: '2 047', unit: 'instructions' });
  });
  it('a trapped run has no tier to report', () => {
    expect(railFigures({ compile: evm, transpile, image, run: { ...run, trap: 'bad' } }).run).toEqual({ value: '66 235', unit: 'cycles', note: 'trapped' });
  });
});

describe('rail status', () => {
  it('a finished run makes the last stage done, a trap makes it failed', () => {
    expect(railStatus('queued', 'run', null)).toBe('ready');
    expect(railStatus('queued', 'run', run)).toBe('done');
    expect(railStatus('queued', 'run', { ...run, trap: 'bad' })).toBe('failed');
  });
  it('other stages keep their job status', () => {
    expect(railStatus('running', 'image', null)).toBe('running');
    expect(railStatus('idle', 'compile', run)).toBe('idle');
  });
  it('an idle stage says what it will do; others say where they are', () => {
    expect(railCaption('idle', 'transpile')).toBe('Bytecode to C');
    expect(railCaption('running', 'transpile')).toBe('Working…');
    expect(railCaption('failed', 'transpile')).toBe('Failed');
    expect(railCaption('ready', 'run')).toBe('Ready to run');
  });
});
