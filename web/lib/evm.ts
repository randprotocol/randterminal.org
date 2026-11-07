import type { JobOptions, Lang, WarningView } from './types';

/** The forks solc 0.8.37 accepts (`settings.evmVersion`), oldest first. Mirrors
 *  terminal-core's `stages::EVM_VERSIONS`; the API rejects anything else. */
export const EVM_VERSIONS = ['homestead', 'tangerineWhistle', 'spuriousDragon', 'byzantium', 'constantinople', 'petersburg', 'istanbul', 'berlin', 'london', 'paris', 'shanghai', 'cancun', 'prague', 'osaka'] as const;
export type EvmVersion = (typeof EVM_VERSIONS)[number];
/** The fork the zkVM's EVM implements, and what a job naming none compiles for. */
export const DEFAULT_EVM_VERSION: EvmVersion = 'shanghai';

/** Remove once the circuits pin (9bf3844) moves to the one that adds these opcodes. */
export const FORK_PIN_HINT = 'Cancun+ opcodes (MCOPY, TLOAD/TSTORE, CLZ) need a circuits pin newer than 9bf3844; at this one they trap.';

export const isEvmVersion = (v: unknown): v is EvmVersion => typeof v === 'string' && (EVM_VERSIONS as readonly string[]).includes(v);

/** The hint for a fork newer than shanghai, else null. */
export function forkHint(v: string): string | null {
  const i = EVM_VERSIONS.indexOf(v as EvmVersion);
  return i > EVM_VERSIONS.indexOf(DEFAULT_EVM_VERSION) ? FORK_PIN_HINT : null;
}

/** The options a job request carries: the fork for Solidity, never for Solana. */
export function jobOptions(lang: Lang, options: JobOptions, fork: string): JobOptions {
  const { evm_version: _drop, ...rest } = options;
  return lang === 'solidity' ? { ...rest, evm_version: fork } : rest;
}

type Store = Pick<Storage, 'getItem' | 'setItem'>;
const KEY = 'randterminal.evmVersion';
const session = (): Store | undefined => { try { return typeof sessionStorage === 'undefined' ? undefined : sessionStorage; } catch { return undefined; } };

/** The fork this browser session picked last; shanghai when none (or storage is blocked). */
export function loadFork(store: Store | undefined = session()): EvmVersion {
  try { const v = store?.getItem(KEY); return isEvmVersion(v) ? v : DEFAULT_EVM_VERSION; } catch { return DEFAULT_EVM_VERSION; }
}
export function saveFork(v: EvmVersion, store: Store | undefined = session()): void {
  try { store?.setItem(KEY, v); } catch { /* storage blocked: the pick lasts until reload */ }
}

/** The pcs the zkVM warnings point at, to mark in the opcode listing. */
export function warnedPcs(warnings: WarningView[] | undefined): Set<number> {
  return new Set((warnings ?? []).flatMap((w) => (w.pc == null ? [] : [w.pc])));
}
