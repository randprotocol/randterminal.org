export const hex32 = (n: number) => `0x${(n >>> 0).toString(16).padStart(8, '0')}`;
export const hexAddr = (n: number) => `0x${(n >>> 0).toString(16)}`;
export const groupDigits = (n: number) => n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
export const short = (h: string, n = 8) => (h.length > n ? `${h.slice(0, n)}…` : h);
/** The emulator addresses RAM in words; the developer reads byte addresses. */
export const byteAddr = (wordAddr: number) => (wordAddr * 4) >>> 0;
/** One line for the cycle's RAM access: a store shows the word it replaced and the word it wrote. */
export function memAccess(c: { mem_read: boolean; mem_write: boolean; mem_addr: number; mem_val: number; mem_old: number }): string | null {
  const at = hexAddr(byteAddr(c.mem_addr));
  if (c.mem_write) return `store ${at}: ${hex32(c.mem_old)} → ${hex32(c.mem_val)}`;
  if (c.mem_read) return `load ${at} → ${hex32(c.mem_val)}`;
  return null;
}
