import { describe, expect, it } from 'vitest';
import { layout } from '@/lib/graph-layout';
import { linesOf, opAt } from '@/components/TranspilePanel';
import { rowsOf } from '@/components/AnalysePanel';
import type { CompileView, Graph } from '@/lib/types';

const node = (id: number, start: number) => ({ id, label: String(start), start, end: start + 1, term: '' });

describe('layout', () => {
  it('ranks by longest forward path and keeps pc order in a rank', () => {
    const g: Graph = { nodes: [node(0, 0), node(1, 10), node(2, 20), node(3, 30)], edges: [
      { from: 0, to: 1, kind: 'fallthrough' }, { from: 0, to: 2, kind: 'taken' }, { from: 1, to: 3, kind: 'fallthrough' }, { from: 2, to: 3, kind: 'fallthrough' }, { from: 3, to: 0, kind: 'jump' },
    ] };
    const l = layout(g, { nodeW: 100, nodeH: 40, gapX: 20, gapY: 30 });
    const y = Object.fromEntries(l.nodes.map((n) => [n.id, n.y]));
    expect(y[0]).toBe(0);
    expect(y[1]).toBe(70);
    expect(y[2]).toBe(70);
    expect(y[3]).toBe(140);
    const r1 = l.nodes.filter((n) => n.y === 70).map((n) => n.id);
    expect(r1).toEqual([1, 2]);
    expect(l.edges).toHaveLength(5);
    expect(l.truncated).toBe(false);
  });
  it('places unreachable nodes after the reachable ones', () => {
    const l = layout({ nodes: [node(0, 0), node(1, 5)], edges: [] });
    expect(l.nodes.find((n) => n.id === 1)!.y).toBeGreaterThan(0);
  });
  it('caps the node count', () => {
    const nodes = Array.from({ length: 1000 }, (_, i) => node(i, i));
    const l = layout({ nodes, edges: [] }, { maxNodes: 400 });
    expect(l.nodes).toHaveLength(400);
    expect(l.truncated).toBe(true);
  });
  it('lays out a synthetic empty trap block whose start is past the last slot', () => {
    const g: Graph = { nodes: [node(0, 0), { id: 1, label: 'trap', start: 99, end: 99, term: 'trap' }], edges: [{ from: 0, to: 1, kind: 'taken' }] };
    const l = layout(g);
    expect(l.nodes).toHaveLength(2);
    expect(l.edges[0]!.points.every(([x, y]) => Number.isFinite(x) && Number.isFinite(y))).toBe(true);
  });
});

describe('op ↔ C lines', () => {
  const m = [{ pc: 0, line: 105 }, { pc: 2, line: 106 }, { pc: 4, line: 107 }];
  it('owns lines up to the next marker', () => { expect(linesOf(m, 2, 200)).toEqual([106, 106]); expect(linesOf(m, 4, 200)).toEqual([107, 200]); expect(linesOf(m, 9, 200)).toBeNull(); });
  it('finds the owner of a line', () => { expect(opAt(m, 104)).toBeNull(); expect(opAt(m, 106)).toBe(2); expect(opAt(m, 150)).toBe(4); });
});

describe('rowsOf', () => {
  it('keeps a synthetic empty trap block out of the selectable slots', () => {
    const view = { lang: 'sbpf', functions: [{ entry: 0, blocks: [
      { start: 0, end: 1, term: 'exit', insns: [{ pc: 0, text: 'mov r0, 0', opc: 0xb7 }, { pc: 1, text: 'exit', opc: 0x95 }] },
      { start: 2, end: 2, term: 'trap', insns: [] },
    ] }] } as unknown as CompileView;
    const rows = rowsOf(view);
    expect(rows.filter((r) => !r.header && !r.inert).map((r) => r.pc)).toEqual([0, 1]);
    expect(rows.at(-1)).toMatchObject({ inert: true, pc: 2 });
  });
});
