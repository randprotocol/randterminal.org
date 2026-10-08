import { describe, expect, it } from 'vitest';
import { byteAddr, groupDigits, hex32, hexAddr, short } from '@/lib/format';

describe('format', () => {
  it('hex32 pads to eight digits', () => { expect(hex32(0x1f)).toBe('0x0000001f'); expect(hex32(0xffffffff)).toBe('0xffffffff'); });
  it('hexAddr is compact', () => { expect(hexAddr(0xf794)).toBe('0xf794'); });
  it('groups digits with thin spaces', () => { expect(groupDigits(66235)).toBe('66 235'); expect(groupDigits(765851)).toBe('765 851'); expect(groupDigits(12)).toBe('12'); });
  it('shortens hashes', () => { expect(short('a0feae92a7311c95')).toBe('a0feae92…'); });
  it('word to byte addresses', () => { expect(byteAddr(0x4000)).toBe(0x10000); });
});
