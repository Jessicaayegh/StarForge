use crate::utils::{config, gas_analyzer};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const REGRESSION_THRESHOLD_PCT: f64 = 10.0;
const WASM_PAGE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractProfileReport {
    pub id: String,
    pub generated_at: String,
    pub contract_label: String,
    pub wasm_path: String,
    pub wasm_sha256: String,
    pub size_bytes: usize,
    pub execution: ExecutionTimeProfile,
    pub memory: MemoryUsageProfile,
    pub bottlenecks: Vec<ProfileBottleneck>,
    pub regression: Option<ProfileRegression>,
    pub comparison: Option<ProfileComparison>,
    pub optimization_score: u8,
    pub dashboard_summary: DashboardSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTimeProfile {
    pub analysis_wall_time_ms: u128,
    pub estimated_instruction_count: usize,
    pub estimated_cpu_gas: u64,
    pub estimated_invocation_time_ms: f64,
    pub hot_sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsageProfile {
    pub linear_memory_pages: usize,
    pub estimated_static_bytes: usize,
    pub estimated_peak_bytes: usize,
    pub code_section_bytes: usize,
    pub custom_section_bytes: usize,
    pub data_segment_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProfileSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileBottleneck {
    pub kind: String,
    pub severity: ProfileSeverity,
    pub metric: String,
    pub description: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileRegression {
    pub baseline_id: String,
    pub gas_delta_pct: f64,
    pub execution_time_delta_pct: f64,
    pub memory_delta_pct: f64,
    pub regression_detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileComparison {
    pub baseline_id: String,
    pub candidate_id: String,
    pub gas_delta: i64,
    pub gas_delta_pct: f64,
    pub instruction_delta: i64,
    pub execution_time_delta_pct: f64,
    pub memory_delta: i64,
    pub memory_delta_pct: f64,
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub total_estimated_gas: u64,
    pub estimated_invocation_time_ms: f64,
    pub estimated_peak_memory_bytes: usize,
    pub bottleneck_count: usize,
    pub regression_detected: bool,
    pub next_actions: Vec<String>,
}

pub fn profile_contract_wasm(
    wasm_path: &Path,
    label: Option<&str>,
    baseline_path: Option<&Path>,
) -> Result<ContractProfileReport> {
    let started = Instant::now();
    let gas_report = gas_analyzer::analyze_wasm_file(wasm_path, label)?;
    let analysis_wall_time_ms = started.elapsed().as_millis();

    let execution = build_execution_profile(&gas_report, analysis_wall_time_ms);
    let memory = build_memory_profile(&gas_report);
    let mut bottlenecks = identify_bottlenecks(&gas_report, &execution, &memory);

    let baseline = match baseline_path {
        Some(path) => Some(load_profile_report(path)?),
        None => None,
    };

    let comparison = baseline
        .as_ref()
        .map(|base| compare_profile_reports(base, &gas_report, &execution, &memory));
    let regression = comparison.as_ref().map(|cmp| ProfileRegression {
        baseline_id: cmp.baseline_id.clone(),
        gas_delta_pct: cmp.gas_delta_pct,
        execution_time_delta_pct: cmp.execution_time_delta_pct,
        memory_delta_pct: cmp.memory_delta_pct,
        regression_detected: is_regression(cmp),
    });

    if regression
        .as_ref()
        .map(|r| r.regression_detected)
        .unwrap_or(false)
    {
        bottlenecks.push(ProfileBottleneck {
            kind: "regression".to_string(),
            severity: ProfileSeverity::High,
            metric: "baseline-comparison".to_string(),
            description: "Candidate profile regressed beyond the configured threshold.".to_string(),
            recommendation: "Inspect gas, instruction, and memory deltas before merging."
                .to_string(),
        });
    }

    let dashboard_summary = build_dashboard_summary(
        gas_report.gas_cost.total,
        execution.estimated_invocation_time_ms,
        memory.estimated_peak_bytes,
        &bottlenecks,
        regression
            .as_ref()
            .map(|r| r.regression_detected)
            .unwrap_or(false),
    );

    Ok(ContractProfileReport {
        id: format!("profile-{}", &gas_report.wasm_sha256[..12]),
        generated_at: Utc::now().to_rfc3339(),
        contract_label: gas_report.contract_label,
        wasm_path: gas_report.wasm_path,
        wasm_sha256: gas_report.wasm_sha256,
        size_bytes: gas_report.size_bytes,
        execution,
        memory,
        bottlenecks,
        regression,
        comparison,
        optimization_score: gas_report.optimization_score,
        dashboard_summary,
    })
}

fn build_execution_profile(
    gas_report: &gas_analyzer::GasAnalysisReport,
    analysis_wall_time_ms: u128,
) -> ExecutionTimeProfile {
    let section = &gas_report.section_profile;
    let estimated_invocation_time_ms =
        section.estimated_instruction_count as f64 * 0.002 + section.import_count as f64 * 0.05;
    let mut hot_sections = Vec::new();

    if section.code_section_bytes > 48 * 1024 {
        hot_sections.push("code-section".to_string());
    }
    if section.import_count > 20 {
        hot_sections.push("host-imports".to_string());
    }
    if section.estimated_instruction_count > 25_000 {
        hot_sections.push("instruction-density".to_string());
    }

    ExecutionTimeProfile {
        analysis_wall_time_ms,
        estimated_instruction_count: section.estimated_instruction_count,
        estimated_cpu_gas: gas_report.gas_cost.cpu_cost,
        estimated_invocation_time_ms,
        hot_sections,
    }
}

fn build_memory_profile(gas_report: &gas_analyzer::GasAnalysisReport) -> MemoryUsageProfile {
    let section = &gas_report.section_profile;
    let linear_memory_pages = section.memory_count.max(1);
    let estimated_static_bytes = section
        .code_section_bytes
        .saturating_add(section.custom_section_bytes)
        .saturating_add(section.data_segment_count.saturating_mul(1024));
    let estimated_peak_bytes = estimated_static_bytes
        .saturating_add(linear_memory_pages.saturating_mul(WASM_PAGE_BYTES))
        .saturating_add(section.estimated_instruction_count.saturating_mul(4))
        .saturating_add(section.import_count.saturating_mul(256));

    MemoryUsageProfile {
        linear_memory_pages,
        estimated_static_bytes,
        estimated_peak_bytes,
        code_section_bytes: section.code_section_bytes,
        custom_section_bytes: section.custom_section_bytes,
        data_segment_count: section.data_segment_count,
    }
}

fn identify_bottlenecks(
    gas_report: &gas_analyzer::GasAnalysisReport,
    execution: &ExecutionTimeProfile,
    memory: &MemoryUsageProfile,
) -> Vec<ProfileBottleneck> {
    let mut bottlenecks = Vec::new();

    if execution.estimated_invocation_time_ms > 50.0 {
        bottlenecks.push(ProfileBottleneck {
            kind: "execution-time".to_string(),
            severity: ProfileSeverity::High,
            metric: format!("{:.2}ms", execution.estimated_invocation_time_ms),
            description: "Estimated invocation time is high for a Soroban contract.".to_string(),
            recommendation:
                "Reduce nested loops, split large entrypoints, or move bulk work off-chain."
                    .to_string(),
        });
    }

    if memory.estimated_peak_bytes > 512 * 1024 {
        bottlenecks.push(ProfileBottleneck {
            kind: "memory-pressure".to_string(),
            severity: ProfileSeverity::High,
            metric: format!("{} bytes", memory.estimated_peak_bytes),
            description: "Estimated peak memory pressure is elevated.".to_string(),
            recommendation:
                "Reduce static data, remove debug sections, and avoid large temporary collections."
                    .to_string(),
        });
    }

    if gas_report.gas_cost.total > 1_500_000 {
        bottlenecks.push(ProfileBottleneck {
            kind: "gas-cost".to_string(),
            severity: ProfileSeverity::Medium,
            metric: format!("{} gas", gas_report.gas_cost.total),
            description: "Estimated gas cost is above the profiling budget.".to_string(),
            recommendation: "Prioritize findings with the highest estimated gas saving."
                .to_string(),
        });
    }

    for finding in &gas_report.findings {
        if matches!(
            finding.severity,
            gas_analyzer::FindingSeverity::Critical
                | gas_analyzer::FindingSeverity::High
                | gas_analyzer::FindingSeverity::Medium
        ) {
            bottlenecks.push(ProfileBottleneck {
                kind: finding.kind.clone(),
                severity: map_severity(&finding.severity),
                metric: finding.id.clone(),
                description: finding.description.clone(),
                recommendation: finding.recommendation.clone(),
            });
        }
    }

    bottlenecks
}

fn compare_profile_reports(
    baseline: &ContractProfileReport,
    gas_report: &gas_analyzer::GasAnalysisReport,
    execution: &ExecutionTimeProfile,
    memory: &MemoryUsageProfile,
) -> ProfileComparison {
    let current_gas = gas_report.gas_cost.total;
    let baseline_gas = baseline.dashboard_summary.total_estimated_gas;
    let gas_delta = current_gas as i64 - baseline_gas as i64;
    let gas_delta_pct = percent_delta(current_gas as f64, baseline_gas as f64);

    let current_time = execution.estimated_invocation_time_ms;
    let baseline_time = baseline.dashboard_summary.estimated_invocation_time_ms;
    let execution_time_delta_pct = percent_delta(current_time, baseline_time);

    let current_memory = memory.estimated_peak_bytes;
    let baseline_memory = baseline.dashboard_summary.estimated_peak_memory_bytes;
    let memory_delta = current_memory as i64 - baseline_memory as i64;
    let memory_delta_pct = percent_delta(current_memory as f64, baseline_memory as f64);

    let instruction_delta = execution.estimated_instruction_count as i64
        - baseline.execution.estimated_instruction_count as i64;

    let verdict = if gas_delta_pct > REGRESSION_THRESHOLD_PCT
        || execution_time_delta_pct > REGRESSION_THRESHOLD_PCT
        || memory_delta_pct > REGRESSION_THRESHOLD_PCT
    {
        "regressed".to_string()
    } else if gas_delta_pct < -REGRESSION_THRESHOLD_PCT
        || execution_time_delta_pct < -REGRESSION_THRESHOLD_PCT
        || memory_delta_pct < -REGRESSION_THRESHOLD_PCT
    {
        "improved".to_string()
    } else {
        "unchanged".to_string()
    };

    ProfileComparison {
        baseline_id: baseline.id.clone(),
        candidate_id: format!("profile-{}", &gas_report.wasm_sha256[..12]),
        gas_delta,
        gas_delta_pct,
        instruction_delta,
        execution_time_delta_pct,
        memory_delta,
        memory_delta_pct,
        verdict,
    }
}

fn is_regression(comparison: &ProfileComparison) -> bool {
    comparison.gas_delta_pct > REGRESSION_THRESHOLD_PCT
        || comparison.execution_time_delta_pct > REGRESSION_THRESHOLD_PCT
        || comparison.memory_delta_pct > REGRESSION_THRESHOLD_PCT
}

fn build_dashboard_summary(
    total_estimated_gas: u64,
    estimated_invocation_time_ms: f64,
    estimated_peak_memory_bytes: usize,
    bottlenecks: &[ProfileBottleneck],
    regression_detected: bool,
) -> DashboardSummary {
    let mut next_actions = Vec::new();
    if regression_detected {
        next_actions.push("Review baseline comparison before merging.".to_string());
    }
    if bottlenecks.is_empty() {
        next_actions
            .push("No bottlenecks detected; keep this report as the next baseline.".to_string());
    } else {
        next_actions.push("Fix high-severity bottlenecks before deployment.".to_string());
        next_actions.push("Re-run profiling and compare against this report.".to_string());
    }

    DashboardSummary {
        total_estimated_gas,
        estimated_invocation_time_ms,
        estimated_peak_memory_bytes,
        bottleneck_count: bottlenecks.len(),
        regression_detected,
        next_actions,
    }
}

fn percent_delta(current: f64, baseline: f64) -> f64 {
    if baseline.abs() < f64::EPSILON {
        0.0
    } else {
        ((current - baseline) / baseline) * 100.0
    }
}

fn map_severity(severity: &gas_analyzer::FindingSeverity) -> ProfileSeverity {
    match severity {
        gas_analyzer::FindingSeverity::Critical => ProfileSeverity::Critical,
        gas_analyzer::FindingSeverity::High => ProfileSeverity::High,
        gas_analyzer::FindingSeverity::Medium => ProfileSeverity::Medium,
        gas_analyzer::FindingSeverity::Low => ProfileSeverity::Low,
        gas_analyzer::FindingSeverity::Info => ProfileSeverity::Info,
    }
}

pub fn save_profile_report(report: &ContractProfileReport) -> Result<PathBuf> {
    let dir = profile_reports_dir()?;
    let path = dir.join(format!("{}.json", report.id));
    fs::write(&path, serde_json::to_string_pretty(report)?)?;
    Ok(path)
}

pub fn load_profile_report(path: &Path) -> Result<ContractProfileReport> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read profile report: {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse profile report: {}", path.display()))
}

pub fn write_profile_report(report: &ContractProfileReport, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(report)?)?;
    Ok(())
}

pub fn write_dashboard_html(report: &ContractProfileReport, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, render_dashboard_html(report))?;
    Ok(())
}

pub fn render_dashboard_html(report: &ContractProfileReport) -> String {
    let bottlenecks = if report.bottlenecks.is_empty() {
        "<li>No bottlenecks detected.</li>".to_string()
    } else {
        report
            .bottlenecks
            .iter()
            .map(|b| {
                format!(
                    "<li><strong>{}</strong> ({:?}, {}): {}<br><span>{}</span></li>",
                    escape_html(&b.kind),
                    b.severity,
                    escape_html(&b.metric),
                    escape_html(&b.description),
                    escape_html(&b.recommendation)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let comparison = report.comparison.as_ref().map_or_else(
        || "<p>No baseline comparison supplied.</p>".to_string(),
        |cmp| {
            format!(
                "<p>Verdict: <strong>{}</strong></p><ul><li>Gas delta: {:+.2}%</li><li>Execution time delta: {:+.2}%</li><li>Memory delta: {:+.2}%</li></ul>",
                escape_html(&cmp.verdict),
                cmp.gas_delta_pct,
                cmp.execution_time_delta_pct,
                cmp.memory_delta_pct
            )
        },
    );

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>StarForge Contract Profile - {label}</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 2rem; color: #18212f; background: #f7f8fb; }}
    main {{ max-width: 960px; margin: 0 auto; }}
    section {{ background: #fff; border: 1px solid #d9dee8; border-radius: 8px; padding: 1rem 1.25rem; margin: 1rem 0; }}
    dl {{ display: grid; grid-template-columns: 220px 1fr; gap: .5rem 1rem; }}
    dt {{ color: #667085; }}
    dd {{ margin: 0; font-weight: 600; }}
    li {{ margin: .6rem 0; }}
    span {{ color: #475467; }}
  </style>
</head>
<body>
<main>
  <h1>Contract Performance Profile</h1>
  <section>
    <h2>Summary</h2>
    <dl>
      <dt>Contract</dt><dd>{label}</dd>
      <dt>WASM SHA-256</dt><dd>{sha}</dd>
      <dt>Estimated gas</dt><dd>{gas}</dd>
      <dt>Estimated invocation time</dt><dd>{time:.2} ms</dd>
      <dt>Estimated peak memory</dt><dd>{memory} bytes</dd>
      <dt>Optimization score</dt><dd>{score}/100</dd>
      <dt>Regression detected</dt><dd>{regression}</dd>
    </dl>
  </section>
  <section>
    <h2>Bottlenecks</h2>
    <ul>{bottlenecks}</ul>
  </section>
  <section>
    <h2>Baseline Comparison</h2>
    {comparison}
  </section>
</main>
</body>
</html>"#,
        label = escape_html(&report.contract_label),
        sha = escape_html(&report.wasm_sha256),
        gas = report.dashboard_summary.total_estimated_gas,
        time = report.dashboard_summary.estimated_invocation_time_ms,
        memory = report.dashboard_summary.estimated_peak_memory_bytes,
        score = report.optimization_score,
        regression = report.dashboard_summary.regression_detected,
        bottlenecks = bottlenecks,
        comparison = comparison
    )
}

fn profile_reports_dir() -> Result<PathBuf> {
    let dir = config::config_dir().join("contract_profiles");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn minimal_wasm() -> Vec<u8> {
        b"\0asm\x01\0\0\0".to_vec()
    }

    fn wasm_with_memory_and_code(code_bytes: usize) -> Vec<u8> {
        let mut wasm = minimal_wasm();
        wasm.extend_from_slice(&[5, 3, 1, 0, 1]);
        wasm.push(10);
        wasm.push((code_bytes + 1) as u8);
        wasm.push(1);
        wasm.extend(std::iter::repeat(0x20).take(code_bytes));
        wasm
    }

    #[test]
    fn profiles_valid_wasm_with_execution_and_memory_metrics() {
        let tmp = tempdir().unwrap();
        let wasm = tmp.path().join("contract.wasm");
        fs::write(&wasm, wasm_with_memory_and_code(32)).unwrap();

        let report = profile_contract_wasm(&wasm, Some("demo"), None).unwrap();

        assert_eq!(report.contract_label, "demo");
        assert!(report.execution.estimated_instruction_count > 0);
        assert!(report.memory.estimated_peak_bytes >= WASM_PAGE_BYTES);
        assert!(!report.dashboard_summary.regression_detected);
    }

    #[test]
    fn detects_regression_against_baseline_report() {
        let tmp = tempdir().unwrap();
        let base_wasm = tmp.path().join("base.wasm");
        let candidate_wasm = tmp.path().join("candidate.wasm");
        let baseline_json = tmp.path().join("baseline.json");
        fs::write(&base_wasm, wasm_with_memory_and_code(8)).unwrap();
        fs::write(&candidate_wasm, wasm_with_memory_and_code(120)).unwrap();

        let baseline = profile_contract_wasm(&base_wasm, Some("base"), None).unwrap();
        write_profile_report(&baseline, &baseline_json).unwrap();

        let candidate =
            profile_contract_wasm(&candidate_wasm, Some("candidate"), Some(&baseline_json))
                .unwrap();

        assert!(candidate.dashboard_summary.regression_detected);
        assert_eq!(
            candidate.regression.as_ref().unwrap().baseline_id,
            baseline.id
        );
    }

    #[test]
    fn dashboard_html_contains_key_sections() {
        let tmp = tempdir().unwrap();
        let wasm = tmp.path().join("contract.wasm");
        fs::write(&wasm, minimal_wasm()).unwrap();
        let report = profile_contract_wasm(&wasm, Some("html-demo"), None).unwrap();

        let html = render_dashboard_html(&report);

        assert!(html.contains("Contract Performance Profile"));
        assert!(html.contains("html-demo"));
        assert!(html.contains("Baseline Comparison"));
    }
}
