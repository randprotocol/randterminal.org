export const hex32 = (n: number) => `0x${(n >>> 0).toString(16).padStart(8, '0')}`;
export const hexAddr = (n: number) => `0x${(n >>> 0).toString(16)}`;
export const groupDigits = (n: number) => n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
export const short = (h: string, n = 8) => (h.length > n ? `${h.slice(0, n)}…` : h);
/** The emulator addresses RAM in words; the developer reads byte addresses. */
export const byteAddr = (wordAddr: number) => (wordAddr * 4) >>> 0;
