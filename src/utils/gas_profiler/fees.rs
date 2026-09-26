//! Soroban resource fee model.
//!
//! On Soroban "gas" is not a single number. A transaction declares, and pays
//! for, several independent resources: CPU instructions, ledger entries read
//! and written, bytes read and written, contract event bytes, and the size of
//! the transaction itself (bandwidth + history). Memory is metered and capped
//! but not charged.
//!
//! [`compute_resource_fee`] mirrors `compute_transaction_resource_fee` from
//! `soroban-env-host` 22 (`src/fees.rs`) exactly, including the round-up
//! per-increment arithmetic, so a breakdown computed from simulated resources
//! matches what the network charges for the same inputs. The *rates* are
//! network configuration that changes by validator vote; the defaults here are
//! mainnet-like values and can be overridden with a JSON file
//! (`--fee-config`). When a simulation reports `minResourceFee`, that number
//! is always the authoritative total and is shown alongside the model.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `INSTRUCTIONS_INCREMENT` in soroban-env-host.
pub const INSTRUCTIONS_INCREMENT: u64 = 10_000;
/// `DATA_SIZE_1KB_INCREMENT` in soroban-env-host.
pub const DATA_SIZE_1KB_INCREMENT: u64 = 1_024;
/// `TX_BASE_RESULT_SIZE` in soroban-env-host (history size overhead).
pub const TX_BASE_RESULT_SIZE: u64 = 300;
/// Approximate envelope overhead of an `UploadContractWasm` transaction on top
/// of the Wasm bytes (source account, sequence, signatures, footprint).
pub const UPLOAD_TX_OVERHEAD_BYTES: u64 = 400;
/// Approximate ledger-entry overhead of a `ContractCode` entry on top of the
/// Wasm bytes (key, hash, cost inputs, TTL entry).
pub const CONTRACT_CODE_ENTRY_OVERHEAD_BYTES: u64 = 150;

/// Per-resource fee rates (stroops). Field names follow
/// `soroban_env_host::fees::FeeConfiguration`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeeSchedule {
    /// Fee per 10,000 CPU instructions.
    pub fee_per_instruction_increment: u64,
    /// Fee per ledger entry read (write entries are also charged as reads).
    pub fee_per_read_entry: u64,
    /// Fee per ledger entry written.
    pub fee_per_write_entry: u64,
    /// Fee per 1 KB read from the ledger.
    pub fee_per_read_1kb: u64,
    /// Fee per 1 KB written to the ledger (dynamic on-chain; depends on
    /// bucket list size).
    pub fee_per_write_1kb: u64,
    /// Fee per 1 KB written to history (transaction size + result).
    pub fee_per_historical_1kb: u64,
    /// Fee per 1 KB of contract events (refundable).
    pub fee_per_contract_event_1kb: u64,
    /// Fee per 1 KB of transaction size (bandwidth).
    pub fee_per_transaction_size_1kb: u64,
    /// Where these rates come from, shown in reports.
    pub source: String,
}

impl Default for FeeSchedule {
    fn default() -> Self {
        Self {
            fee_per_instruction_increment: 25,
            fee_per_read_entry: 6_250,
            fee_per_write_entry: 10_000,
            fee_per_read_1kb: 1_786,
            fee_per_write_1kb: 11_800,
            fee_per_historical_1kb: 16_235,
            fee_per_contract_event_1kb: 10_000,
            fee_per_transaction_size_1kb: 1_624,
            source: "built-in mainnet-like defaults (override with --fee-config)".to_string(),
        }
    }
}

/// Per-transaction resource limits (network configuration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TxLimits {
    pub max_instructions: u64,
    pub max_memory_bytes: u64,
    pub max_read_entries: u64,
    pub max_write_entries: u64,
    pub max_read_bytes: u64,
    pub max_write_bytes: u64,
    pub max_contract_events_bytes: u64,
    pub max_tx_size_bytes: u64,
}

impl Default for TxLimits {
    fn default() -> Self {
        Self {
            max_instructions: 100_000_000,
            max_memory_bytes: 40 * 1024 * 1024,
            max_read_entries: 40,
            max_write_entries: 25,
            max_read_bytes: 200 * 1024,
            max_write_bytes: 129 * 1024,
            max_contract_events_bytes: 8_198,
            max_tx_size_bytes: 129 * 1024,
        }
    }
}

/// Fee rates plus limits, loadable from one JSON document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkFeeConfig {
    pub fees: FeeSchedule,
    pub limits: TxLimits,
}

impl NetworkFeeConfig {
    /// Load a config from JSON. Missing fields fall back to the defaults.
    pub fn from_json_file(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read fee config {}", path.display()))?;
        let mut cfg: NetworkFeeConfig = serde_json::from_str(&raw)
            .with_context(|| format!("Invalid fee config JSON in {}", path.display()))?;
        if cfg.fees.source == FeeSchedule::default().source {
            cfg.fees.source = path.display().to_string();
        }
        Ok(cfg)
    }
}

/// Resources consumed (or declared) by one invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourceUsage {
    pub instructions: u64,
    /// Memory high-water mark, when the source reported it.
    pub memory_bytes: Option<u64>,
    pub read_entries: u64,
    pub write_entries: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub contract_events_bytes: u64,
    pub transaction_size_bytes: u64,
}

/// Fee attributed to each resource, in stroops.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeBreakdown {
    pub cpu: u64,
    pub read_entries: u64,
    pub write_entries: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub historical: u64,
    pub bandwidth: u64,
    /// Refundable: charged up front, refunded if unused.
    pub events: u64,
    pub non_refundable_total: u64,
    pub total: u64,
}

impl FeeBreakdown {
    /// `(label, stroops)` for every component, in a stable order.
    pub fn components(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("cpu", self.cpu),
            ("read_entries", self.read_entries),
            ("write_entries", self.write_entries),
            ("read_bytes", self.read_bytes),
            ("write_bytes", self.write_bytes),
            ("historical", self.historical),
            ("bandwidth", self.bandwidth),
            ("events", self.events),
        ]
    }

    /// The component with the largest share, if any fee was charged.
    pub fn dominant(&self) -> Option<(&'static str, u64)> {
        self.components()
            .into_iter()
            .filter(|(_, v)| *v > 0)
            .max_by_key(|(_, v)| *v)
    }
}

fn per_increment(value: u64, rate: u64, increment: u64) -> u64 {
    let product = value.saturating_mul(rate);
    let inc = increment.max(1);
    product / inc + u64::from(product % inc != 0)
}

/// Compute the resource fee for `usage` under `fees`, mirroring
/// `soroban_env_host::fees::compute_transaction_resource_fee`.
pub fn compute_resource_fee(usage: &ResourceUsage, fees: &FeeSchedule) -> FeeBreakdown {
    let cpu = per_increment(
        usage.instructions,
        fees.fee_per_instruction_increment,
        INSTRUCTIONS_INCREMENT,
    );
    let read_entries = fees
        .fee_per_read_entry
        .saturating_mul(usage.read_entries.saturating_add(usage.write_entries));
    let write_entries = fees.fee_per_write_entry.saturating_mul(usage.write_entries);
    let read_bytes = per_increment(
        usage.read_bytes,
        fees.fee_per_read_1kb,
        DATA_SIZE_1KB_INCREMENT,
    );
    let write_bytes = per_increment(
        usage.write_bytes,
        fees.fee_per_write_1kb,
        DATA_SIZE_1KB_INCREMENT,
    );
    let historical = per_increment(
        usage
            .transaction_size_bytes
            .saturating_add(TX_BASE_RESULT_SIZE),
        fees.fee_per_historical_1kb,
        DATA_SIZE_1KB_INCREMENT,
    );
    let bandwidth = per_increment(
        usage.transaction_size_bytes,
        fees.fee_per_transaction_size_1kb,
        DATA_SIZE_1KB_INCREMENT,
    );
    let events = per_increment(
        usage.contract_events_bytes,
        fees.fee_per_contract_event_1kb,
        DATA_SIZE_1KB_INCREMENT,
    );
    let non_refundable_total = cpu
        .saturating_add(read_entries)
        .saturating_add(write_entries)
        .saturating_add(read_bytes)
        .saturating_add(write_bytes)
        .saturating_add(historical)
        .saturating_add(bandwidth);
    FeeBreakdown {
        cpu,
        read_entries,
        write_entries,
        read_bytes,
        write_bytes,
        historical,
        bandwidth,
        events,
        non_refundable_total,
        total: non_refundable_total.saturating_add(events),
    }
}

/// Model the resource fee of uploading `wasm_len` bytes of code: one new
/// `ContractCode` write entry, the code bytes written to the ledger, and the
/// code bytes carried in the transaction envelope (bandwidth + history).
///
/// CPU for parsing the module and the initial rent are excluded, so this is a
/// lower bound. It is, however, exactly proportional to size, which makes it
/// the right yardstick for "how much does stripping N bytes save".
pub fn estimate_upload_fee(wasm_len: u64, fees: &FeeSchedule) -> FeeBreakdown {
    let usage = ResourceUsage {
        instructions: 0,
        memory_bytes: None,
        read_entries: 0,
        write_entries: 1,
        read_bytes: 0,
        write_bytes: wasm_len.saturating_add(CONTRACT_CODE_ENTRY_OVERHEAD_BYTES),
        contract_events_bytes: 0,
        transaction_size_bytes: wasm_len.saturating_add(UPLOAD_TX_OVERHEAD_BYTES),
    };
    compute_resource_fee(&usage, fees)
}

/// Upload-fee savings from removing `removed_bytes` from a `wasm_len` module.
pub fn upload_savings(wasm_len: u64, removed_bytes: u64, fees: &FeeSchedule) -> u64 {
    let before = estimate_upload_fee(wasm_len, fees).total;
    let after = estimate_upload_fee(wasm_len.saturating_sub(removed_bytes), fees).total;
    before.saturating_sub(after)
}

/// Utilization of each capped resource as a percentage of the tx limit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LimitUtilization {
    pub instructions_pct: f64,
    pub memory_pct: Option<f64>,
    pub read_entries_pct: f64,
    pub write_entries_pct: f64,
    pub read_bytes_pct: f64,
    pub write_bytes_pct: f64,
    pub events_pct: f64,
}

impl LimitUtilization {
    pub fn compute(usage: &ResourceUsage, limits: &TxLimits) -> Self {
        fn pct(v: u64, max: u64) -> f64 {
            if max == 0 {
                0.0
            } else {
                v as f64 / max as f64 * 100.0
            }
        }
        Self {
            instructions_pct: pct(usage.instructions, limits.max_instructions),
            memory_pct: usage.memory_bytes.map(|m| pct(m, limits.max_memory_bytes)),
            // Write entries are part of the read footprint too.
            read_entries_pct: pct(
                usage.read_entries + usage.write_entries,
                limits.max_read_entries,
            ),
            write_entries_pct: pct(usage.write_entries, limits.max_write_entries),
            read_bytes_pct: pct(usage.read_bytes, limits.max_read_bytes),
            write_bytes_pct: pct(usage.write_bytes, limits.max_write_bytes),
            events_pct: pct(usage.contract_events_bytes, limits.max_contract_events_bytes),
        }
    }

    /// `(resource, pct)` pairs, highest first.
    pub fn ranked(&self) -> Vec<(&'static str, f64)> {
        let mut v = vec![
            ("instructions", self.instructions_pct),
            ("read_entries", self.read_entries_pct),
            ("write_entries", self.write_entries_pct),
            ("read_bytes", self.read_bytes_pct),
            ("write_bytes", self.write_bytes_pct),
            ("events", self.events_pct),
        ];
        if let Some(m) = self.memory_pct {
            v.push(("memory", m));
        }
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_increment_rounds_up() {
        assert_eq!(per_increment(0, 25, 10_000), 0);
        assert_eq!(per_increment(1, 25, 10_000), 1);
        assert_eq!(per_increment(10_000, 25, 10_000), 25);
        assert_eq!(per_increment(10_001, 25, 10_000), 26);
    }

    #[test]
    fn fee_matches_host_formula() {
        let fees = FeeSchedule::default();
        let usage = ResourceUsage {
            instructions: 1_000_000,
            memory_bytes: Some(1_000),
            read_entries: 2,
            write_entries: 1,
            read_bytes: 2_048,
            write_bytes: 512,
            contract_events_bytes: 100,
            transaction_size_bytes: 700,
        };
        let f = compute_resource_fee(&usage, &fees);
        assert_eq!(f.cpu, 2_500);
        // write entries are charged as reads too: (2 + 1) * 6250
        assert_eq!(f.read_entries, 18_750);
        assert_eq!(f.write_entries, 10_000);
        assert_eq!(f.read_bytes, 3_572);
        assert_eq!(f.write_bytes, 5_900);
        // (700 + 300) * 16235 / 1024, rounded up
        assert_eq!(f.historical, 15_855);
        // 700 * 1624 / 1024 = 1110.15 -> 1111
        assert_eq!(f.bandwidth, 1_111);
        // 100 * 10000 / 1024 = 976.56 -> 977
        assert_eq!(f.events, 977);
        assert_eq!(f.total, f.non_refundable_total + f.events);
        assert_eq!(f.dominant().unwrap().0, "read_entries");
    }

    #[test]
    fn upload_fee_grows_with_size() {
        let fees = FeeSchedule::default();
        let small = estimate_upload_fee(10_000, &fees).total;
        let big = estimate_upload_fee(60_000, &fees).total;
        assert!(big > small);
        assert!(upload_savings(60_000, 50_000, &fees) > 0);
        assert_eq!(upload_savings(60_000, 0, &fees), 0);
    }

    #[test]
    fn utilization_counts_write_entries_as_reads() {
        let usage = ResourceUsage {
            read_entries: 10,
            write_entries: 10,
            ..Default::default()
        };
        let u = LimitUtilization::compute(&usage, &TxLimits::default());
        assert!((u.read_entries_pct - 50.0).abs() < 1e-9);
        assert!((u.write_entries_pct - 40.0).abs() < 1e-9);
        assert_eq!(u.ranked()[0].0, "read_entries");
    }

    #[test]
    fn partial_config_json_uses_defaults() {
        let cfg: NetworkFeeConfig =
            serde_json::from_str(r#"{"fees":{"fee_per_write_1kb":20000}}"#).unwrap();
        assert_eq!(cfg.fees.fee_per_write_1kb, 20_000);
        assert_eq!(cfg.fees.fee_per_read_entry, 6_250);
        assert_eq!(cfg.limits.max_instructions, 100_000_000);
    }
}
