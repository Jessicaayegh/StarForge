//! Contract performance regression testing.
//!
//! Tracks named performance baselines (with an append-only history), compares
//! fresh measurements against them using per-metric thresholds and a
//! statistical noise band, raises alerts, and renders text / Markdown / JSON
//! reports suitable for CI gates and PR comments.
//!
//! Measurements are plain samples per metric (CPU instructions, memory bytes,
//! wall time, fees, ...). They can be loaded from a JSON file produced by any
//! benchmark harness, or collected by timing a command repeatedly with
//! [`measure_command`].

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Current on-disk schema version for baselines.
pub const BASELINE_SCHEMA_VERSION: u32 = 1;

/// Default project-local directory for baselines, so they can be committed and
/// shared by CI and contributors.
pub const DEFAULT_BASELINE_DIR: &str = ".starforge/perf-baselines";

// ── Measurements ──────────────────────────────────────────────────────────────

/// Raw samples collected for one metric in a single run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricSamples {
    pub name: String,
    #[serde(default)]
    pub unit: Option<String>,
    /// Throughput-style metrics set this; costs (CPU, memory, time, fees) do not.
    #[serde(default)]
    pub higher_is_better: bool,
    pub samples: Vec<f64>,
}

/// Accepted measurement file layouts:
///
/// ```json
/// { "metrics": [ { "name": "transfer.cpu_insns", "unit": "insns", "samples": [1, 2] } ] }
/// { "metrics": { "transfer.cpu_insns": [1, 2] } }
/// ```
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MeasurementMetrics {
    List(Vec<MetricSamples>),
    Map(BTreeMap<String, Vec<f64>>),
}

#[derive(Debug, Deserialize)]
struct MeasurementFile {
    metrics: MeasurementMetrics,
}

/// Parse measurements from JSON text.
pub fn parse_measurements(json: &str) -> Result<Vec<MetricSamples>> {
    let file: MeasurementFile =
        serde_json::from_str(json).context("invalid performance measurement JSON")?;
    let metrics = match file.metrics {
        MeasurementMetrics::List(list) => list,
        MeasurementMetrics::Map(map) => map
            .into_iter()
            .map(|(name, samples)| MetricSamples {
                name,
                unit: None,
                higher_is_better: false,
                samples,
            })
            .collect(),
    };
    for metric in &metrics {
        if metric.name.trim().is_empty() {
            anyhow::bail!("performance metric names cannot be empty");
        }
        if metric.samples.is_empty() {
            anyhow::bail!("metric '{}' has no samples", metric.name);
        }
        if metric.samples.iter().any(|value| !value.is_finite()) {
            anyhow::bail!("metric '{}' contains a non-finite sample", metric.name);
        }
    }
    Ok(metrics)
}

/// Load measurements from a JSON file.
pub fn load_measurements(path: &Path) -> Result<Vec<MetricSamples>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read measurements from {}", path.display()))?;
    parse_measurements(&raw)
}

/// Run `command` through the shell `warmup + iterations` times and record the
/// wall-clock time of each measured run as `<label>.wall_time_ms`.
///
/// The command must succeed on every iteration; a failing benchmark is a test
/// failure, not a performance sample.
pub fn measure_command(
    label: &str,
    command: &str,
    iterations: u32,
    warmup: u32,
) -> Result<MetricSamples> {
    if iterations == 0 {
        anyhow::bail!("--iterations must be at least 1");
    }
    let mut samples = Vec::with_capacity(iterations as usize);
    for run in 0..(warmup + iterations) {
        let started = Instant::now();
        let status = shell(command)
            .status()
            .with_context(|| format!("failed to run benchmark command '{}'", command))?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        if !status.success() {
            anyhow::bail!(
                "benchmark command '{}' failed on run {} with {}",
                command,
                run + 1,
                status
            );
        }
        if run >= warmup {
            samples.push(elapsed_ms);
        }
    }
    Ok(MetricSamples {
        name: format!("{}.wall_time_ms", label),
        unit: Some("ms".to_string()),
        higher_is_better: false,
        samples,
    })
}

fn shell(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    }
}

// ── Statistics ────────────────────────────────────────────────────────────────

/// Summary statistics for a metric's samples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricStats {
    pub samples: usize,
    pub mean: f64,
    pub median: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
    pub p95: f64,
}

impl MetricStats {
    pub fn from_samples(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        let mean = sorted.iter().sum::<f64>() / n as f64;
        let variance = if n > 1 {
            sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64
        } else {
            0.0
        };
        let median = if n % 2 == 0 {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        } else {
            sorted[n / 2]
        };
        let p95_index = ((n as f64 * 0.95).ceil() as usize).clamp(1, n) - 1;
        Some(Self {
            samples: n,
            mean,
            median,
            stddev: variance.sqrt(),
            min: sorted[0],
            max: sorted[n - 1],
            p95: sorted[p95_index],
        })
    }
}

// ── Baselines ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselineMetric {
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub higher_is_better: bool,
    pub stats: MetricStats,
}

/// A named snapshot of expected performance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerfBaseline {
    pub schema_version: u32,
    pub name: String,
    pub created_at: String,
    #[serde(default)]
    pub git_commit: Option<String>,
    pub metrics: BTreeMap<String, BaselineMetric>,
}

impl PerfBaseline {
    pub fn from_measurements(name: &str, measurements: &[MetricSamples]) -> Result<Self> {
        validate_baseline_name(name)?;
        let mut metrics = BTreeMap::new();
        for metric in measurements {
            let stats = MetricStats::from_samples(&metric.samples)
                .ok_or_else(|| anyhow::anyhow!("metric '{}' has no samples", metric.name))?;
            metrics.insert(
                metric.name.clone(),
                BaselineMetric {
                    unit: metric.unit.clone(),
                    higher_is_better: metric.higher_is_better,
                    stats,
                },
            );
        }
        Ok(Self {
            schema_version: BASELINE_SCHEMA_VERSION,
            name: name.to_string(),
            created_at: Utc::now().to_rfc3339(),
            git_commit: current_git_commit(),
            metrics,
        })
    }
}

fn validate_baseline_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.starts_with('.');
    if !valid {
        anyhow::bail!(
            "invalid baseline name '{}'; use letters, digits, '-', '_' or '.'",
            name
        );
    }
    Ok(())
}

fn current_git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!commit.is_empty()).then_some(commit)
}

/// Stores baselines as `<name>.json` plus an append-only `<name>.history.jsonl`
/// so the evolution of each metric can be tracked across updates.
#[derive(Debug, Clone)]
pub struct BaselineStore {
    dir: PathBuf,
}

impl BaselineStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn baseline_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.json", name))
    }

    fn history_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.history.jsonl", name))
    }

    /// Write `baseline` as the current version and append it to its history.
    pub fn save(&self, baseline: &PerfBaseline) -> Result<PathBuf> {
        validate_baseline_name(&baseline.name)?;
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("failed to create {}", self.dir.display()))?;
        let path = self.baseline_path(&baseline.name);
        fs::write(&path, serde_json::to_string_pretty(baseline)?)
            .with_context(|| format!("failed to write baseline {}", path.display()))?;

        let history = self.history_path(&baseline.name);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&history)
            .with_context(|| format!("failed to open {}", history.display()))?;
        serde_json::to_writer(&mut file, baseline)?;
        file.write_all(b"\n")?;
        Ok(path)
    }

    pub fn load(&self, name: &str) -> Result<PerfBaseline> {
        validate_baseline_name(name)?;
        let path = self.baseline_path(name);
        let raw = fs::read_to_string(&path).with_context(|| {
            format!(
                "baseline '{}' not found at {}; record one with `starforge perf regression baseline --name {}`",
                name,
                path.display(),
                name
            )
        })?;
        let baseline: PerfBaseline = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse baseline {}", path.display()))?;
        if baseline.schema_version > BASELINE_SCHEMA_VERSION {
            anyhow::bail!(
                "baseline {} uses schema v{}, newer than supported v{}",
                path.display(),
                baseline.schema_version,
                BASELINE_SCHEMA_VERSION
            );
        }
        Ok(baseline)
    }

    /// Every recorded version of a baseline, oldest first. Corrupt lines are skipped.
    pub fn history(&self, name: &str) -> Result<Vec<PerfBaseline>> {
        validate_baseline_name(name)?;
        let path = self.history_path(name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file =
            fs::File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
        let mut versions = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(baseline) = serde_json::from_str::<PerfBaseline>(&line) {
                versions.push(baseline);
            }
        }
        Ok(versions)
    }

    /// Names of all stored baselines, sorted.
    pub fn list(&self) -> Result<Vec<String>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let file_name = entry?.file_name().to_string_lossy().to_string();
            if let Some(name) = file_name.strip_suffix(".json") {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(names)
    }
}

// ── Regression detection ──────────────────────────────────────────────────────

/// Thresholds applied when comparing measurements against a baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegressionPolicy {
    /// Worsening (percent) that produces a warning.
    pub warn_pct: f64,
    /// Worsening (percent) that counts as a regression.
    pub fail_pct: f64,
    /// Changes within this many baseline standard deviations are treated as noise.
    pub noise_sigma: f64,
    /// Per-metric regression thresholds (percent), overriding `fail_pct`.
    #[serde(default)]
    pub metric_fail_pct: BTreeMap<String, f64>,
}

impl Default for RegressionPolicy {
    fn default() -> Self {
        Self {
            warn_pct: 5.0,
            fail_pct: 10.0,
            noise_sigma: 2.0,
            metric_fail_pct: BTreeMap::new(),
        }
    }
}

impl RegressionPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.warn_pct < 0.0 || self.fail_pct < 0.0 || self.noise_sigma < 0.0 {
            anyhow::bail!("regression thresholds must be non-negative");
        }
        if self.warn_pct > self.fail_pct {
            anyhow::bail!(
                "--warn-pct ({}) must not exceed --fail-pct ({})",
                self.warn_pct,
                self.fail_pct
            );
        }
        Ok(())
    }

    /// Parse `metric=percent` overrides.
    pub fn with_metric_overrides(mut self, specs: &[String]) -> Result<Self> {
        for spec in specs {
            let (metric, pct) = spec.split_once('=').ok_or_else(|| {
                anyhow::anyhow!(
                    "invalid metric threshold '{}'; expected metric=percent",
                    spec
                )
            })?;
            let pct: f64 = pct
                .trim()
                .parse()
                .with_context(|| format!("invalid percent in metric threshold '{}'", spec))?;
            if pct < 0.0 {
                anyhow::bail!("metric threshold '{}' must be non-negative", spec);
            }
            self.metric_fail_pct.insert(metric.trim().to_string(), pct);
        }
        Ok(self)
    }

    fn fail_pct_for(&self, metric: &str) -> f64 {
        self.metric_fail_pct
            .get(metric)
            .copied()
            .unwrap_or(self.fail_pct)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    Regressed,
    Warning,
    Missing,
    New,
    Unchanged,
    Improved,
}

impl MetricStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Regressed => "regressed",
            Self::Warning => "warning",
            Self::Missing => "missing",
            Self::New => "new",
            Self::Unchanged => "unchanged",
            Self::Improved => "improved",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricComparison {
    pub metric: String,
    #[serde(default)]
    pub unit: Option<String>,
    pub status: MetricStatus,
    pub baseline_mean: Option<f64>,
    pub current_mean: Option<f64>,
    /// Signed change of the mean relative to the baseline (percent).
    pub change_pct: Option<f64>,
    /// Threshold (percent) that applied to this metric.
    pub fail_threshold_pct: f64,
    /// True when the change is inside the baseline's noise band.
    pub within_noise: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Warn,
    Fail,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

/// Condition that makes `perf regression check` exit non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOn {
    /// Any regression (default).
    Regression,
    /// Regressions and warnings.
    Warning,
    /// Report only; never fail.
    Never,
}

impl FailOn {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "regression" => Ok(Self::Regression),
            "warning" | "warn" => Ok(Self::Warning),
            "never" => Ok(Self::Never),
            other => anyhow::bail!(
                "unsupported --fail-on '{}'; use regression, warning, or never",
                other
            ),
        }
    }

    pub fn should_fail(self, verdict: Verdict) -> bool {
        match self {
            Self::Regression => verdict == Verdict::Fail,
            Self::Warning => verdict != Verdict::Pass,
            Self::Never => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerfAlert {
    pub severity: String,
    pub metric: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComparisonSummary {
    pub regressed: usize,
    pub warnings: usize,
    pub improved: usize,
    pub unchanged: usize,
    pub new: usize,
    pub missing: usize,
}

/// Result of comparing measurements to a baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegressionReport {
    pub baseline: String,
    pub baseline_created_at: String,
    #[serde(default)]
    pub baseline_commit: Option<String>,
    #[serde(default)]
    pub current_commit: Option<String>,
    pub generated_at: String,
    pub policy: RegressionPolicy,
    pub verdict: Verdict,
    pub summary: ComparisonSummary,
    pub alerts: Vec<PerfAlert>,
    pub comparisons: Vec<MetricComparison>,
}

/// Compare `current` measurements against `baseline` under `policy`.
///
/// A metric regresses when its mean worsens by more than its threshold *and*
/// the change exceeds `noise_sigma` baseline standard deviations. Metrics in
/// the baseline but absent from the run are reported as `missing` (a warning,
/// since a silently dropped benchmark hides regressions).
pub fn compare(
    baseline: &PerfBaseline,
    current: &[MetricSamples],
    policy: &RegressionPolicy,
) -> Result<RegressionReport> {
    policy.validate()?;
    let current_by_name: BTreeMap<&str, &MetricSamples> =
        current.iter().map(|m| (m.name.as_str(), m)).collect();
    let names: BTreeSet<&str> = baseline
        .metrics
        .keys()
        .map(String::as_str)
        .chain(current_by_name.keys().copied())
        .collect();

    let mut comparisons = Vec::new();
    for name in names {
        let fail_pct = policy.fail_pct_for(name);
        let base = baseline.metrics.get(name);
        let cur = current_by_name.get(name);
        let comparison = match (base, cur) {
            (Some(base), None) => MetricComparison {
                metric: name.to_string(),
                unit: base.unit.clone(),
                status: MetricStatus::Missing,
                baseline_mean: Some(base.stats.mean),
                current_mean: None,
                change_pct: None,
                fail_threshold_pct: fail_pct,
                within_noise: false,
            },
            (None, Some(cur)) => MetricComparison {
                metric: name.to_string(),
                unit: cur.unit.clone(),
                status: MetricStatus::New,
                baseline_mean: None,
                current_mean: MetricStats::from_samples(&cur.samples).map(|s| s.mean),
                change_pct: None,
                fail_threshold_pct: fail_pct,
                within_noise: false,
            },
            (Some(base), Some(cur)) => {
                let stats = MetricStats::from_samples(&cur.samples)
                    .ok_or_else(|| anyhow::anyhow!("metric '{}' has no samples", name))?;
                classify(name, base, &stats, fail_pct, policy)
            }
            (None, None) => continue,
        };
        comparisons.push(comparison);
    }

    // Worst first, then by name, so reports lead with what needs attention.
    comparisons.sort_by(|a, b| {
        a.status
            .cmp(&b.status)
            .then_with(|| a.metric.cmp(&b.metric))
    });

    let mut summary = ComparisonSummary::default();
    let mut alerts = Vec::new();
    for c in &comparisons {
        match c.status {
            MetricStatus::Regressed => {
                summary.regressed += 1;
                alerts.push(PerfAlert {
                    severity: "critical".to_string(),
                    metric: c.metric.clone(),
                    message: format!(
                        "{} regressed by {:.2}% (threshold {:.2}%)",
                        c.metric,
                        c.change_pct.unwrap_or_default().abs(),
                        c.fail_threshold_pct
                    ),
                });
            }
            MetricStatus::Warning => {
                summary.warnings += 1;
                alerts.push(PerfAlert {
                    severity: "warning".to_string(),
                    metric: c.metric.clone(),
                    message: format!(
                        "{} worsened by {:.2}% (warn threshold {:.2}%)",
                        c.metric,
                        c.change_pct.unwrap_or_default().abs(),
                        policy.warn_pct
                    ),
                });
            }
            MetricStatus::Missing => {
                summary.missing += 1;
                alerts.push(PerfAlert {
                    severity: "warning".to_string(),
                    metric: c.metric.clone(),
                    message: format!("{} is in the baseline but was not measured", c.metric),
                });
            }
            MetricStatus::New => summary.new += 1,
            MetricStatus::Unchanged => summary.unchanged += 1,
            MetricStatus::Improved => summary.improved += 1,
        }
    }

    let verdict = if summary.regressed > 0 {
        Verdict::Fail
    } else if summary.warnings > 0 || summary.missing > 0 {
        Verdict::Warn
    } else {
        Verdict::Pass
    };

    Ok(RegressionReport {
        baseline: baseline.name.clone(),
        baseline_created_at: baseline.created_at.clone(),
        baseline_commit: baseline.git_commit.clone(),
        current_commit: current_git_commit(),
        generated_at: Utc::now().to_rfc3339(),
        policy: policy.clone(),
        verdict,
        summary,
        alerts,
        comparisons,
    })
}

fn classify(
    name: &str,
    base: &BaselineMetric,
    current: &MetricStats,
    fail_pct: f64,
    policy: &RegressionPolicy,
) -> MetricComparison {
    let warn_pct = policy.warn_pct;
    let base_mean = base.stats.mean;
    let delta = current.mean - base_mean;
    let pct = if base_mean != 0.0 {
        delta / base_mean.abs() * 100.0
    } else if delta == 0.0 {
        0.0
    } else if delta > 0.0 {
        // Any change from a zero baseline is unbounded in relative terms.
        f64::INFINITY
    } else {
        f64::NEG_INFINITY
    };
    // Positive `worsening` always means "worse", whatever the metric's direction.
    let worsening = if base.higher_is_better { -pct } else { pct };
    let within_noise = delta != 0.0 && delta.abs() <= policy.noise_sigma * base.stats.stddev;

    let status = if within_noise {
        MetricStatus::Unchanged
    } else if worsening > fail_pct {
        MetricStatus::Regressed
    } else if worsening > warn_pct {
        MetricStatus::Warning
    } else if worsening < -warn_pct {
        MetricStatus::Improved
    } else {
        MetricStatus::Unchanged
    };

    MetricComparison {
        metric: name.to_string(),
        unit: base.unit.clone(),
        status,
        baseline_mean: Some(base_mean),
        current_mean: Some(current.mean),
        // Clamp so the report stays serializable as JSON; rendered as +/-inf.
        change_pct: Some(pct.clamp(f64::MIN, f64::MAX)),
        fail_threshold_pct: fail_pct,
        within_noise,
    }
}

// ── Reports ───────────────────────────────────────────────────────────────────

fn fmt_value(value: Option<f64>, unit: &Option<String>) -> String {
    match value {
        Some(v) => match unit {
            Some(unit) => format!("{:.2} {}", v, unit),
            None => format!("{:.2}", v),
        },
        None => "-".to_string(),
    }
}

fn fmt_change(change: Option<f64>) -> String {
    match change {
        Some(v) if v >= f64::MAX => "+inf".to_string(),
        Some(v) if v <= f64::MIN => "-inf".to_string(),
        Some(v) => format!("{:+.2}%", v),
        None => "-".to_string(),
    }
}

impl RegressionReport {
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Performance Regression Report");
        let _ = writeln!(out, "=============================");
        let _ = writeln!(
            out,
            "Baseline : {} ({}{})",
            self.baseline,
            self.baseline_created_at,
            self.baseline_commit
                .as_ref()
                .map(|c| format!(", commit {}", c))
                .unwrap_or_default()
        );
        let _ = writeln!(
            out,
            "Policy   : warn > {:.2}%, fail > {:.2}%, noise band {:.1} sigma",
            self.policy.warn_pct, self.policy.fail_pct, self.policy.noise_sigma
        );
        let _ = writeln!(out, "Verdict  : {}", self.verdict.label());
        let _ = writeln!(
            out,
            "Summary  : {} regressed, {} warning, {} missing, {} improved, {} unchanged, {} new",
            self.summary.regressed,
            self.summary.warnings,
            self.summary.missing,
            self.summary.improved,
            self.summary.unchanged,
            self.summary.new
        );
        let _ = writeln!(out);
        for c in &self.comparisons {
            let _ = writeln!(
                out,
                "  [{:<9}] {:<40} {:>18} -> {:<18} {}{}",
                c.status.label(),
                c.metric,
                fmt_value(c.baseline_mean, &c.unit),
                fmt_value(c.current_mean, &c.unit),
                fmt_change(c.change_pct),
                if c.within_noise { " (noise)" } else { "" }
            );
        }
        if !self.alerts.is_empty() {
            let _ = writeln!(out, "\nAlerts:");
            for alert in &self.alerts {
                let _ = writeln!(out, "  - [{}] {}", alert.severity, alert.message);
            }
        }
        out
    }

    /// Markdown suitable for a PR comment or CI job summary.
    pub fn render_markdown(&self) -> String {
        let icon = match self.verdict {
            Verdict::Pass => "✅",
            Verdict::Warn => "⚠️",
            Verdict::Fail => "❌",
        };
        let mut out = String::new();
        let _ = writeln!(
            out,
            "## {} Performance regression check: {}\n",
            icon,
            self.verdict.label()
        );
        let _ = writeln!(
            out,
            "Baseline `{}` recorded {}{}. Thresholds: warn > {:.2}%, fail > {:.2}%, noise band {:.1}σ.\n",
            self.baseline,
            self.baseline_created_at,
            self.baseline_commit
                .as_ref()
                .map(|c| format!(" at `{}`", c))
                .unwrap_or_default(),
            self.policy.warn_pct,
            self.policy.fail_pct,
            self.policy.noise_sigma
        );
        let _ = writeln!(out, "| Status | Metric | Baseline | Current | Change |");
        let _ = writeln!(out, "|---|---|---:|---:|---:|");
        for c in &self.comparisons {
            let _ = writeln!(
                out,
                "| {} | `{}` | {} | {} | {}{} |",
                c.status.label(),
                c.metric,
                fmt_value(c.baseline_mean, &c.unit),
                fmt_value(c.current_mean, &c.unit),
                fmt_change(c.change_pct),
                if c.within_noise { " (noise)" } else { "" }
            );
        }
        if !self.alerts.is_empty() {
            let _ = writeln!(out, "\n### Alerts\n");
            for alert in &self.alerts {
                let _ = writeln!(out, "- **{}**: {}", alert.severity, alert.message);
            }
        }
        out
    }

    pub fn render(&self, format: ReportFormat) -> Result<String> {
        Ok(match format {
            ReportFormat::Text => self.render_text(),
            ReportFormat::Markdown => self.render_markdown(),
            ReportFormat::Json => serde_json::to_string_pretty(self)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Text,
    Markdown,
    Json,
}

impl ReportFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "markdown" | "md" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            other => anyhow::bail!(
                "unsupported report format '{}'; use text, markdown, or json",
                other
            ),
        }
    }
}

/// One row of a metric's trend across baseline versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselineTrendPoint {
    pub created_at: String,
    pub git_commit: Option<String>,
    pub mean: Option<f64>,
}

/// Mean of every metric across the recorded versions of a baseline.
pub fn baseline_trends(history: &[PerfBaseline]) -> BTreeMap<String, Vec<BaselineTrendPoint>> {
    let names: BTreeSet<&String> = history.iter().flat_map(|b| b.metrics.keys()).collect();
    names
        .into_iter()
        .map(|name| {
            let points = history
                .iter()
                .map(|b| BaselineTrendPoint {
                    created_at: b.created_at.clone(),
                    git_commit: b.git_commit.clone(),
                    mean: b.metrics.get(name).map(|m| m.stats.mean),
                })
                .collect();
            (name.clone(), points)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn metric(name: &str, samples: &[f64]) -> MetricSamples {
        MetricSamples {
            name: name.to_string(),
            unit: Some("insns".to_string()),
            higher_is_better: false,
            samples: samples.to_vec(),
        }
    }

    fn baseline(metrics: &[MetricSamples]) -> PerfBaseline {
        PerfBaseline::from_measurements("main", metrics).unwrap()
    }

    #[test]
    fn stats_are_computed() {
        let stats = MetricStats::from_samples(&[4.0, 1.0, 3.0, 2.0]).unwrap();
        assert_eq!(stats.samples, 4);
        assert_eq!(stats.mean, 2.5);
        assert_eq!(stats.median, 2.5);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 4.0);
        assert_eq!(stats.p95, 4.0);
        assert!((stats.stddev - 1.2909944).abs() < 1e-6);
        assert!(MetricStats::from_samples(&[]).is_none());
    }

    #[test]
    fn measurement_files_accept_list_and_map_layouts() {
        let list = parse_measurements(
            r#"{"metrics":[{"name":"tps","unit":"tx/s","higher_is_better":true,"samples":[10,12]}]}"#,
        )
        .unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].higher_is_better);

        let map = parse_measurements(r#"{"metrics":{"transfer.cpu":[1,2,3]}}"#).unwrap();
        assert_eq!(map[0].name, "transfer.cpu");
        assert_eq!(map[0].samples, vec![1.0, 2.0, 3.0]);

        assert!(parse_measurements(r#"{"metrics":{"empty":[]}}"#).is_err());
        assert!(parse_measurements("not json").is_err());
    }

    #[test]
    fn detects_regressions_warnings_and_improvements() {
        let base = baseline(&[
            metric("cpu", &[1000.0, 1000.0, 1000.0]),
            metric("mem", &[500.0, 500.0]),
            metric("fee", &[100.0, 100.0]),
            metric("stable", &[50.0, 50.0]),
            metric("dropped", &[1.0]),
        ]);
        let current = [
            metric("cpu", &[1200.0, 1200.0]), // +20% -> regression
            metric("mem", &[535.0, 535.0]),   // +7% -> warning
            metric("fee", &[80.0, 80.0]),     // -20% -> improved
            metric("stable", &[51.0, 51.0]),  // +2% -> unchanged
            metric("brand_new", &[5.0, 5.0]), // new
        ];
        let report = compare(&base, &current, &RegressionPolicy::default()).unwrap();

        let status = |name: &str| {
            report
                .comparisons
                .iter()
                .find(|c| c.metric == name)
                .unwrap()
                .status
        };
        assert_eq!(status("cpu"), MetricStatus::Regressed);
        assert_eq!(status("mem"), MetricStatus::Warning);
        assert_eq!(status("fee"), MetricStatus::Improved);
        assert_eq!(status("stable"), MetricStatus::Unchanged);
        assert_eq!(status("brand_new"), MetricStatus::New);
        assert_eq!(status("dropped"), MetricStatus::Missing);
        assert_eq!(report.verdict, Verdict::Fail);
        assert_eq!(report.summary.regressed, 1);
        assert_eq!(report.comparisons[0].metric, "cpu");
        assert!(report
            .alerts
            .iter()
            .any(|a| a.severity == "critical" && a.metric == "cpu"));
    }

    #[test]
    fn higher_is_better_metrics_regress_when_they_drop() {
        let mut tps = metric("tps", &[100.0, 100.0]);
        tps.higher_is_better = true;
        let base = baseline(&[tps.clone()]);
        tps.samples = vec![80.0, 80.0];
        let report = compare(&base, &[tps], &RegressionPolicy::default()).unwrap();
        assert_eq!(report.comparisons[0].status, MetricStatus::Regressed);
    }

    #[test]
    fn noisy_metrics_are_not_flagged() {
        // Baseline stddev ~158, so a +150 shift stays within a 2-sigma band.
        let base = baseline(&[metric("wall", &[800.0, 1000.0, 1200.0, 1000.0, 1000.0])]);
        let report = compare(
            &base,
            &[metric("wall", &[1150.0])],
            &RegressionPolicy::default(),
        )
        .unwrap();
        assert!(report.comparisons[0].within_noise);
        assert_eq!(report.comparisons[0].status, MetricStatus::Unchanged);
        assert_eq!(report.verdict, Verdict::Pass);
    }

    #[test]
    fn per_metric_thresholds_override_the_default() {
        let base = baseline(&[metric("cpu", &[100.0, 100.0])]);
        let policy = RegressionPolicy::default()
            .with_metric_overrides(&["cpu=25".to_string()])
            .unwrap();
        let report = compare(&base, &[metric("cpu", &[120.0])], &policy).unwrap();
        assert_eq!(report.comparisons[0].status, MetricStatus::Warning);
        assert_eq!(report.comparisons[0].fail_threshold_pct, 25.0);
        assert!(RegressionPolicy::default()
            .with_metric_overrides(&["cpu".to_string()])
            .is_err());
    }

    #[test]
    fn zero_baselines_are_handled() {
        let base = baseline(&[metric("errors", &[0.0, 0.0])]);
        let report = compare(
            &base,
            &[metric("errors", &[3.0])],
            &RegressionPolicy::default(),
        )
        .unwrap();
        assert_eq!(report.comparisons[0].status, MetricStatus::Regressed);
        assert!(report.render_text().contains("+inf"));
    }

    #[test]
    fn invalid_policy_is_rejected() {
        let base = baseline(&[metric("cpu", &[1.0])]);
        let policy = RegressionPolicy {
            warn_pct: 20.0,
            fail_pct: 10.0,
            ..RegressionPolicy::default()
        };
        assert!(compare(&base, &[metric("cpu", &[1.0])], &policy).is_err());
    }

    #[test]
    fn fail_on_controls_exit_behaviour() {
        assert!(FailOn::Regression.should_fail(Verdict::Fail));
        assert!(!FailOn::Regression.should_fail(Verdict::Warn));
        assert!(FailOn::Warning.should_fail(Verdict::Warn));
        assert!(!FailOn::Never.should_fail(Verdict::Fail));
        assert!(FailOn::parse("bogus").is_err());
    }

    #[test]
    fn store_saves_loads_lists_and_tracks_history() {
        let dir = TempDir::new().unwrap();
        let store = BaselineStore::new(dir.path());
        let first = baseline(&[metric("cpu", &[100.0])]);
        store.save(&first).unwrap();
        let mut second = baseline(&[metric("cpu", &[90.0])]);
        second.created_at = "later".to_string();
        store.save(&second).unwrap();

        assert_eq!(store.load("main").unwrap().metrics["cpu"].stats.mean, 90.0);
        assert_eq!(store.list().unwrap(), vec!["main".to_string()]);
        let history = store.history("main").unwrap();
        assert_eq!(history.len(), 2);
        let trends = baseline_trends(&history);
        let means: Vec<Option<f64>> = trends["cpu"].iter().map(|p| p.mean).collect();
        assert_eq!(means, vec![Some(100.0), Some(90.0)]);

        assert!(store.load("missing").is_err());
        assert!(store.load("../escape").is_err());
    }

    #[test]
    fn reports_render_in_every_format() {
        let base = baseline(&[metric("cpu", &[100.0, 100.0])]);
        let report = compare(
            &base,
            &[metric("cpu", &[150.0])],
            &RegressionPolicy::default(),
        )
        .unwrap();
        let text = report.render(ReportFormat::Text).unwrap();
        assert!(text.contains("Verdict  : FAIL"));
        let md = report.render(ReportFormat::Markdown).unwrap();
        assert!(md.contains("| regressed | `cpu` |"));
        assert!(md.contains("### Alerts"));
        let json: serde_json::Value =
            serde_json::from_str(&report.render(ReportFormat::Json).unwrap()).unwrap();
        assert_eq!(json["verdict"], "fail");
        assert!(ReportFormat::parse("xml").is_err());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn measure_command_times_successful_runs_and_rejects_failures() {
        let samples = measure_command("noop", "true", 3, 1).unwrap();
        assert_eq!(samples.name, "noop.wall_time_ms");
        assert_eq!(samples.samples.len(), 3);
        assert!(measure_command("bad", "false", 1, 0).is_err());
        assert!(measure_command("none", "true", 0, 0).is_err());
    }
}
