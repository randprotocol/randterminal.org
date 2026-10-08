//! Every JSON shape the API serves. `web/lib/types.ts` mirrors this file one to one; change both.
use serde::{Deserialize, Serialize};

// ---- jobs -------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    Solidity,
    SolanaRust,
    SolanaElf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StageName {
    Compile,
    Transpile,
    Build,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobOptions {
    /// The contract to transpile when the Solidity file defines several.
    #[serde(default)]
    pub contract: Option<String>,
    /// CHAINID's value; the fleet chain by default.
    #[serde(default = "default_chain_id")]
    pub chain_id: u64,
    /// `1` or `2`; two is the default and what ships.
    #[serde(default = "default_stage")]
    pub stage: u8,
}
impl Default for JobOptions {
    fn default() -> Self {
        Self {
            contract: None,
            chain_id: default_chain_id(),
            stage: default_stage(),
        }
    }
}
pub fn default_chain_id() -> u64 {
    20
}
pub fn default_stage() -> u8 {
    2
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub severity: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageError {
    /// `compile` | `transpile` | `build` | `timeout` | `unsupported`
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageState {
    pub status: StageStatus,
    #[serde(default)]
    pub started: Option<u64>,
    #[serde(default)]
    pub ended: Option<u64>,
    #[serde(default)]
    pub error: Option<StageError>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsoleLine {
    pub stage: StageName,
    pub stream: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobView {
    pub id: String,
    pub lang: Lang,
    pub options: JobOptions,
    pub created: u64,
    pub compile: StageState,
    pub transpile: StageState,
    pub build: StageState,
    pub console: Vec<ConsoleLine>,
    /// True once the pipeline has stopped, whether every stage is done or one failed (the
    /// stages after a failure stay `queued`).
    #[serde(default)]
    pub finished: bool,
}

// ---- compile ----------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvmOpcode {
    pub pc: usize,
    pub opcode: u8,
    pub mnemonic: String,
    #[serde(default)]
    pub push_hex: Option<String>,
    pub static_gas: u64,
    pub jumpdest: bool,
    pub block: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvmCompileView {
    pub contract: String,
    pub contracts: Vec<String>,
    pub creation_hex: String,
    pub runtime_hex: String,
    pub size_bytes: usize,
    pub abi: serde_json::Value,
    pub opcodes: Vec<EvmOpcode>,
    pub blocks: usize,
    pub jumpdests: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SbpfInsnView {
    pub pc: usize,
    pub text: String,
    pub opc: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SbpfBlockView {
    pub start: usize,
    pub end: usize,
    pub term: String,
    pub insns: Vec<SbpfInsnView>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SbpfFunctionView {
    pub entry: usize,
    pub blocks: Vec<SbpfBlockView>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SbpfSyscallView {
    pub hash: u32,
    pub name: String,
    pub count: usize,
    pub supported: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SbpfCompileView {
    pub elf_bytes: usize,
    pub text_bytes: usize,
    pub rodata_bytes: usize,
    pub entry_pc: usize,
    pub functions: Vec<SbpfFunctionView>,
    pub instructions: usize,
    pub syscalls: Vec<SbpfSyscallView>,
    pub warnings: Vec<WarningView>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "lang", rename_all = "kebab-case")]
pub enum CompileView {
    Evm(EvmCompileView),
    Sbpf(SbpfCompileView),
}

// ---- transpile --------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WarningView {
    pub kind: String,
    #[serde(default)]
    pub pc: Option<usize>,
    pub detail: String,
    pub explanation: String,
    pub fatal: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: usize,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub term: String,
    #[serde(default)]
    pub function: Option<usize>,
    #[serde(default)]
    pub static_gas: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// `pc` of a source opcode / sBPF slot ↔ 1-based line in the generated C.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpLine {
    pub pc: usize,
    pub line: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranspileStats {
    pub blocks: usize,
    pub ops: usize,
    pub locals: usize,
    pub c_lines: usize,
    #[serde(default)]
    pub functions: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranspileView {
    pub warnings: Vec<WarningView>,
    pub graph: Graph,
    pub c_source: String,
    pub op_map: Vec<OpLine>,
    pub stats: TranspileStats,
}

// ---- image ------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DisasmLine {
    pub pc: u32,
    pub word: u32,
    pub text: String,
    #[serde(default)]
    pub ecall: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckFinding {
    pub addr: u32,
    pub word: u32,
    pub rule: String,
    pub what: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceMap {
    /// RV32 pc → 1-based C line, sorted by pc.
    pub pc_to_c: Vec<(u32, usize)>,
    /// 1-based C line → source pc (from the transpile op_map), sorted by line.
    pub c_to_src: Vec<(usize, usize)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageView {
    pub words: usize,
    pub text_words: usize,
    pub data_words: usize,
    pub prologue_words: usize,
    pub base_pc: u32,
    pub text_base: u32,
    pub hc: String,
    pub program_id: String,
    pub cap_words: usize,
    pub fits: bool,
    pub findings: Vec<CheckFinding>,
    pub disasm: Vec<DisasmLine>,
    #[serde(default)]
    pub source_map: Option<SourceMap>,
    #[serde(default)]
    pub source_map_note: Option<String>,
}

// ---- runs -------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StorageSeed {
    Slot {
        slot: String,
        value: String,
    },
    Mapping {
        base: u32,
        key: String,
        value: String,
    },
    Mapping2 {
        base: u32,
        key1: String,
        key2: String,
        value: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvmCallForm {
    /// Canonical signature, e.g. `transfer(address,uint256)`.
    pub function: String,
    pub args: Vec<String>,
    pub caller: String,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default = "zero_str")]
    pub value: String,
    #[serde(default = "default_gas")]
    pub gas_limit: u64,
    #[serde(default)]
    pub storage: Vec<StorageSeed>,
}
pub fn zero_str() -> String {
    "0".to_string()
}
pub fn default_gas() -> u64 {
    1_000_000
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountForm {
    pub key: String,
    pub owner: String,
    pub lamports: u64,
    pub data: String,
    #[serde(default)]
    pub is_signer: bool,
    #[serde(default)]
    pub is_writable: bool,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SbpfCallForm {
    pub program_id: String,
    pub accounts: Vec<AccountForm>,
    pub data: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "lang", rename_all = "kebab-case")]
pub enum CallForm {
    Evm(EvmCallForm),
    Sbpf(SbpfCallForm),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRequest {
    pub call: CallForm,
    #[serde(default)]
    pub max_cycles: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryStep {
    pub run: usize,
    pub slot: String,
    /// Gas the host dry run used before halting.
    pub gas_used: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableRows {
    pub cpu: usize,
    pub memory: usize,
    pub alu: usize,
    pub poseidon2_permutations: usize,
    pub keccak_permutations: usize,
    pub sha256_compressions: usize,
    pub digest_rows: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierCapacity {
    pub tier: usize,
    pub cpu: usize,
    pub alu: usize,
    pub memory_min: usize,
    pub memory_max: usize,
    pub poseidon2_permutations: usize,
    pub keccak_permutations: usize,
    pub sha256_compressions: usize,
    pub fits: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyscallView {
    pub clk: u32,
    pub kind: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSummary {
    pub rid: String,
    pub cycles: usize,
    pub halted: bool,
    #[serde(default)]
    pub trap: Option<String>,
    #[serde(default)]
    pub trap_clk: Option<u32>,
    pub outputs: [u32; 8],
    pub status: u32,
    #[serde(default)]
    pub tier: Option<usize>,
    pub rows: TableRows,
    pub tiers: Vec<TierCapacity>,
    pub syscall_counts: Vec<(String, usize)>,
    pub hot_pcs: Vec<(u32, usize)>,
    pub input_words: usize,
    pub public_words: usize,
    #[serde(default)]
    pub discovery: Vec<DiscoveryStep>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleView {
    pub clk: u32,
    pub pc: u32,
    pub next_pc: u32,
    pub word: u32,
    pub rd: u8,
    pub writes_rd: bool,
    pub rd_val: u32,
    pub mem_read: bool,
    pub mem_write: bool,
    pub mem_addr: u32,
    pub mem_val: u32,
    pub syscall: bool,
    pub hash_row: bool,
    pub n_alu: u16,
    pub sys_idx: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CyclePage {
    pub from: u32,
    pub cycles: Vec<CycleView>,
    pub total: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateView {
    pub clk: u32,
    pub regs: [u32; 32],
    pub cycle: CycleView,
    pub text: String,
    #[serde(default)]
    pub c_line: Option<usize>,
    #[serde(default)]
    pub src_pc: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryView {
    pub clk: u32,
    pub addr: u32,
    pub words: Vec<u32>,
}

// ---- samples ----------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleMeta {
    pub id: String,
    pub title: String,
    pub lang: Lang,
    pub files: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default_call: Option<CallForm>,
    #[serde(default)]
    pub options: Option<JobOptions>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleView {
    pub meta: SampleMeta,
    pub files: Vec<SampleFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleFile {
    pub name: String,
    /// `text` or `base64`.
    pub encoding: String,
    pub content: String,
}

// ---- requests ---------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileIn {
    pub name: String,
    #[serde(default = "text_enc")]
    pub encoding: String,
    pub content: String,
}
pub fn text_enc() -> String {
    "text".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewJob {
    pub lang: Lang,
    pub files: Vec<FileIn>,
    #[serde(default)]
    pub options: JobOptions,
}
