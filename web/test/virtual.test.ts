import { describe, expect, it } from 'vitest';
import { visibleRange } from '@/lib/virtual';

describe('visibleRange', () => {
  it('covers the viewport plus overscan', () => { expect(visibleRange(0, 200, 20, 1000, 5)).toEqual([0, 15]); });
  it('clamps at the end', () => { expect(visibleRange(19_900, 200, 20, 1000, 5)).toEqual([990, 1000]); });
  it('handles empty lists', () => { expect(visibleRange(0, 200, 20, 0)).toEqual([0, 0]); });
});

import { parseHexPc, rowAt } from '@/components/ImagePanel';
describe('image pc lookup', () => {
  const d = [{ pc: 0xf794 }, { pc: 0xf798 }, { pc: 0x10000 }];
  it('finds the row at or below a pc', () => { expect(rowAt(d, 0xf798)).toBe(1); expect(rowAt(d, 0xf79a)).toBe(1); expect(rowAt(d, 0x20000)).toBe(2); expect(rowAt(d, 0x10)).toBeNull(); });
  it('parses hex strictly', () => { expect(parseHexPc('0xF794')).toBe(0xf794); expect(parseHexPc('f794')).toBe(0xf794); expect(parseHexPc('12zz')).toBeNull(); expect(parseHexPc('')).toBeNull(); });
});
