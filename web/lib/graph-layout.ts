import type { Graph, GraphNode } from './types';

export interface Placed extends GraphNode { x: number; y: number }
export interface Routed { from: number; to: number; kind: string; points: [number, number][] }

export function layout(g: Graph, o: { nodeW?: number; nodeH?: number; gapX?: number; gapY?: number; maxNodes?: number } = {}) {
  const W = o.nodeW ?? 120, H = o.nodeH ?? 36, GX = o.gapX ?? 24, GY = o.gapY ?? 36, MAX = o.maxNodes ?? 400;
  const truncated = g.nodes.length > MAX;
  const nodes = g.nodes.slice(0, MAX);
  const ids = new Set(nodes.map((n) => n.id));
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const edges = g.edges.filter((e) => ids.has(e.from) && ids.has(e.to));
  const forward = edges.filter((e) => byId.get(e.to)!.start > byId.get(e.from)!.start);
  // Longest path over forward edges, in pc order (forward edges only go up in pc, so that is topological).
  // A node with no forward predecessor (the entry, each function entry, an unreachable block) is a root at rank 0.
  const preds = new Map<number, number[]>();
  for (const e of forward) preds.set(e.to, [...(preds.get(e.to) ?? []), e.from]);
  const rank = new Map<number, number>();
  const order = [...nodes].sort((a, b) => a.start - b.start);
  for (const n of order) rank.set(n.id, Math.max(-1, ...(preds.get(n.id) ?? []).map((p) => rank.get(p) ?? -1)) + 1);
  const rows = new Map<number, GraphNode[]>();
  for (const n of order) { const r = rank.get(n.id)!; rows.set(r, [...(rows.get(r) ?? []), n]); }
  const placed: Placed[] = [];
  let width = 0;
  for (const [r, row] of rows) row.forEach((n, i) => { placed.push({ ...n, x: i * (W + GX), y: r * (H + GY) }); width = Math.max(width, (i + 1) * (W + GX)); });
  const at = new Map(placed.map((p) => [p.id, p]));
  const routed: Routed[] = edges.map((e) => {
    const a = at.get(e.from)!, b = at.get(e.to)!;
    const back = b.y <= a.y;
    const pts: [number, number][] = back
      ? [[a.x + W, a.y + H / 2], [Math.max(a.x, b.x) + W + GX / 2, a.y + H / 2], [Math.max(a.x, b.x) + W + GX / 2, b.y + H / 2], [b.x + W, b.y + H / 2]]
      : [[a.x + W / 2, a.y + H], [a.x + W / 2, a.y + H + GY / 2], [b.x + W / 2, a.y + H + GY / 2], [b.x + W / 2, b.y]];
    return { ...e, points: pts };
  });
  const height = (Math.max(0, ...placed.map((p) => p.y)) + H);
  return { nodes: placed, edges: routed, width, height, truncated, shown: nodes.length };
}
