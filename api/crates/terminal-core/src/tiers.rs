//! How a run fills the STARK tables and which tier it lands in. Mirrors `Machine::prove_salted_with`
//! and `rand-guest run` term for term; the numbers here are what the tables panel draws.
use crate::views::{TableRows, TierCapacity};
use rand_zkvm::emulator::{Execution, HashRow};
use rand_zkvm::hash::{input_digest_row_count, public_digest_row_count};
use rand_zkvm::isa::Program;
use rand_zkvm::machine::{Tier, MAX_MEM_LOG_HEIGHT, TIERS};
use rand_zkvm::tables::{keccak, poseidon2, sha256};

pub fn rows(program: &Program, n_in: usize, n_pub: usize, exec: &Execution) -> TableRows {
    let digest_rows = program.digest_rows() + input_digest_row_count(n_in) + public_digest_row_count(n_pub);
    let absorb = exec.events.iter().filter(|e| matches!(e.hash_row, Some(HashRow::Absorb { .. }))).count();
    TableRows {
        cpu: exec.cycles(),
        memory: exec.events.iter().map(|e| e.accesses.len() + e.keccak_accesses.len() + e.sha256_accesses.len()).sum(),
        alu: exec.events.iter().map(|e| e.alu.len()).sum(),
        poseidon2_permutations: digest_rows + absorb,
        keccak_permutations: exec.events.iter().filter(|e| e.keccak_row.is_some()).count(),
        sha256_compressions: exec.events.iter().filter(|e| e.sha256_row.is_some()).count(),
        digest_rows,
    }
}

pub fn cycles_total(rows: &TableRows) -> usize { rows.cpu + rows.digest_rows }

pub fn pick(rows: &TableRows, cycles_total: usize) -> Option<usize> {
    Tier::for_workload(cycles_total, rows.poseidon2_permutations).map(|t| t.0)
}

pub fn capacities(rows: &TableRows, cycles_total: usize) -> Vec<TierCapacity> {
    TIERS.iter().map(|&t| {
        let tier = Tier(t);
        let poseidon2_permutations = tier.poseidon2_height() / poseidon2::BLOCK;
        let keccak_permutations = (1usize << tier.max_keccak_log_height().min(keccak::MAX_LOG_HEIGHT)) / keccak::BLOCK;
        let sha256_compressions = (1usize << tier.max_sha256_log_height()) / sha256::BLOCK;
        let memory_max = 1usize << MAX_MEM_LOG_HEIGHT;
        let fits = cycles_total <= tier.max_cycles()
            && rows.poseidon2_permutations <= poseidon2_permutations
            && rows.alu <= tier.alu_height()
            && rows.memory < memory_max
            && rows.keccak_permutations <= keccak_permutations
            && rows.sha256_compressions <= sha256_compressions;
        TierCapacity {
            tier: t, cpu: tier.cpu_height(), alu: tier.alu_height(),
            memory_min: 1usize << tier.min_mem_log_height(), memory_max,
            poseidon2_permutations, keccak_permutations, sha256_compressions, fits,
        }
    }).collect()
}
