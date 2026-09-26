//! Advanced gas optimization analyzer (issue #349).
//!
//! Soroban "gas" is a vector of resources (CPU instructions, ledger entries
//! and bytes read/written, event bytes, transaction size), each priced by the
//! network fee schedule. This module profiles a contract along both axes that
//! matter:
//!
//! * **Static profile** ([`profile_wasm`]): a structural decode of the Wasm
//!   (see [`wasm`]) that attributes every host call to the contract entry
//!   point that can reach it, with loop context, so expensive operations such
//!   as storage writes inside loops are pinned to a named function.
//! * **Execution profile** ([`measure`]): real resource usage from simulation,
//!   priced per resource with [`fees::compute_resource_fee`] (a mirror of the
//!   host's fee function) and checked against per-transaction limits.
//!
//! On top of both sits an evidence-based suggestion engine ([`suggest`]),
//! report rendering ([`report`]), version comparison ([`compare`]) and a
//! benchmark suite runner ([`bench`]).

pub mod bench;
pub mod compare;
pub mod fees;
pub mod host_fns;
pub mod measure;
pub mod report;
pub mod suggest;
pub mod wasm;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use fees::{FeeBreakdown, NetworkFeeConfig};
use host_fns::HostCategory;
use measure::MeasuredInvocation;
use suggest::Suggestion;
use wasm::WasmModule;

/// Version of the [`GasProfileReport`] JSON schema.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// Multiplier applied to host-call weights for call sites inside loops in the
/// static cost index (a loop body runs at least once and usually many times).
pub const LOOP_WEIGHT_MULTIPLIER: u64 = 10;

/// Static profile of one exported contract function, including everything it
/// can reach through direct calls.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContractFunctionProfile {
    pub name: String,
    pub func_index: u32,
    /// Instructions in the export's own body.
    pub own_instructions: u64,
    /// Defined functions reachable through direct calls (including itself).
    pub reachable_functions: usize,
    /// Instructions across all reachable function bodies (static size, each
    /// function counted once — not a dynamic execution count).
    pub reachable_instructions: u64,
    /// Host call sites reachable from this export, by resource category.
    pub host_calls: BTreeMap<HostCategory, u32>,
    /// Host call sites reachable from this export, by host function name.
    pub host_functions: BTreeMap<String, u32>,
    /// Host call sites that execute inside a loop (directly, or because the
    /// function containing them is called from inside a loop).
    pub host_calls_in_loops: BTreeMap<HostCategory, u32>,
    /// Host function names called inside loops.
    pub host_functions_in_loops: BTreeMap<String, u32>,
    pub loops: u32,
    pub max_loop_depth: u32,
    /// `call_indirect` sites reachable; their targets are not followed, so
    /// host call counts are a lower bound when this is non-zero.
    pub indirect_call_sites: u32,
    pub rejected_features: Vec<String>,
    /// Relative static cost used for ranking (see [`cost_index`]).
    pub cost_index: u64,
}

impl ContractFunctionProfile {
    pub fn sites(&self, cat: HostCategory) -> u32 {
        self.host_calls.get(&cat).copied().unwrap_or(0)
    }

    pub fn loop_sites(&self, cat: HostCategory) -> u32 {
        self.host_calls_in_loops.get(&cat).copied().unwrap_or(0)
    }
}

/// Module-wide facts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub imports: usize,
    pub host_imports: usize,
    pub unknown_imports: Vec<String>,
    pub exported_functions: usize,
    pub defined_functions: usize,
    pub total_instructions: u64,
    pub globals: u32,
    pub data_segments: u32,
    pub data_bytes: usize,
    pub initial_memory_pages: u64,
    pub custom_sections: BTreeMap<String, usize>,
    pub strippable_custom_bytes: usize,
    pub has_start: bool,
    pub has_env_meta: bool,
    pub rejected_features: Vec<String>,
    pub multi_value_types: usize,
    pub undecodable_functions: usize,
}

/// A defined function that is expensive on its own (exported or not).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalHotspot {
    pub func_index: u32,
    pub name: Option<String>,
    pub instructions: u64,
    pub loops: u32,
    pub max_loop_depth: u32,
    pub host_call_sites: u32,
}

/// Full gas profile of a contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GasProfileReport {
    pub schema_version: u32,
    pub generated_at: String,
    pub contract_label: String,
    pub wasm_path: String,
    pub wasm_sha256: String,
    pub size_bytes: usize,
    pub module: ModuleSummary,
    /// Modelled resource fee of uploading this Wasm (lower bound).
    pub upload_fee: FeeBreakdown,
    pub fee_config: NetworkFeeConfig,
    /// Exported functions, most expensive first.
    pub functions: Vec<ContractFunctionProfile>,
    pub hotspots: Vec<InternalHotspot>,
    pub measurements: Vec<MeasuredInvocation>,
    pub suggestions: Vec<Suggestion>,
    /// 0–100, higher is better; derived from suggestion severities.
    pub score: u8,
}

impl GasProfileReport {
    pub fn function(&self, name: &str) -> Option<&ContractFunctionProfile> {
        self.functions.iter().find(|f| f.name == name)
    }

    pub fn has_critical(&self) -> bool {
        self.suggestions
            .iter()
            .any(|s| s.severity == crate::utils::gas_analyzer::FindingSeverity::Critical)
    }

    /// Load a report previously written with `--format json` / `--output`.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read gas report {}", path.display()))?;
        let report: GasProfileReport = serde_json::from_str(&raw).with_context(|| {
            format!(
                "{} is not a gas profile report (expected output of `starforge gas profile --format json`)",
                path.display()
            )
        })?;
        Ok(report)
    }

    /// Attach measurements and re-run the suggestion engine.
    pub fn with_measurements(mut self, measurements: Vec<MeasuredInvocation>) -> Self {
        self.measurements.extend(measurements);
        self.refresh_suggestions();
        self
    }

    fn refresh_suggestions(&mut self) {
        self.suggestions = suggest::suggest(self);
        self.score = suggest::score(&self.suggestions);
    }
}

/// Relative static cost: reachable instructions plus weighted host call
/// sites, with loop sites weighted [`LOOP_WEIGHT_MULTIPLIER`]x. Only
/// meaningful for ranking functions and comparing versions of one contract.
pub fn cost_index(p: &ContractFunctionProfile) -> u64 {
    let mut total = p.reachable_instructions;
    for (cat, n) in &p.host_calls {
        total = total.saturating_add(cat.weight().saturating_mul(u64::from(*n)));
    }
    for (cat, n) in &p.host_calls_in_loops {
        total = total.saturating_add(
            cat.weight()
                .saturating_mul(u64::from(*n))
                .saturating_mul(LOOP_WEIGHT_MULTIPLIER - 1),
        );
    }
    total
}

fn profile_export(m: &WasmModule, name: &str, func_index: u32) -> ContractFunctionProfile {
    let mut p = ContractFunctionProfile {
        name: name.to_string(),
        func_index,
        ..Default::default()
    };
    if let Some(body) = m.body_for_function(func_index) {
        p.own_instructions = body.instruction_count;
    }

    // Reachability (each function once).
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut stack = vec![func_index];
    let mut rejected: BTreeSet<String> = BTreeSet::new();
    while let Some(idx) = stack.pop() {
        if !seen.insert(idx) {
            continue;
        }
        let body = match m.body_for_function(idx) {
            Some(b) => b,
            None => continue,
        };
        p.reachable_instructions += body.instruction_count;
        p.loops += body.loop_count;
        p.max_loop_depth = p.max_loop_depth.max(body.max_loop_depth);
        p.indirect_call_sites += body.call_indirect_count;
        rejected.extend(body.rejected_features.iter().cloned());
        for (imp, n) in &body.host_calls {
            if let Some(i) = m.import_for_function(*imp) {
                *p.host_calls.entry(i.category).or_insert(0) += n;
                *p.host_functions.entry(import_label(i)).or_insert(0) += n;
            }
        }
        stack.extend(body.direct_calls.keys().copied());
    }
    p.reachable_functions = seen.len();
    p.rejected_features = rejected.into_iter().collect();

    // Loop context: find every function that can run inside a loop (called
    // from a loop body, or reachable from such a function), then count each
    // reachable function's host calls exactly once: all of them if the
    // function runs inside a loop, otherwise only its own loop sites.
    let mut visited: HashSet<(u32, bool)> = HashSet::new();
    let mut runs_in_loop: BTreeSet<u32> = BTreeSet::new();
    let mut work = vec![(func_index, false)];
    while let Some((idx, in_loop)) = work.pop() {
        if !visited.insert((idx, in_loop)) {
            continue;
        }
        if in_loop {
            runs_in_loop.insert(idx);
        }
        if let Some(body) = m.body_for_function(idx) {
            for callee in body.direct_calls.keys() {
                let callee_in_loop = in_loop || body.direct_calls_in_loops.contains_key(callee);
                work.push((*callee, callee_in_loop));
            }
        }
    }
    for idx in &seen {
        let body = match m.body_for_function(*idx) {
            Some(b) => b,
            None => continue,
        };
        let source = if runs_in_loop.contains(idx) {
            &body.host_calls
        } else {
            &body.host_calls_in_loops
        };
        for (imp, n) in source {
            if let Some(i) = m.import_for_function(*imp) {
                *p.host_calls_in_loops.entry(i.category).or_insert(0) += n;
                *p.host_functions_in_loops.entry(import_label(i)).or_insert(0) += n;
            }
        }
    }
    p.cost_index = cost_index(&p);
    p
}

fn import_label(i: &wasm::Import) -> String {
    i.host_name
        .clone()
        .unwrap_or_else(|| format!("{}.{}", i.module, i.field))
}

/// Build the static profile from decoded module bytes.
pub fn profile_bytes(
    bytes: &[u8],
    label: &str,
    wasm_path: &str,
    cfg: &NetworkFeeConfig,
) -> Result<GasProfileReport> {
    let m = wasm::decode(bytes)?;
    let sha = hex::encode(Sha256::digest(bytes));

    let mut functions: Vec<ContractFunctionProfile> = m
        .function_exports()
        .map(|e| profile_export(&m, &e.name, e.index))
        .collect();
    functions.sort_by(|a, b| {
        b.cost_index
            .cmp(&a.cost_index)
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut rejected: BTreeSet<String> = BTreeSet::new();
    for f in &m.functions {
        rejected.extend(f.rejected_features.iter().cloned());
    }
    let multi_value_types = m.types.iter().filter(|(_, r)| *r > 1).count();
    if multi_value_types > 0 {
        rejected.insert("multi-value".to_string());
    }

    let summary = ModuleSummary {
        imports: m.imports.len(),
        host_imports: m
            .imports
            .iter()
            .filter(|i| i.kind == 0 && i.host_name.is_some())
            .count(),
        unknown_imports: m
            .imports
            .iter()
            .filter(|i| i.kind == 0 && i.host_name.is_none())
            .map(|i| format!("{}.{}", i.module, i.field))
            .collect(),
        exported_functions: m.function_exports().count(),
        defined_functions: m.functions.len(),
        total_instructions: m.total_instructions(),
        globals: m.global_count,
        data_segments: m.data_segment_count,
        data_bytes: m.data_bytes,
        initial_memory_pages: m.initial_memory_pages,
        custom_sections: m.custom_sections.clone(),
        strippable_custom_bytes: m.strippable_custom_bytes(),
        has_start: m.has_start,
        has_env_meta: m.custom_sections.contains_key("contractenvmetav0"),
        rejected_features: rejected.into_iter().collect(),
        multi_value_types,
        undecodable_functions: m
            .functions
            .iter()
            .filter(|f| f.decode_error.is_some())
            .count(),
    };

    let mut hotspots: Vec<InternalHotspot> = m
        .functions
        .iter()
        .map(|f| InternalHotspot {
            func_index: f.index,
            name: f.name.clone(),
            instructions: f.instruction_count,
            loops: f.loop_count,
            max_loop_depth: f.max_loop_depth,
            host_call_sites: f.host_calls.values().sum(),
        })
        .collect();
    hotspots.sort_by(|a, b| {
        b.instructions
            .cmp(&a.instructions)
            .then_with(|| a.func_index.cmp(&b.func_index))
    });
    hotspots.truncate(10);

    let mut report = GasProfileReport {
        schema_version: REPORT_SCHEMA_VERSION,
        generated_at: Utc::now().to_rfc3339(),
        contract_label: label.to_string(),
        wasm_path: wasm_path.to_string(),
        wasm_sha256: sha,
        size_bytes: bytes.len(),
        module: summary,
        upload_fee: fees::estimate_upload_fee(bytes.len() as u64, &cfg.fees),
        fee_config: cfg.clone(),
        functions,
        hotspots,
        measurements: Vec::new(),
        suggestions: Vec::new(),
        score: 100,
    };
    report.refresh_suggestions();
    Ok(report)
}

/// Profile a Wasm file on disk.
pub fn profile_wasm(
    path: &Path,
    label: Option<&str>,
    cfg: &NetworkFeeConfig,
) -> Result<GasProfileReport> {
    let bytes =
        std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let label = label.map(str::to_string).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("contract")
            .to_string()
    });
    profile_bytes(&bytes, &label, &path.display().to_string(), cfg)
        .with_context(|| format!("Failed to profile {}", path.display()))
}

/// Load either a Wasm file (profiled fresh) or a saved JSON report.
pub fn load_or_profile(path: &Path, cfg: &NetworkFeeConfig) -> Result<GasProfileReport> {
    let head = {
        use std::io::Read;
        let mut f = std::fs::File::open(path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        let mut buf = [0u8; 4];
        let n = f.read(&mut buf)?;
        buf[..n].to_vec()
    };
    if head.as_slice() == b"\0asm" {
        profile_wasm(path, None, cfg)
    } else {
        GasProfileReport::load(path)
    }
}

#[cfg(test)]
mod tests {
    use super::wasm::builder::{op, ModuleBuilder};
    use super::*;

    fn cfg() -> NetworkFeeConfig {
        NetworkFeeConfig::default()
    }

    /// `helper` writes storage; `batch` calls `helper` inside a loop;
    /// `get` reads storage once.
    fn fixture() -> Vec<u8> {
        ModuleBuilder::new()
            .import("l", "1") // 0 get_contract_data
            .import("l", "_") // 1 put_contract_data
            .func(None, [op::call(1), op::drop()].concat()) // 2 helper
            .func(
                Some("batch"),
                [
                    op::loop_start(),
                    op::call(2),
                    op::i32_const_zero(),
                    op::br_if(0),
                    op::end(),
                ]
                .concat(),
            ) // 3
            .func(Some("get"), [op::call(0), op::drop()].concat()) // 4
            .custom("contractenvmetav0", vec![0; 4])
            .build()
    }

    #[test]
    fn attributes_host_calls_through_call_graph() {
        let r = profile_bytes(&fixture(), "t", "t.wasm", &cfg()).unwrap();
        let batch = r.function("batch").unwrap();
        assert_eq!(batch.reachable_functions, 2);
        assert_eq!(batch.sites(HostCategory::StorageWrite), 1);
        // helper is called from inside the loop, so its write is a loop site
        assert_eq!(batch.loop_sites(HostCategory::StorageWrite), 1);
        assert_eq!(
            batch.host_functions_in_loops.get("put_contract_data"),
            Some(&1)
        );
        let get = r.function("get").unwrap();
        assert_eq!(get.sites(HostCategory::StorageRead), 1);
        assert_eq!(get.loop_sites(HostCategory::StorageRead), 0);
        // batch ranks above get
        assert_eq!(r.functions[0].name, "batch");
        assert!(r.module.has_env_meta);
    }

    #[test]
    fn cost_index_weights_loops() {
        let r = profile_bytes(&fixture(), "t", "t.wasm", &cfg()).unwrap();
        let batch = r.function("batch").unwrap();
        let expected = batch.reachable_instructions
            + HostCategory::StorageWrite.weight() * LOOP_WEIGHT_MULTIPLIER;
        assert_eq!(batch.cost_index, expected);
    }

    #[test]
    fn report_round_trips_through_json() {
        let r = profile_bytes(&fixture(), "t", "t.wasm", &cfg()).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let back: GasProfileReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn load_or_profile_detects_format() {
        let dir = tempfile::tempdir().unwrap();
        let wasm_path = dir.path().join("c.wasm");
        std::fs::write(&wasm_path, fixture()).unwrap();
        let r = load_or_profile(&wasm_path, &cfg()).unwrap();
        let json_path = dir.path().join("c.json");
        std::fs::write(&json_path, serde_json::to_string(&r).unwrap()).unwrap();
        let back = load_or_profile(&json_path, &cfg()).unwrap();
        assert_eq!(back.wasm_sha256, r.wasm_sha256);
    }
}
