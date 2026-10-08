import { describe, expect, it } from 'vitest';
import { filesFor, reduce, stageStatus, type JobState } from '@/lib/job';
import type { JobView } from '@/lib/types';

const q = { status: 'queued' as const };
const job: JobView = { id: 'j', lang: 'solidity', options: { chain_id: 20, stage: 2 }, created: 1, compile: q, transpile: q, build: q, console: [], finished: false };
const empty: JobState = { job: null, console: [], error: null };

describe('job reducer', () => {
  it('takes the snapshot', () => { expect(reduce(empty, { type: 'job', job }).job).toEqual(job); });
  it('applies stage changes without mutating', () => {
    const s1 = reduce(empty, { type: 'job', job });
    const s2 = reduce(s1, { type: 'stage', stage: 'compile', state: { status: 'done', started: 1, ended: 2 } });
    expect(s2.job!.compile.status).toBe('done');
    expect(s1.job!.compile.status).toBe('queued');
  });
  it('appends console lines and marks done', () => {
    let s = reduce(empty, { type: 'job', job: { ...job, console: [{ stage: 'compile', stream: 'stdout', text: 'a' }] } });
    s = reduce(s, { type: 'console', line: { stage: 'build', stream: 'stderr', text: 'b' } });
    expect(s.console.map((l) => l.text)).toEqual(['a', 'b']);
    expect(reduce(s, { type: 'done' }).job!.finished).toBe(true);
  });
  it('maps the five panels onto the three stages', () => {
    const j = { ...job, compile: { status: 'done' as const }, transpile: { status: 'running' as const } };
    expect(stageStatus(j, 'compile')).toBe('done');
    expect(stageStatus(j, 'analyse')).toBe('done');
    expect(stageStatus(j, 'transpile')).toBe('running');
    expect(stageStatus(j, 'image')).toBe('queued');
    expect(stageStatus(j, 'run')).toBe('idle');
    expect(stageStatus({ ...j, build: { status: 'done' } }, 'run')).toBe('queued');
    expect(stageStatus(null, 'compile')).toBe('idle');
  });
  it('builds request files', () => {
    expect(filesFor('solidity', { 'A.sol': 'x' })).toEqual([{ name: 'A.sol', encoding: 'text', content: 'x' }]);
    expect(filesFor('solana-elf', {}, 'QUJD')).toEqual([{ name: 'program.so', encoding: 'base64', content: 'QUJD' }]);
    expect(filesFor('solana-rust', { 'lib.rs': 'fn f(){}', 'Cargo.toml': '[package]' }).map((f) => f.name)).toEqual(['lib.rs', 'Cargo.toml']);
  });
});
