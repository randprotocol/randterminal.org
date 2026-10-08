import { describe, expect, it } from 'vitest';
import { visibleRange } from '@/lib/virtual';

describe('visibleRange', () => {
  it('covers the viewport plus overscan', () => { expect(visibleRange(0, 200, 20, 1000, 5)).toEqual([0, 15]); });
  it('clamps at the end', () => { expect(visibleRange(19_900, 200, 20, 1000, 5)).toEqual([990, 1000]); });
  it('handles empty lists', () => { expect(visibleRange(0, 200, 20, 0)).toEqual([0, 0]); });
});
