export function visibleRange(scrollTop: number, viewport: number, rowH: number, n: number, overscan = 8): [number, number] {
  if (n === 0) return [0, 0];
  const first = Math.max(0, Math.floor(scrollTop / rowH) - overscan);
  const last = Math.min(n, Math.ceil((scrollTop + viewport) / rowH) + overscan);
  return [first, last];
}
