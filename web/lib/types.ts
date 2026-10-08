// Mirrors api/crates/terminal-core/src/views.rs one to one (snake_case, same tags); change both together.
export type Lang = 'solidity' | 'solana-rust' | 'solana-elf';
export type StageStatus = 'queued' | 'running' | 'done' | 'failed';
export type StageName = 'compile' | 'transpile' | 'build';
export interface JobOptions { contract?: string | null; chain_id: number; stage: number }
export interface Diagnostic { file: string; line: number; col: number; severity: string; message: string }
export interface StageError { kind: string; message: string; diagnostics: Diagnostic[] }
export interface StageState { status: StageStatus; started?: number | null; ended?: number | null; error?: StageError | null }
export interface ConsoleLine { stage: StageName; stream: string; text: string }
export interface JobView { id: string; lang: Lang; options: JobOptions; created: number; compile: StageState; transpile: StageState; build: StageState; console: ConsoleLine[]; finished: boolean }

export interface AbiParam { name: string; type: string }
export interface AbiItem { type: string; name?: string; inputs?: AbiParam[]; outputs?: AbiParam[]; stateMutability?: string }
export interface EvmOpcode { pc: number; opcode: number; mnemonic: string; push_hex?: string | null; static_gas: number; jumpdest: boolean; block: number }
export interface EvmCompileView { lang: 'evm'; contract: string; contracts: string[]; creation_hex: string; runtime_hex: string; size_bytes: number; abi: AbiItem[]; opcodes: EvmOpcode[]; blocks: number; jumpdests: number }
export interface SbpfInsnView { pc: number; text: string; opc: number }
export interface SbpfBlockView { start: number; end: number; term: string; insns: SbpfInsnView[] }
export interface SbpfFunctionView { entry: number; blocks: SbpfBlockView[] }
export interface SbpfSyscallView { hash: number; name: string; count: number; supported: boolean }
export interface WarningView { kind: string; pc?: number | null; detail: string; explanation: string; fatal: boolean }
export interface SbpfCompileView { lang: 'sbpf'; elf_bytes: number; text_bytes: number; rodata_bytes: number; entry_pc: number; functions: SbpfFunctionView[]; instructions: number; syscalls: SbpfSyscallView[]; warnings: WarningView[] }
export type CompileView = EvmCompileView | SbpfCompileView;

export interface GraphNode { id: number; label: string; start: number; end: number; term: string; function?: number | null; static_gas?: number | null }
export interface GraphEdge { from: number; to: number; kind: string }
export interface Graph { nodes: GraphNode[]; edges: GraphEdge[] }
export interface OpLine { pc: number; line: number }
export interface TranspileStats { blocks: number; ops: number; locals: number; c_lines: number; functions?: number | null }
export interface TranspileView { warnings: WarningView[]; graph: Graph; c_source: string; op_map: OpLine[]; stats: TranspileStats }

export interface DisasmLine { pc: number; word: number; text: string; ecall?: string | null }
export interface CheckFinding { addr: number; word: number; rule: string; what: string }
export interface SourceMap { pc_to_c: [number, number][]; c_to_src: [number, number][] }
export interface ImageView { words: number; text_words: number; data_words: number; prologue_words: number; base_pc: number; text_base: number; hc: string; program_id: string; cap_words: number; fits: boolean; findings: CheckFinding[]; disasm: DisasmLine[]; source_map?: SourceMap | null; source_map_note?: string | null }

export type StorageSeed =
  | { kind: 'slot'; slot: string; value: string }
  | { kind: 'mapping'; base: number; key: string; value: string }
  | { kind: 'mapping2'; base: number; key1: string; key2: string; value: string };
export interface EvmCallForm { lang: 'evm'; function: string; args: string[]; caller: string; address?: string | null; value?: string; gas_limit?: number; storage?: StorageSeed[] }
export interface AccountForm { key: string; owner: string; lamports: number; data: string; is_signer?: boolean; is_writable?: boolean; executable?: boolean }
export interface SbpfCallForm { lang: 'sbpf'; program_id: string; accounts: AccountForm[]; data: string }
export type CallForm = EvmCallForm | SbpfCallForm;
export interface RunRequest { call: CallForm; max_cycles?: number | null }

export interface DiscoveryStep { run: number; slot: string; gas_used: number }
export interface TableRows { cpu: number; memory: number; alu: number; poseidon2_permutations: number; keccak_permutations: number; sha256_compressions: number; digest_rows: number }
export interface TierCapacity { tier: number; cpu: number; alu: number; memory_min: number; memory_max: number; poseidon2_permutations: number; keccak_permutations: number; sha256_compressions: number; fits: boolean }
export interface SyscallView { clk: number; kind: string; detail: string }
export interface RunSummary { rid: string; cycles: number; halted: boolean; trap?: string | null; trap_clk?: number | null; outputs: number[]; status: number; tier?: number | null; rows: TableRows; tiers: TierCapacity[]; syscall_counts: [string, number][]; hot_pcs: [number, number][]; input_words: number; public_words: number; discovery?: DiscoveryStep[] }
export interface CycleView { clk: number; pc: number; next_pc: number; word: number; rd: number; writes_rd: boolean; rd_val: number; mem_read: boolean; mem_write: boolean; mem_addr: number; mem_val: number; syscall: boolean; hash_row: boolean; n_alu: number; sys_idx: number; mem_old: number }
/** mem_val: the word a load read or a store wrote; mem_old: the word a store replaced (else = mem_val). */
export interface CyclePage { from: number; cycles: CycleView[]; total: number }
export interface StateView { clk: number; regs: number[]; cycle: CycleView; text: string; c_line?: number | null; src_pc?: number | null }
export interface MemoryView { clk: number; addr: number; words: number[] }

export interface SampleMeta { id: string; title: string; lang: Lang; files: string[]; description?: string; default_call?: CallForm | null; options?: JobOptions | null }
export interface SampleFile { name: string; encoding: 'text' | 'base64'; content: string }
export interface SampleView { meta: SampleMeta; files: SampleFile[] }
export interface FileIn { name: string; encoding?: 'text' | 'base64'; content: string }
export interface NewJob { lang: Lang; files: FileIn[]; options?: JobOptions }

/** What `subscribeJob` delivers. The wire events are `job` (a JobView snapshot, first and again after
 *  a lag), `stage` ({type:"stage",stage,state}), `console` ({type:"console",stage,stream,text}), `done`. */
export type JobStreamEvent =
  | { type: 'job'; job: JobView }
  | { type: 'stage'; stage: StageName; state: StageState }
  | { type: 'console'; line: ConsoleLine }
  | { type: 'done' }
  | { type: 'error'; message: string };
