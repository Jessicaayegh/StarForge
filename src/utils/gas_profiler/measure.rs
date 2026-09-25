//! Execution profiling: turn a real invocation's resource accounting into a
//! [`MeasuredInvocation`].
//!
//! Sources, in order of preference:
//! * a saved `simulateTransaction` response (full JSON-RPC envelope or bare
//!   `result`), e.g. from `stellar contract invoke --sim-only` or
//!   `starforge simulate --output`;
//! * a live simulation against Soroban RPC (see `starforge gas profile
//!   --contract-id`);
//! * a hand-written [`ResourceUsage`] JSON document, useful for budgets and
//!   for numbers taken from `stellar contract invoke --cost` or test
//!   `env.cost_estimate()` output.

use super::fees::{
    compute_resource_fee, FeeBreakdown, LimitUtilization, NetworkFeeConfig, ResourceUsage,
};
use crate::utils::simulation_resources::{self, SimulationResources};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use stellar_xdr::curr::{ContractEventType, DiagnosticEvent, Limits, ReadXdr, WriteXdr};

/// Rough size of a Soroban invoke transaction envelope excluding
/// `transactionData` (source account, sequence, fee, one operation with a
/// short argument list, one signature). Only used when the real size is not
/// known.
pub const ESTIMATED_INVOKE_ENVELOPE_BYTES: u64 = 300;

/// One profiled contract invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredInvocation {
    /// Contract function the measurement belongs to.
    pub function: String,
    /// Where the numbers came from (file path or `rpc:<contract>`).
    pub source: String,
    pub usage: ResourceUsage,
    /// `minResourceFee` reported by simulation: the authoritative total.
    pub reported_min_resource_fee: Option<u64>,
    /// Per-resource attribution computed with the fee schedule.
    pub model_fee: FeeBreakdown,
    pub utilization: LimitUtilization,
    pub requires_restore: bool,
    pub restore_fee_stroops: Option<u64>,
    pub warnings: Vec<String>,
}

impl MeasuredInvocation {
    /// The fee to report: the simulated minimum when available, otherwise
    /// the model total.
    pub fn effective_fee(&self) -> u64 {
        self.reported_min_resource_fee
            .unwrap_or(self.model_fee.total)
    }
}

/// Build a measurement from already-parsed resources.
pub fn from_simulation_resources(
    function: &str,
    source: &str,
    res: &SimulationResources,
    events_bytes: u64,
    cfg: &NetworkFeeConfig,
) -> MeasuredInvocation {
    let mut warnings = res.warnings.clone();
    let fp = res.footprint.clone().unwrap_or_default();
    if res.footprint.is_none() {
        warnings.push(
            "simulation carried no transactionData; ledger footprint is unknown (counted as 0)"
                .to_string(),
        );
    }
    let instructions = res
        .cpu_instructions
        .unwrap_or_else(|| u64::from(fp.instructions));
    let tx_data_bytes = res
        .footprint
        .as_ref()
        .map(|_| estimate_tx_data_size(&fp))
        .unwrap_or(0);
    let usage = ResourceUsage {
        instructions,
        memory_bytes: res.memory_bytes,
        read_entries: fp.read_only_entries as u64,
        write_entries: fp.read_write_entries as u64,
        read_bytes: u64::from(fp.read_bytes),
        write_bytes: u64::from(fp.write_bytes),
        contract_events_bytes: events_bytes,
        transaction_size_bytes: ESTIMATED_INVOKE_ENVELOPE_BYTES + tx_data_bytes,
    };
    warnings.push(
        "transaction size is estimated; bandwidth/history attribution is approximate".to_string(),
    );
    build(
        function,
        source,
        usage,
        Some(res.min_resource_fee_stroops),
        res.restore_fee_stroops,
        warnings,
        cfg,
    )
}

/// Rough XDR size of `SorobanTransactionData` for a footprint (32-byte-ish
/// keys are typical for contract data; instance/code keys are larger).
fn estimate_tx_data_size(fp: &simulation_resources::SimulationFootprint) -> u64 {
    40 + (fp.total_entries() as u64) * 80
}

/// Build a measurement from a hand-written usage document.
pub fn from_usage(
    function: &str,
    source: &str,
    usage: ResourceUsage,
    cfg: &NetworkFeeConfig,
) -> MeasuredInvocation {
    build(function, source, usage, None, None, Vec::new(), cfg)
}

fn build(
    function: &str,
    source: &str,
    usage: ResourceUsage,
    reported: Option<u64>,
    restore: Option<u64>,
    warnings: Vec<String>,
    cfg: &NetworkFeeConfig,
) -> MeasuredInvocation {
    let model_fee = compute_resource_fee(&usage, &cfg.fees);
    let utilization = LimitUtilization::compute(&usage, &cfg.limits);
    MeasuredInvocation {
        function: function.to_string(),
        source: source.to_string(),
        usage,
        reported_min_resource_fee: reported,
        model_fee,
        utilization,
        requires_restore: restore.is_some(),
        restore_fee_stroops: restore,
        warnings,
    }
}

/// Sum the XDR size of contract and system events in a simulation's
/// `events` array (base64 `DiagnosticEvent`s). Diagnostic-only events are not
/// charged and are skipped. Undecodable entries are ignored.
pub fn contract_events_bytes(events: &[Value]) -> u64 {
    events
        .iter()
        .filter_map(|v| v.as_str())
        .filter_map(|s| BASE64.decode(s).ok())
        .filter_map(|b| DiagnosticEvent::from_xdr(b, Limits::none()).ok())
        .filter(|d| d.in_successful_contract_call)
        .filter(|d| {
            matches!(
                d.event.type_,
                ContractEventType::Contract | ContractEventType::System
            )
        })
        .filter_map(|d| d.event.to_xdr(Limits::none()).ok())
        .map(|b| b.len() as u64)
        .sum()
}

fn events_array(value: &Value) -> Vec<Value> {
    let result = value.get("result").unwrap_or(value);
    result
        .get("events")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Parse a measurement document: a `simulateTransaction` response or a
/// [`ResourceUsage`] JSON object (detected by a top-level `instructions`).
pub fn parse_measurement(
    function: &str,
    source: &str,
    raw: &str,
    cfg: &NetworkFeeConfig,
) -> Result<MeasuredInvocation> {
    let value: Value = serde_json::from_str(raw)
        .with_context(|| format!("{} is not valid JSON", source))?;
    if value.get("instructions").is_some() {
        let usage: ResourceUsage = serde_json::from_value(value)
            .with_context(|| format!("{} is not a valid resource usage document", source))?;
        return Ok(from_usage(function, source, usage, cfg));
    }
    let res = simulation_resources::parse_simulation_resources(&value)
        .map_err(|e| anyhow!("{}: {}", source, e))?;
    let events = contract_events_bytes(&events_array(&value));
    Ok(from_simulation_resources(
        function, source, &res, events, cfg,
    ))
}

/// Read and parse a measurement file.
pub fn load_measurement(
    function: &str,
    path: &Path,
    cfg: &NetworkFeeConfig,
) -> Result<MeasuredInvocation> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("Cannot read measurement file {}", path.display()))?;
    if meta.len() as usize > simulation_resources::MAX_RESPONSE_BYTES {
        anyhow::bail!(
            "{} is {} bytes, above the {} byte limit",
            path.display(),
            meta.len(),
            simulation_resources::MAX_RESPONSE_BYTES
        );
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read measurement file {}", path.display()))?;
    parse_measurement(function, &path.display().to_string(), &raw, cfg)
}

/// Parse a `NAME=PATH` measurement spec. Without `=`, the file stem is used
/// as the function name.
pub fn parse_sim_spec(spec: &str) -> Result<(String, std::path::PathBuf)> {
    if let Some((name, path)) = spec.split_once('=') {
        if name.trim().is_empty() || path.trim().is_empty() {
            anyhow::bail!("invalid --sim '{}': expected FUNCTION=PATH", spec);
        }
        return Ok((name.trim().to_string(), path.trim().into()));
    }
    let path = std::path::PathBuf::from(spec);
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid --sim '{}': expected FUNCTION=PATH", spec))?
        .to_string();
    Ok((name, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usage_document() {
        let cfg = NetworkFeeConfig::default();
        let m = parse_measurement(
            "transfer",
            "inline",
            r#"{"instructions": 2000000, "read_entries": 3, "write_entries": 2,
                "read_bytes": 1500, "write_bytes": 400, "contract_events_bytes": 200,
                "transaction_size_bytes": 600}"#,
            &cfg,
        )
        .unwrap();
        assert_eq!(m.function, "transfer");
        assert_eq!(m.usage.instructions, 2_000_000);
        assert_eq!(m.model_fee.cpu, 5_000);
        assert!(m.reported_min_resource_fee.is_none());
        assert_eq!(m.effective_fee(), m.model_fee.total);
    }

    #[test]
    fn parses_simulation_envelope() {
        let cfg = NetworkFeeConfig::default();
        let raw = r#"{"jsonrpc":"2.0","id":1,"result":{
            "minResourceFee":"58000",
            "cost":{"cpuInsns":"1500000","memBytes":"900000"},
            "latestLedger": 100
        }}"#;
        let m = parse_measurement("mint", "sim.json", raw, &cfg).unwrap();
        assert_eq!(m.reported_min_resource_fee, Some(58_000));
        assert_eq!(m.usage.instructions, 1_500_000);
        assert_eq!(m.usage.memory_bytes, Some(900_000));
        assert_eq!(m.effective_fee(), 58_000);
        assert!(m.warnings.iter().any(|w| w.contains("footprint")));
    }

    #[test]
    fn rejects_garbage() {
        let cfg = NetworkFeeConfig::default();
        assert!(parse_measurement("f", "x", "not json", &cfg).is_err());
        assert!(parse_measurement("f", "x", "{}", &cfg).is_err());
    }

    #[test]
    fn sim_spec_parsing() {
        let (n, p) = parse_sim_spec("transfer=sims/t.json").unwrap();
        assert_eq!(n, "transfer");
        assert_eq!(p, std::path::PathBuf::from("sims/t.json"));
        let (n, _) = parse_sim_spec("sims/mint.json").unwrap();
        assert_eq!(n, "mint");
        assert!(parse_sim_spec("=x").is_err());
    }

    #[test]
    fn undecodable_events_are_ignored() {
        let events = vec![Value::String("!!!".into()), Value::Null];
        assert_eq!(contract_events_bytes(&events), 0);
    }
}
