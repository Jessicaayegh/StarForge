//! Security best-practices analyzer for Soroban contracts.
//!
//! A small rule engine evaluates contract source against a library of
//! best practices mapped to industry references (OWASP Smart Contract Top 10,
//! CWE). Results are turned into a weighted security score and grade,
//! prioritized recommendations, reports (text, Markdown, JSON, SARIF), and a
//! per-project remediation tracker that follows findings across runs.
//!
//! Findings can be suppressed inline with a justification comment on the
//! offending line or the line above it:
//!
//! ```text
//! // starforge-allow(SF-ERR-001): value was validated above
//! ```

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Default per-project remediation state file.
pub const DEFAULT_STATE_FILE: &str = ".starforge/security/best-practices.json";

// ── Severity ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "info" => Ok(Self::Info),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "critical" => Ok(Self::Critical),
            other => anyhow::bail!(
                "unknown severity '{}'; use info, low, medium, high, or critical",
                other
            ),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Score points deducted for the first occurrence of a rule at this severity.
    fn weight(self) -> f64 {
        match self {
            Self::Info => 1.0,
            Self::Low => 3.0,
            Self::Medium => 8.0,
            Self::High => 15.0,
            Self::Critical => 25.0,
        }
    }

    fn sarif_level(self) -> &'static str {
        match self {
            Self::Critical | Self::High => "error",
            Self::Medium => "warning",
            Self::Low | Self::Info => "note",
        }
    }
}

// ── Source model ──────────────────────────────────────────────────────────────

/// A function extracted from contract source.
#[derive(Debug, Clone)]
pub struct SourceFunction {
    pub name: String,
    pub is_pub: bool,
    /// 1-based line of the `fn` signature.
    pub line: usize,
    /// Body text including the signature.
    pub body: String,
    /// True when the function lives in a `#[contractimpl]` block.
    pub in_contract_impl: bool,
    /// True when preceded by a `///` doc comment.
    pub documented: bool,
}

/// Parsed view of one contract source file handed to every rule.
#[derive(Debug, Clone)]
pub struct ContractSource {
    pub path: String,
    pub text: String,
    pub lines: Vec<String>,
    pub functions: Vec<SourceFunction>,
    /// Line ranges (1-based, inclusive) of `#[cfg(test)]` modules.
    test_ranges: Vec<(usize, usize)>,
}

impl ContractSource {
    pub fn parse(path: &str, text: &str) -> Self {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let test_ranges = find_test_ranges(&lines);
        let functions = extract_functions(&lines, &test_ranges);
        Self {
            path: path.to_string(),
            text: text.to_string(),
            lines,
            functions,
            test_ranges,
        }
    }

    /// True when `line` (1-based) belongs to a `#[cfg(test)]` module.
    pub fn in_test_code(&self, line: usize) -> bool {
        self.test_ranges
            .iter()
            .any(|(start, end)| line >= *start && line <= *end)
    }

    /// Non-test lines (1-based number, text) with `//` comments stripped.
    fn code_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.lines
            .iter()
            .enumerate()
            .map(|(i, line)| (i + 1, strip_line_comment(line)))
            .filter(move |(n, _)| !self.in_test_code(*n))
    }

    fn contract_functions(&self) -> impl Iterator<Item = &SourceFunction> {
        self.functions.iter().filter(|f| f.in_contract_impl)
    }

    fn line_text(&self, line: usize) -> &str {
        self.lines
            .get(line.saturating_sub(1))
            .map(String::as_str)
            .unwrap_or("")
    }
}

fn strip_line_comment(line: &str) -> &str {
    match line.find("//") {
        Some(index) => &line[..index],
        None => line,
    }
}

fn brace_delta(line: &str) -> i64 {
    let code = strip_line_comment(line);
    code.matches('{').count() as i64 - code.matches('}').count() as i64
}

/// End line (1-based, inclusive) of the block opened at or after `start` (0-based).
fn block_end(lines: &[String], start: usize) -> usize {
    let mut depth = 0i64;
    let mut opened = false;
    for (i, line) in lines.iter().enumerate().skip(start) {
        let code = strip_line_comment(line);
        if code.contains('{') {
            opened = true;
        }
        depth += brace_delta(line);
        if opened && depth <= 0 {
            return i + 1;
        }
        // A bodiless declaration (`fn f();`) ends at its semicolon.
        if !opened && code.trim_end().ends_with(';') {
            return i + 1;
        }
    }
    lines.len()
}

fn find_test_ranges(lines: &[String]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("#[cfg(test)]") {
            let end = block_end(lines, i + 1);
            ranges.push((i + 1, end));
            i = end;
        } else {
            i += 1;
        }
    }
    ranges
}

fn extract_functions(lines: &[String], test_ranges: &[(usize, usize)]) -> Vec<SourceFunction> {
    let in_test = |line: usize| test_ranges.iter().any(|(s, e)| line >= *s && line <= *e);

    // Ranges of #[contractimpl] blocks.
    let mut impl_ranges = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("#[contractimpl") {
            if let Some(offset) = lines[i + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with("impl"))
            {
                let start = i + 1 + offset;
                impl_ranges.push((start + 1, block_end(lines, start)));
            }
        }
    }

    let mut functions = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let code = strip_line_comment(line).trim_start();
        let rest = code
            .strip_prefix("pub(crate) ")
            .or_else(|| code.strip_prefix("pub "))
            .unwrap_or(code);
        let is_pub = code.starts_with("pub ");
        let Some(signature) = rest.strip_prefix("fn ") else {
            continue;
        };
        let name: String = signature
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() || in_test(i + 1) {
            continue;
        }
        let end = block_end(lines, i);
        let documented = lines[..i]
            .iter()
            .rev()
            .map(|l| l.trim())
            .find(|l| !l.starts_with("#["))
            .is_some_and(|l| l.starts_with("///"));
        functions.push(SourceFunction {
            name,
            is_pub,
            line: i + 1,
            body: lines[i..end].join("\n"),
            in_contract_impl: impl_ranges.iter().any(|(s, e)| i + 1 >= *s && i < *e),
            documented,
        });
    }
    functions
}

// ── Rules ─────────────────────────────────────────────────────────────────────

/// A violation reported by a rule check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub line: usize,
    pub function: Option<String>,
    pub evidence: String,
}

type RuleCheck = fn(&ContractSource) -> Vec<Violation>;

/// A best practice the analyzer enforces.
#[derive(Clone)]
pub struct BestPracticeRule {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub severity: Severity,
    /// Industry references, e.g. `OWASP-SC01:2025`, `CWE-862`.
    pub references: &'static [&'static str],
    pub rationale: &'static str,
    pub recommendation: &'static str,
    check: RuleCheck,
}

impl std::fmt::Debug for BestPracticeRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BestPracticeRule")
            .field("id", &self.id)
            .field("severity", &self.severity)
            .finish()
    }
}

/// Serializable description of a rule, for listings and reports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleInfo {
    pub id: String,
    pub title: String,
    pub category: String,
    pub severity: Severity,
    pub references: Vec<String>,
    pub rationale: String,
    pub recommendation: String,
}

impl BestPracticeRule {
    pub fn info(&self) -> RuleInfo {
        RuleInfo {
            id: self.id.to_string(),
            title: self.title.to_string(),
            category: self.category.to_string(),
            severity: self.severity,
            references: self.references.iter().map(|r| r.to_string()).collect(),
            rationale: self.rationale.to_string(),
            recommendation: self.recommendation.to_string(),
        }
    }
}

const STORAGE_WRITES: &[&str] = &[".set(", ".remove(", ".extend_ttl(", ".bump("];
const AUTH_CALLS: &[&str] = &["require_auth(", "require_auth_for_args("];

fn writes_storage(body: &str) -> bool {
    body.contains(".storage()") && STORAGE_WRITES.iter().any(|w| body.contains(w))
}

fn requires_auth(body: &str) -> bool {
    AUTH_CALLS.iter().any(|c| body.contains(c))
}

fn fn_violation(f: &SourceFunction, evidence: String) -> Violation {
    Violation {
        line: f.line,
        function: Some(f.name.clone()),
        evidence,
    }
}

/// Report every non-test line containing any needle.
fn lines_containing(src: &ContractSource, needles: &[&str]) -> Vec<Violation> {
    src.code_lines()
        .filter(|(_, code)| needles.iter().any(|n| code.contains(n)))
        .map(|(line, code)| Violation {
            line,
            function: function_at(src, line),
            evidence: code.trim().to_string(),
        })
        .collect()
}

fn function_at(src: &ContractSource, line: usize) -> Option<String> {
    src.functions
        .iter()
        .rfind(|f| f.line <= line && line < f.line + f.body.lines().count())
        .map(|f| f.name.clone())
}

fn is_initializer(name: &str) -> bool {
    matches!(name, "init" | "initialize" | "__constructor" | "setup")
}

fn check_missing_auth(src: &ContractSource) -> Vec<Violation> {
    src.contract_functions()
        .filter(|f| f.is_pub && !is_initializer(&f.name))
        .filter(|f| writes_storage(&f.body) && !requires_auth(&f.body))
        .map(|f| {
            fn_violation(
                f,
                format!("`{}` writes storage without require_auth()", f.name),
            )
        })
        .collect()
}

fn check_reinitialization(src: &ContractSource) -> Vec<Violation> {
    src.contract_functions()
        .filter(|f| f.is_pub && is_initializer(&f.name) && f.name != "__constructor")
        .filter(|f| writes_storage(&f.body) && !f.body.contains(".has("))
        .map(|f| {
            fn_violation(
                f,
                format!(
                    "`{}` sets state without checking whether it was already initialized",
                    f.name
                ),
            )
        })
        .collect()
}

fn check_upgrade_auth(src: &ContractSource) -> Vec<Violation> {
    src.contract_functions()
        .filter(|f| f.body.contains("update_current_contract_wasm") && !requires_auth(&f.body))
        .map(|f| {
            fn_violation(
                f,
                format!("`{}` upgrades contract WASM without require_auth()", f.name),
            )
        })
        .collect()
}

fn check_mock_auth(src: &ContractSource) -> Vec<Violation> {
    lines_containing(src, &["mock_all_auths", "mock_auths("])
}

fn check_unchecked_arithmetic(src: &ContractSource) -> Vec<Violation> {
    let amount_words = ["amount", "balance", "supply", "total", "price", "fee"];
    src.contract_functions()
        .flat_map(|f| {
            f.body
                .lines()
                .enumerate()
                .map(move |(offset, line)| (f, f.line + offset, strip_line_comment(line)))
        })
        .filter(|(_, _, code)| {
            let lower = code.to_lowercase();
            let arithmetic = ["+=", "-=", "*=", " + ", " - ", " * "]
                .iter()
                .any(|op| code.contains(op));
            arithmetic
                && amount_words.iter().any(|w| lower.contains(w))
                && !code.contains("checked_")
                && !code.contains("saturating_")
        })
        .map(|(f, line, code)| Violation {
            line,
            function: Some(f.name.clone()),
            evidence: code.trim().to_string(),
        })
        .collect()
}

fn check_overflow_checks_profile(src: &ContractSource) -> Vec<Violation> {
    // Only meaningful when the analyzed input includes the crate manifest.
    if src.path.ends_with("Cargo.toml")
        && src.text.contains("[profile.release]")
        && !src.text.contains("overflow-checks = true")
    {
        let line = src
            .lines
            .iter()
            .position(|l| l.trim() == "[profile.release]")
            .map(|i| i + 1)
            .unwrap_or(1);
        return vec![Violation {
            line,
            function: None,
            evidence: "[profile.release] without overflow-checks = true".to_string(),
        }];
    }
    Vec::new()
}

fn check_unwrap(src: &ContractSource) -> Vec<Violation> {
    lines_containing(src, &[".unwrap()", ".expect("])
}

fn check_string_panics(src: &ContractSource) -> Vec<Violation> {
    lines_containing(src, &["panic!(\"", "unreachable!(\"", "unimplemented!("])
}

fn check_missing_ttl(src: &ContractSource) -> Vec<Violation> {
    let uses_long_lived_storage = src.code_lines().any(|(_, code)| {
        (code.contains(".persistent()") || code.contains(".instance()")) && code.contains(".set(")
    });
    if uses_long_lived_storage && !src.text.contains("extend_ttl") && !src.text.contains(".bump(") {
        let line = src
            .code_lines()
            .find(|(_, code)| code.contains(".persistent()") || code.contains(".instance()"))
            .map(|(line, _)| line)
            .unwrap_or(1);
        return vec![Violation {
            line,
            function: function_at(src, line),
            evidence: "persistent/instance storage is written but TTL is never extended"
                .to_string(),
        }];
    }
    Vec::new()
}

fn check_unbounded_instance_collections(src: &ContractSource) -> Vec<Violation> {
    src.code_lines()
        .filter(|(_, code)| {
            code.contains(".instance()")
                && code.contains(".set(")
                && (code.contains("Vec") || code.contains("Map") || code.contains("vec!"))
        })
        .map(|(line, code)| Violation {
            line,
            function: function_at(src, line),
            evidence: code.trim().to_string(),
        })
        .collect()
}

fn check_missing_events(src: &ContractSource) -> Vec<Violation> {
    let mutating: Vec<&SourceFunction> = src
        .contract_functions()
        .filter(|f| f.is_pub && writes_storage(&f.body))
        .collect();
    if mutating.is_empty() {
        return Vec::new();
    }
    mutating
        .into_iter()
        .filter(|f| !f.body.contains(".events()") && !f.body.contains("::publish("))
        .filter(|f| !is_initializer(&f.name))
        .map(|f| {
            fn_violation(
                f,
                format!("`{}` changes state without emitting an event", f.name),
            )
        })
        .collect()
}

fn check_external_call_before_effects(src: &ContractSource) -> Vec<Violation> {
    let call_markers = ["invoke_contract", "Client::new(", "try_invoke_contract"];
    src.contract_functions()
        .filter_map(|f| {
            let lines: Vec<&str> = f.body.lines().map(strip_line_comment).collect();
            let call = lines
                .iter()
                .position(|l| call_markers.iter().any(|m| l.contains(m)))?;
            let write_after = lines[call + 1..]
                .iter()
                .any(|l| l.contains(".storage()") || STORAGE_WRITES.iter().any(|w| l.contains(w)));
            write_after.then(|| {
                fn_violation(
                    f,
                    format!(
                        "`{}` writes state after an external contract call (line {})",
                        f.name,
                        f.line + call
                    ),
                )
            })
        })
        .collect()
}

fn check_unbounded_loops(src: &ContractSource) -> Vec<Violation> {
    src.contract_functions()
        .filter(|f| f.body.contains(".storage()"))
        .flat_map(|f| {
            f.body
                .lines()
                .enumerate()
                .map(move |(offset, line)| (f, f.line + offset, strip_line_comment(line)))
        })
        .filter(|(_, _, code)| {
            let trimmed = code.trim_start();
            (trimmed.starts_with("for ") && trimmed.contains(" in ") && !trimmed.contains(".."))
                || trimmed.starts_with("loop")
                || trimmed.starts_with("while ")
        })
        .map(|(f, line, code)| Violation {
            line,
            function: Some(f.name.clone()),
            evidence: code.trim().to_string(),
        })
        .collect()
}

fn check_weak_randomness(src: &ContractSource) -> Vec<Violation> {
    src.code_lines()
        .filter(|(_, code)| {
            (code.contains("ledger().timestamp()") || code.contains("ledger().sequence()"))
                && (code.contains('%') || code.to_lowercase().contains("rand"))
        })
        .map(|(line, code)| Violation {
            line,
            function: function_at(src, line),
            evidence: code.trim().to_string(),
        })
        .collect()
}

fn check_no_std(src: &ContractSource) -> Vec<Violation> {
    if src.path.ends_with(".rs")
        && src.text.contains("#[contract]")
        && !src.text.contains("#![no_std]")
    {
        return vec![Violation {
            line: 1,
            function: None,
            evidence: "contract crate root does not declare #![no_std]".to_string(),
        }];
    }
    Vec::new()
}

fn check_undocumented_entry_points(src: &ContractSource) -> Vec<Violation> {
    src.contract_functions()
        .filter(|f| f.is_pub && !f.documented)
        .map(|f| {
            fn_violation(
                f,
                format!("public entry point `{}` has no doc comment", f.name),
            )
        })
        .collect()
}

/// The built-in best-practices library.
pub fn rule_library() -> Vec<BestPracticeRule> {
    vec![
        BestPracticeRule {
            id: "SF-AUTH-001",
            title: "State-changing entry point without authorization",
            category: "access-control",
            severity: Severity::Critical,
            references: &["OWASP-SC01:2025", "CWE-862"],
            rationale: "Any account can invoke a public function that writes storage without require_auth(), taking over balances or configuration.",
            recommendation: "Call `address.require_auth()` (or `require_auth_for_args`) for the acting account before mutating storage.",
            check: check_missing_auth,
        },
        BestPracticeRule {
            id: "SF-AUTH-002",
            title: "Initializer can be called more than once",
            category: "access-control",
            severity: Severity::High,
            references: &["OWASP-SC01:2025", "CWE-665"],
            rationale: "A re-callable initializer lets an attacker overwrite the admin or core configuration after deployment.",
            recommendation: "Guard initialization with `if env.storage().instance().has(&DataKey::Admin) { panic_with_error!(...) }`, or use `__constructor`.",
            check: check_reinitialization,
        },
        BestPracticeRule {
            id: "SF-AUTH-003",
            title: "Contract upgrade without authorization",
            category: "access-control",
            severity: Severity::Critical,
            references: &["OWASP-SC01:2025", "CWE-284"],
            rationale: "Unauthenticated calls to update_current_contract_wasm let anyone replace the contract code.",
            recommendation: "Load the stored admin and call `admin.require_auth()` before `update_current_contract_wasm`.",
            check: check_upgrade_auth,
        },
        BestPracticeRule {
            id: "SF-AUTH-004",
            title: "Authorization mocking outside tests",
            category: "access-control",
            severity: Severity::Critical,
            references: &["CWE-489"],
            rationale: "mock_all_auths()/mock_auths() disable signature checks and must never ship in contract code.",
            recommendation: "Move auth mocking into `#[cfg(test)]` modules only.",
            check: check_mock_auth,
        },
        BestPracticeRule {
            id: "SF-ARITH-001",
            title: "Unchecked arithmetic on value-bearing variables",
            category: "arithmetic",
            severity: Severity::High,
            references: &["OWASP-SC08:2025", "CWE-190", "CWE-191"],
            rationale: "Overflow or underflow on balances and supplies can mint value or wrap balances.",
            recommendation: "Use `checked_add`/`checked_sub`/`checked_mul` and return a contract error on overflow.",
            check: check_unchecked_arithmetic,
        },
        BestPracticeRule {
            id: "SF-ARITH-002",
            title: "Release profile without overflow checks",
            category: "arithmetic",
            severity: Severity::Medium,
            references: &["OWASP-SC08:2025", "CWE-190"],
            rationale: "Without overflow-checks, integer overflow silently wraps in release WASM builds.",
            recommendation: "Set `overflow-checks = true` under `[profile.release]` in Cargo.toml.",
            check: check_overflow_checks_profile,
        },
        BestPracticeRule {
            id: "SF-ERR-001",
            title: "unwrap()/expect() in contract code",
            category: "error-handling",
            severity: Severity::Medium,
            references: &["OWASP-SC05:2025", "CWE-248"],
            rationale: "Panics from unwrap/expect abort with opaque errors that clients cannot handle or distinguish.",
            recommendation: "Return `Result<_, ContractError>` or use `panic_with_error!` with a `#[contracterror]` enum.",
            check: check_unwrap,
        },
        BestPracticeRule {
            id: "SF-ERR-002",
            title: "String panics instead of typed contract errors",
            category: "error-handling",
            severity: Severity::Low,
            references: &["OWASP-SC05:2025", "CWE-755"],
            rationale: "String panics bloat the WASM and do not surface stable error codes to callers.",
            recommendation: "Define a `#[contracterror]` enum and use `panic_with_error!(&env, Error::X)`.",
            check: check_string_panics,
        },
        BestPracticeRule {
            id: "SF-STORE-001",
            title: "Long-lived storage without TTL management",
            category: "storage",
            severity: Severity::Medium,
            references: &["CWE-400"],
            rationale: "Persistent and instance entries are archived when their TTL expires, making the contract unusable until restored.",
            recommendation: "Call `extend_ttl` on instance and persistent entries when they are written or read.",
            check: check_missing_ttl,
        },
        BestPracticeRule {
            id: "SF-STORE-002",
            title: "Unbounded collection stored in instance storage",
            category: "storage",
            severity: Severity::Medium,
            references: &["OWASP-SC10:2025", "CWE-770"],
            rationale: "Instance storage is loaded on every call; growing Vec/Map values there raise costs until calls exceed resource limits.",
            recommendation: "Store per-item entries under persistent keys (e.g. `DataKey::Item(id)`) instead of one growing collection.",
            check: check_unbounded_instance_collections,
        },
        BestPracticeRule {
            id: "SF-EVT-001",
            title: "State change without an event",
            category: "observability",
            severity: Severity::Low,
            references: &["CWE-778"],
            rationale: "Without events, indexers, monitors, and incident responders cannot observe state changes.",
            recommendation: "Publish an event (`env.events().publish(...)`) from every state-changing entry point.",
            check: check_missing_events,
        },
        BestPracticeRule {
            id: "SF-EXT-001",
            title: "State written after external contract call",
            category: "external-calls",
            severity: Severity::High,
            references: &["OWASP-SC05:2025", "CWE-841"],
            rationale: "Writing state after calling another contract breaks checks-effects-interactions and invites re-entrancy-style inconsistencies.",
            recommendation: "Update storage before calling other contracts, and validate their results.",
            check: check_external_call_before_effects,
        },
        BestPracticeRule {
            id: "SF-DOS-001",
            title: "Unbounded iteration in storage-backed entry point",
            category: "denial-of-service",
            severity: Severity::Medium,
            references: &["OWASP-SC10:2025", "CWE-834"],
            rationale: "Loops over data that grows with usage eventually exceed Soroban CPU and read limits, bricking the function.",
            recommendation: "Bound loops with explicit limits or paginate over keys.",
            check: check_unbounded_loops,
        },
        BestPracticeRule {
            id: "SF-RAND-001",
            title: "Ledger data used as randomness",
            category: "randomness",
            severity: Severity::High,
            references: &["OWASP-SC09:2025", "CWE-338"],
            rationale: "Ledger timestamp and sequence are known to validators and submitters in advance.",
            recommendation: "Use `env.prng()` for non-critical randomness, or a commit-reveal/oracle scheme for value-bearing outcomes.",
            check: check_weak_randomness,
        },
        BestPracticeRule {
            id: "SF-BASE-001",
            title: "Contract crate does not declare #![no_std]",
            category: "baseline",
            severity: Severity::Low,
            references: &["CWE-1104"],
            rationale: "Soroban contracts are expected to be no_std; linking std increases WASM size and attack surface.",
            recommendation: "Add `#![no_std]` to the contract crate root.",
            check: check_no_std,
        },
        BestPracticeRule {
            id: "SF-DOC-001",
            title: "Undocumented public entry point",
            category: "maintainability",
            severity: Severity::Info,
            references: &["CWE-1059"],
            rationale: "Undocumented entry points make authorization and invariants harder to review.",
            recommendation: "Add `///` docs describing who may call the function and what it changes.",
            check: check_undocumented_entry_points,
        },
    ]
}

// ── Analysis ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub fingerprint: String,
    pub rule_id: String,
    pub title: String,
    pub category: String,
    pub severity: Severity,
    pub file: String,
    pub line: usize,
    #[serde(default)]
    pub function: Option<String>,
    pub evidence: String,
    pub references: Vec<String>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuppressedFinding {
    pub rule_id: String,
    pub file: String,
    pub line: usize,
    pub justification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategoryScore {
    pub category: String,
    pub score: f64,
    pub findings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityScore {
    /// 0-100, higher is better.
    pub score: f64,
    pub grade: char,
    pub by_severity: BTreeMap<String, usize>,
    pub by_category: Vec<CategoryScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recommendation {
    pub priority: usize,
    pub rule_id: String,
    pub title: String,
    pub severity: Severity,
    pub occurrences: usize,
    pub action: String,
    pub rationale: String,
    pub references: Vec<String>,
    pub locations: Vec<String>,
}

/// Outcome of tracking findings against the remediation state.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RemediationProgress {
    pub new: usize,
    pub open: usize,
    pub resolved_this_run: usize,
    pub accepted: usize,
    pub total_resolved: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisReport {
    pub generated_at: String,
    pub files: Vec<String>,
    pub rules_evaluated: usize,
    pub score: SecurityScore,
    pub findings: Vec<Finding>,
    pub suppressed: Vec<SuppressedFinding>,
    pub recommendations: Vec<Recommendation>,
    #[serde(default)]
    pub remediation: Option<RemediationProgress>,
}

fn fingerprint(rule_id: &str, file: &str, function: Option<&str>, evidence: &str) -> String {
    // Line numbers are deliberately excluded so findings survive unrelated edits.
    let normalized: String = evidence.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hasher = Sha256::new();
    hasher.update(rule_id.as_bytes());
    hasher.update([0]);
    hasher.update(file.as_bytes());
    hasher.update([0]);
    hasher.update(function.unwrap_or("").as_bytes());
    hasher.update([0]);
    hasher.update(normalized.as_bytes());
    hex::encode(&hasher.finalize()[..8])
}

/// `// starforge-allow(RULE-ID): reason` on the line or the line above.
fn suppression(src: &ContractSource, line: usize, rule_id: &str) -> Option<String> {
    let marker = format!("starforge-allow({})", rule_id);
    [line, line.saturating_sub(1)]
        .into_iter()
        .filter(|l| *l >= 1)
        .map(|l| src.line_text(l))
        .find_map(|text| {
            let index = text.find(&marker)?;
            let reason = text[index + marker.len()..]
                .trim_start_matches(':')
                .trim()
                .to_string();
            Some(if reason.is_empty() {
                "no justification given".to_string()
            } else {
                reason
            })
        })
}

/// Options controlling which rules run.
#[derive(Debug, Clone, Default)]
pub struct AnalyzerOptions {
    /// Only report rules at or above this severity.
    pub min_severity: Option<Severity>,
    /// Rule ids to skip.
    pub disabled_rules: BTreeSet<String>,
}

/// The rule engine.
pub struct BestPracticesAnalyzer {
    rules: Vec<BestPracticeRule>,
}

impl Default for BestPracticesAnalyzer {
    fn default() -> Self {
        Self::new(AnalyzerOptions::default())
    }
}

impl BestPracticesAnalyzer {
    pub fn new(options: AnalyzerOptions) -> Self {
        let rules = rule_library()
            .into_iter()
            .filter(|rule| {
                options
                    .min_severity
                    .map_or(true, |min| rule.severity >= min)
            })
            .filter(|rule| !options.disabled_rules.contains(rule.id))
            .collect();
        Self { rules }
    }

    pub fn rules(&self) -> &[BestPracticeRule] {
        &self.rules
    }

    /// Analyze in-memory sources given as (path, contents).
    pub fn analyze_sources(&self, sources: &[(String, String)]) -> AnalysisReport {
        let mut findings = Vec::new();
        let mut suppressed = Vec::new();
        for (path, text) in sources {
            let src = ContractSource::parse(path, text);
            for rule in &self.rules {
                let mut seen = BTreeSet::new();
                for violation in (rule.check)(&src) {
                    if let Some(justification) = suppression(&src, violation.line, rule.id) {
                        suppressed.push(SuppressedFinding {
                            rule_id: rule.id.to_string(),
                            file: path.clone(),
                            line: violation.line,
                            justification,
                        });
                        continue;
                    }
                    let fp = fingerprint(
                        rule.id,
                        path,
                        violation.function.as_deref(),
                        &violation.evidence,
                    );
                    if !seen.insert(fp.clone()) {
                        continue;
                    }
                    findings.push(Finding {
                        fingerprint: fp,
                        rule_id: rule.id.to_string(),
                        title: rule.title.to_string(),
                        category: rule.category.to_string(),
                        severity: rule.severity,
                        file: path.clone(),
                        line: violation.line,
                        function: violation.function,
                        evidence: violation.evidence,
                        references: rule.references.iter().map(|r| r.to_string()).collect(),
                        recommendation: rule.recommendation.to_string(),
                    });
                }
            }
        }
        findings.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });

        AnalysisReport {
            generated_at: Utc::now().to_rfc3339(),
            files: sources.iter().map(|(p, _)| p.clone()).collect(),
            rules_evaluated: self.rules.len(),
            score: score(&self.rules, &findings),
            recommendations: recommendations(&self.rules, &findings),
            findings,
            suppressed,
            remediation: None,
        }
    }

    /// Analyze a `.rs` file, a `Cargo.toml`, or a directory (recursively,
    /// skipping `target/` and hidden directories).
    pub fn analyze_path(&self, path: &Path) -> Result<AnalysisReport> {
        let files = collect_sources(path)?;
        if files.is_empty() {
            anyhow::bail!("no Rust sources found at {}", path.display());
        }
        let mut sources = Vec::new();
        for file in files {
            let text = fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            sources.push((file.display().to_string(), text));
        }
        Ok(self.analyze_sources(&sources))
    }
}

fn collect_sources(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        anyhow::bail!("path not found: {}", path.display());
    }
    let mut files = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry?;
            let entry_path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    stack.push(entry_path);
                }
            } else if file_type.is_file() && (name.ends_with(".rs") || name == "Cargo.toml") {
                files.push(entry_path);
            }
        }
    }
    files.sort();
    Ok(files)
}

// ── Scoring & recommendations ─────────────────────────────────────────────────

/// Deduction for `count` occurrences of a rule: the first costs the full
/// severity weight, repeats add diminishing amounts, capped at twice the weight
/// so one noisy rule cannot zero the score on its own.
fn rule_penalty(severity: Severity, count: usize) -> f64 {
    let weight = severity.weight();
    let extra = (count.saturating_sub(1) as f64) * weight * 0.25;
    (weight + extra).min(weight * 2.0)
}

fn grade(score: f64) -> char {
    match score {
        s if s >= 90.0 => 'A',
        s if s >= 80.0 => 'B',
        s if s >= 70.0 => 'C',
        s if s >= 60.0 => 'D',
        _ => 'F',
    }
}

fn score(rules: &[BestPracticeRule], findings: &[Finding]) -> SecurityScore {
    let mut per_rule: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_severity: BTreeMap<String, usize> = BTreeMap::new();
    for finding in findings {
        *per_rule.entry(finding.rule_id.as_str()).or_insert(0) += 1;
        *by_severity
            .entry(finding.severity.label().to_string())
            .or_insert(0) += 1;
    }

    let penalty_for = |rule: &BestPracticeRule| {
        per_rule
            .get(rule.id)
            .map_or(0.0, |count| rule_penalty(rule.severity, *count))
    };

    let total_penalty: f64 = rules.iter().map(penalty_for).sum();
    let mut overall = (100.0 - total_penalty).max(0.0);
    // Any open critical issue caps the grade at D.
    if by_severity.contains_key("critical") {
        overall = overall.min(69.0);
    }

    let categories: BTreeSet<&str> = rules.iter().map(|r| r.category).collect();
    let by_category = categories
        .into_iter()
        .map(|category| {
            let in_category: Vec<&BestPracticeRule> =
                rules.iter().filter(|r| r.category == category).collect();
            let max: f64 = in_category.iter().map(|r| r.severity.weight() * 2.0).sum();
            let lost: f64 = in_category.iter().map(|r| penalty_for(r)).sum();
            CategoryScore {
                category: category.to_string(),
                score: if max > 0.0 {
                    ((1.0 - lost / max) * 100.0).max(0.0)
                } else {
                    100.0
                },
                findings: findings.iter().filter(|f| f.category == category).count(),
            }
        })
        .collect();

    SecurityScore {
        score: (overall * 10.0).round() / 10.0,
        grade: grade(overall),
        by_severity,
        by_category,
    }
}

fn recommendations(rules: &[BestPracticeRule], findings: &[Finding]) -> Vec<Recommendation> {
    let mut recs: Vec<Recommendation> = rules
        .iter()
        .filter_map(|rule| {
            let hits: Vec<&Finding> = findings.iter().filter(|f| f.rule_id == rule.id).collect();
            if hits.is_empty() {
                return None;
            }
            Some(Recommendation {
                priority: 0,
                rule_id: rule.id.to_string(),
                title: rule.title.to_string(),
                severity: rule.severity,
                occurrences: hits.len(),
                action: rule.recommendation.to_string(),
                rationale: rule.rationale.to_string(),
                references: rule.references.iter().map(|r| r.to_string()).collect(),
                locations: hits
                    .iter()
                    .map(|f| format!("{}:{}", f.file, f.line))
                    .collect(),
            })
        })
        .collect();
    // Highest score impact first.
    recs.sort_by(|a, b| {
        rule_penalty(b.severity, b.occurrences)
            .partial_cmp(&rule_penalty(a.severity, a.occurrences))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });
    for (index, rec) in recs.iter_mut().enumerate() {
        rec.priority = index + 1;
    }
    recs
}

// ── Remediation tracking ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrackedStatus {
    Open,
    Resolved,
    /// Risk accepted by a maintainer; excluded from gating.
    Accepted,
}

impl TrackedStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Accepted => "accepted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackedFinding {
    pub fingerprint: String,
    pub rule_id: String,
    pub title: String,
    pub severity: Severity,
    pub file: String,
    pub line: usize,
    pub status: TrackedStatus,
    pub first_seen: String,
    pub last_seen: String,
    #[serde(default)]
    pub resolved_at: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RemediationState {
    pub findings: BTreeMap<String, TrackedFinding>,
    #[serde(default)]
    pub score_history: Vec<ScoreSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreSnapshot {
    pub at: String,
    pub score: f64,
    pub grade: char,
    pub open: usize,
}

/// Persists remediation state per project so findings are followed across runs:
/// new findings open, disappearing ones resolve, and returning ones reopen.
pub struct RemediationTracker {
    path: PathBuf,
}

impl RemediationTracker {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<RemediationState> {
        if !self.path.exists() {
            return Ok(RemediationState::default());
        }
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse remediation state {}", self.path.display()))
    }

    fn save(&self, state: &RemediationState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(state)?)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }

    /// Reconcile a report with stored state, persist it, and attach progress to the report.
    ///
    /// Only files analyzed in this run are resolved, so scanning a subset of a
    /// project does not close findings in files that were not examined.
    pub fn track(&self, report: &mut AnalysisReport) -> Result<RemediationProgress> {
        let mut state = self.load()?;
        let now = Utc::now().to_rfc3339();
        let mut progress = RemediationProgress::default();
        let current: BTreeSet<&str> = report
            .findings
            .iter()
            .map(|f| f.fingerprint.as_str())
            .collect();
        let analyzed: BTreeSet<&str> = report.files.iter().map(String::as_str).collect();

        for finding in &report.findings {
            match state.findings.get_mut(&finding.fingerprint) {
                Some(tracked) => {
                    tracked.last_seen = now.clone();
                    tracked.line = finding.line;
                    if tracked.status == TrackedStatus::Resolved {
                        tracked.status = TrackedStatus::Open;
                        tracked.resolved_at = None;
                        progress.new += 1;
                    }
                }
                None => {
                    state.findings.insert(
                        finding.fingerprint.clone(),
                        TrackedFinding {
                            fingerprint: finding.fingerprint.clone(),
                            rule_id: finding.rule_id.clone(),
                            title: finding.title.clone(),
                            severity: finding.severity,
                            file: finding.file.clone(),
                            line: finding.line,
                            status: TrackedStatus::Open,
                            first_seen: now.clone(),
                            last_seen: now.clone(),
                            resolved_at: None,
                            note: None,
                        },
                    );
                    progress.new += 1;
                }
            }
        }

        for tracked in state.findings.values_mut() {
            if tracked.status == TrackedStatus::Open
                && analyzed.contains(tracked.file.as_str())
                && !current.contains(tracked.fingerprint.as_str())
            {
                tracked.status = TrackedStatus::Resolved;
                tracked.resolved_at = Some(now.clone());
                progress.resolved_this_run += 1;
            }
        }

        for tracked in state.findings.values() {
            match tracked.status {
                TrackedStatus::Open => progress.open += 1,
                TrackedStatus::Accepted => progress.accepted += 1,
                TrackedStatus::Resolved => progress.total_resolved += 1,
            }
        }

        state.score_history.push(ScoreSnapshot {
            at: now,
            score: report.score.score,
            grade: report.score.grade,
            open: progress.open,
        });
        self.save(&state)?;
        report.remediation = Some(progress.clone());
        Ok(progress)
    }

    /// Mark a finding (by fingerprint or unique fingerprint prefix) as accepted
    /// risk or reopen it.
    pub fn set_status(
        &self,
        fingerprint: &str,
        status: TrackedStatus,
        note: Option<String>,
    ) -> Result<TrackedFinding> {
        let mut state = self.load()?;
        let matches: Vec<String> = state
            .findings
            .keys()
            .filter(|key| key.starts_with(fingerprint))
            .cloned()
            .collect();
        let key = match matches.as_slice() {
            [key] => key.clone(),
            [] => anyhow::bail!("no tracked finding matches '{}'", fingerprint),
            _ => anyhow::bail!(
                "'{}' matches {} findings; use a longer fingerprint",
                fingerprint,
                matches.len()
            ),
        };
        let tracked = state
            .findings
            .get_mut(&key)
            .expect("key was taken from the map");
        tracked.status = status;
        tracked.resolved_at = (status == TrackedStatus::Resolved).then(|| Utc::now().to_rfc3339());
        if note.is_some() {
            tracked.note = note;
        }
        let updated = tracked.clone();
        self.save(&state)?;
        Ok(updated)
    }
}

// ── Reports ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Text,
    Markdown,
    Json,
    Sarif,
}

impl ReportFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "markdown" | "md" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            "sarif" => Ok(Self::Sarif),
            other => anyhow::bail!(
                "unsupported report format '{}'; use text, markdown, json, or sarif",
                other
            ),
        }
    }
}

impl AnalysisReport {
    /// True when any finding is at or above `threshold`.
    pub fn has_findings_at_or_above(&self, threshold: Severity) -> bool {
        self.findings.iter().any(|f| f.severity >= threshold)
    }

    pub fn render(&self, format: ReportFormat) -> Result<String> {
        Ok(match format {
            ReportFormat::Text => self.render_text(),
            ReportFormat::Markdown => self.render_markdown(),
            ReportFormat::Json => serde_json::to_string_pretty(self)?,
            ReportFormat::Sarif => serde_json::to_string_pretty(&self.to_sarif())?,
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Security Best Practices Report");
        let _ = writeln!(out, "==============================");
        let _ = writeln!(
            out,
            "Score    : {:.1}/100 (grade {})",
            self.score.score, self.score.grade
        );
        let _ = writeln!(
            out,
            "Scanned  : {} file(s), {} rule(s)",
            self.files.len(),
            self.rules_evaluated
        );
        let _ = writeln!(
            out,
            "Findings : {} ({} suppressed)",
            self.findings.len(),
            self.suppressed.len()
        );
        if let Some(progress) = &self.remediation {
            let _ = writeln!(
                out,
                "Tracking : {} open, {} new, {} resolved this run, {} accepted",
                progress.open, progress.new, progress.resolved_this_run, progress.accepted
            );
        }
        let _ = writeln!(out, "\nCategory scores:");
        for category in &self.score.by_category {
            let _ = writeln!(
                out,
                "  {:<18} {:>5.1}  ({} finding(s))",
                category.category, category.score, category.findings
            );
        }
        if !self.findings.is_empty() {
            let _ = writeln!(out, "\nFindings:");
            for f in &self.findings {
                let _ = writeln!(
                    out,
                    "  [{:<8}] {} {}:{}  {}\n             {}  ({})  id={}",
                    f.severity.label(),
                    f.rule_id,
                    f.file,
                    f.line,
                    f.title,
                    f.evidence,
                    f.references.join(", "),
                    f.fingerprint
                );
            }
        }
        if !self.recommendations.is_empty() {
            let _ = writeln!(out, "\nRecommendations (highest impact first):");
            for rec in &self.recommendations {
                let _ = writeln!(
                    out,
                    "  {}. [{}] {} x{}: {}",
                    rec.priority,
                    rec.severity.label(),
                    rec.title,
                    rec.occurrences,
                    rec.action
                );
            }
        }
        out
    }

    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# Security Best Practices Report\n");
        let _ = writeln!(
            out,
            "**Score:** {:.1}/100 — grade **{}** · {} file(s) · {} rule(s) · {} finding(s), {} suppressed\n",
            self.score.score,
            self.score.grade,
            self.files.len(),
            self.rules_evaluated,
            self.findings.len(),
            self.suppressed.len()
        );
        if let Some(progress) = &self.remediation {
            let _ = writeln!(
                out,
                "**Remediation:** {} open · {} new · {} resolved this run · {} accepted · {} resolved overall\n",
                progress.open,
                progress.new,
                progress.resolved_this_run,
                progress.accepted,
                progress.total_resolved
            );
        }
        let _ = writeln!(out, "## Category scores\n");
        let _ = writeln!(out, "| Category | Score | Findings |\n|---|---:|---:|");
        for c in &self.score.by_category {
            let _ = writeln!(out, "| {} | {:.1} | {} |", c.category, c.score, c.findings);
        }
        if !self.recommendations.is_empty() {
            let _ = writeln!(out, "\n## Recommendations\n");
            for rec in &self.recommendations {
                let _ = writeln!(
                    out,
                    "{}. **{}** (`{}`, {}, {} occurrence(s)) — {}  \n   _Why:_ {}  \n   _References:_ {}",
                    rec.priority,
                    rec.title,
                    rec.rule_id,
                    rec.severity.label(),
                    rec.occurrences,
                    rec.action,
                    rec.rationale,
                    rec.references.join(", ")
                );
            }
        }
        if !self.findings.is_empty() {
            let _ = writeln!(out, "\n## Findings\n");
            let _ = writeln!(
                out,
                "| Severity | Rule | Location | Evidence |\n|---|---|---|---|"
            );
            for f in &self.findings {
                let _ = writeln!(
                    out,
                    "| {} | `{}` | `{}:{}` | {} |",
                    f.severity.label(),
                    f.rule_id,
                    f.file,
                    f.line,
                    f.evidence.replace('|', "\\|")
                );
            }
        }
        out
    }

    /// SARIF 2.1.0 for GitHub code scanning and other SAST dashboards.
    pub fn to_sarif(&self) -> serde_json::Value {
        let rule_ids: BTreeSet<&str> = self.findings.iter().map(|f| f.rule_id.as_str()).collect();
        let rules: Vec<serde_json::Value> = rule_library()
            .into_iter()
            .filter(|r| rule_ids.contains(r.id))
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "name": r.title,
                    "shortDescription": { "text": r.title },
                    "fullDescription": { "text": r.rationale },
                    "help": { "text": r.recommendation },
                    "properties": { "tags": r.references, "category": r.category },
                    "defaultConfiguration": { "level": r.severity.sarif_level() }
                })
            })
            .collect();
        let results: Vec<serde_json::Value> = self
            .findings
            .iter()
            .map(|f| {
                serde_json::json!({
                    "ruleId": f.rule_id,
                    "level": f.severity.sarif_level(),
                    "message": { "text": format!("{}: {}", f.title, f.evidence) },
                    "partialFingerprints": { "starforge/v1": f.fingerprint },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": f.file },
                            "region": { "startLine": f.line }
                        }
                    }]
                })
            })
            .collect();
        serde_json::json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": { "driver": {
                    "name": "starforge-best-practices",
                    "informationUri": "https://github.com/Nanle-code/StarForge",
                    "rules": rules
                }},
                "results": results
            }]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const VULNERABLE: &str = r#"#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct Token;

#[contractimpl]
impl Token {
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Mint tokens.
    pub fn mint(env: Env, to: Address, amount: i128) {
        let balance: i128 = env.storage().persistent().get(&to).unwrap();
        env.storage().persistent().set(&to, &(balance + amount));
    }

    /// Upgrade.
    pub fn upgrade(env: Env, hash: BytesN<32>) {
        env.deployer().update_current_contract_wasm(hash);
    }

    /// Lottery.
    pub fn draw(env: Env) -> u64 {
        env.ledger().timestamp() % 10
    }
}

#[cfg(test)]
mod test {
    fn t() {
        env.mock_all_auths();
        let x = foo().unwrap();
    }
}
"#;

    const HARDENED: &str = r#"#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct Token;

#[contractimpl]
impl Token {
    /// Initialize once.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().extend_ttl(100, 1000);
    }

    /// Mint tokens; admin only.
    pub fn mint(env: Env, to: Address, amount: i128) -> Result<(), Error> {
        let admin: Address = read_admin(&env)?;
        admin.require_auth();
        let balance: i128 = env.storage().persistent().get(&to).unwrap_or(0);
        let next = balance.checked_add(amount).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&to, &next);
        env.storage().persistent().extend_ttl(&to, 100, 1000);
        env.events().publish((symbol_short!("mint"), to), amount);
        Ok(())
    }
}
"#;

    fn analyze(source: &str) -> AnalysisReport {
        BestPracticesAnalyzer::default()
            .analyze_sources(&[("src/lib.rs".to_string(), source.to_string())])
    }

    fn rule_hits(report: &AnalysisReport, rule: &str) -> usize {
        report.findings.iter().filter(|f| f.rule_id == rule).count()
    }

    #[test]
    fn library_rules_are_unique_and_referenced() {
        let rules = rule_library();
        let ids: BTreeSet<&str> = rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), rules.len());
        assert!(rules.len() >= 15);
        assert!(rules.iter().all(|r| !r.references.is_empty()));
        assert!(rules.iter().all(|r| !r.recommendation.is_empty()));
    }

    #[test]
    fn source_parser_finds_contract_functions_and_test_modules() {
        let src = ContractSource::parse("lib.rs", VULNERABLE);
        let names: Vec<&str> = src
            .functions
            .iter()
            .filter(|f| f.in_contract_impl)
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(names, vec!["initialize", "mint", "upgrade", "draw"]);
        assert!(!src.functions.iter().any(|f| f.name == "t"));
        let mint = src.functions.iter().find(|f| f.name == "mint").unwrap();
        assert!(mint.documented);
        assert!(!src.functions[0].documented);
    }

    #[test]
    fn vulnerable_contract_triggers_expected_rules() {
        let report = analyze(VULNERABLE);
        assert_eq!(
            rule_hits(&report, "SF-AUTH-001"),
            1,
            "{:#?}",
            report.findings
        );
        assert_eq!(rule_hits(&report, "SF-AUTH-002"), 1);
        assert_eq!(rule_hits(&report, "SF-AUTH-003"), 1);
        assert_eq!(rule_hits(&report, "SF-ARITH-001"), 1);
        assert_eq!(rule_hits(&report, "SF-ERR-001"), 1);
        assert_eq!(rule_hits(&report, "SF-STORE-001"), 1);
        assert_eq!(rule_hits(&report, "SF-RAND-001"), 1);
        assert_eq!(rule_hits(&report, "SF-EVT-001"), 1);
        assert_eq!(rule_hits(&report, "SF-DOC-001"), 1);
        // Test-only code is ignored.
        assert_eq!(rule_hits(&report, "SF-AUTH-004"), 0);
        assert_eq!(report.findings[0].severity, Severity::Critical);
        assert!(report.score.score < 70.0);
        assert!(matches!(report.score.grade, 'D' | 'F'));
        assert_eq!(report.recommendations[0].priority, 1);
        assert_eq!(report.recommendations[0].severity, Severity::Critical);
    }

    #[test]
    fn hardened_contract_scores_well() {
        let report = analyze(HARDENED);
        assert!(report.findings.is_empty(), "{:#?}", report.findings);
        assert_eq!(report.score.score, 100.0);
        assert_eq!(report.score.grade, 'A');
        assert!(report.recommendations.is_empty());
    }

    #[test]
    fn inline_suppressions_require_matching_rule() {
        let source = HARDENED.replace(
            "        let balance: i128 = env.storage().persistent().get(&to).unwrap_or(0);",
            "        // starforge-allow(SF-ERR-001): key is set during initialize\n        let balance: i128 = env.storage().persistent().get(&to).unwrap();\n        let other = foo().unwrap();",
        );
        let report = analyze(&source);
        assert_eq!(report.suppressed.len(), 1);
        assert_eq!(
            report.suppressed[0].justification,
            "key is set during initialize"
        );
        assert_eq!(rule_hits(&report, "SF-ERR-001"), 1);
    }

    #[test]
    fn options_filter_rules() {
        let analyzer = BestPracticesAnalyzer::new(AnalyzerOptions {
            min_severity: Some(Severity::High),
            disabled_rules: ["SF-RAND-001".to_string()].into_iter().collect(),
        });
        assert!(analyzer
            .rules()
            .iter()
            .all(|r| r.severity >= Severity::High));
        let report = analyzer.analyze_sources(&[("lib.rs".into(), VULNERABLE.into())]);
        assert_eq!(rule_hits(&report, "SF-RAND-001"), 0);
        assert_eq!(rule_hits(&report, "SF-ERR-001"), 0);
        assert!(report.has_findings_at_or_above(Severity::Critical));
    }

    #[test]
    fn cargo_manifest_overflow_checks() {
        let manifest = "[package]\nname = \"c\"\n\n[profile.release]\nopt-level = \"z\"\n";
        let report = BestPracticesAnalyzer::default()
            .analyze_sources(&[("Cargo.toml".into(), manifest.into())]);
        assert_eq!(rule_hits(&report, "SF-ARITH-002"), 1);
        let fixed = format!("{}overflow-checks = true\n", manifest);
        let report =
            BestPracticesAnalyzer::default().analyze_sources(&[("Cargo.toml".into(), fixed)]);
        assert_eq!(rule_hits(&report, "SF-ARITH-002"), 0);
    }

    #[test]
    fn scoring_diminishes_repeats_and_caps_criticals() {
        assert_eq!(rule_penalty(Severity::Medium, 1), 8.0);
        assert_eq!(rule_penalty(Severity::Medium, 3), 12.0);
        assert_eq!(rule_penalty(Severity::Medium, 50), 16.0);
        assert_eq!(grade(95.0), 'A');
        assert_eq!(grade(59.9), 'F');
    }

    #[test]
    fn fingerprints_are_stable_across_line_moves() {
        let a = analyze(VULNERABLE);
        let shifted = VULNERABLE.replacen("#![no_std]\n", "#![no_std]\n\n\n", 1);
        let b = analyze(&shifted);
        let fa: BTreeSet<_> = a.findings.iter().map(|f| &f.fingerprint).collect();
        let fb: BTreeSet<_> = b.findings.iter().map(|f| &f.fingerprint).collect();
        assert_eq!(fa, fb);
    }

    #[test]
    fn remediation_tracker_follows_findings_across_runs() {
        let dir = TempDir::new().unwrap();
        let tracker = RemediationTracker::new(dir.path().join("state.json"));

        let mut first = analyze(VULNERABLE);
        let progress = tracker.track(&mut first).unwrap();
        assert_eq!(progress.new, first.findings.len());
        assert_eq!(progress.open, first.findings.len());
        assert!(first.remediation.is_some());

        // Accept one finding as known risk.
        let accepted = first.findings.last().unwrap().fingerprint.clone();
        let updated = tracker
            .set_status(
                &accepted[..8],
                TrackedStatus::Accepted,
                Some("tracked in #1".into()),
            )
            .unwrap();
        assert_eq!(updated.status, TrackedStatus::Accepted);

        // Fixing the contract resolves the remaining open findings.
        let mut second = analyze(HARDENED);
        let progress = tracker.track(&mut second).unwrap();
        assert_eq!(progress.new, 0);
        assert_eq!(progress.open, 0);
        assert_eq!(progress.accepted, 1);
        assert_eq!(progress.resolved_this_run, first.findings.len() - 1);

        // Regressing reopens them.
        let mut third = analyze(VULNERABLE);
        let progress = tracker.track(&mut third).unwrap();
        assert_eq!(progress.new, first.findings.len() - 1);

        let state = tracker.load().unwrap();
        assert_eq!(state.score_history.len(), 3);
        assert!(tracker
            .set_status("zzzz", TrackedStatus::Open, None)
            .is_err());
    }

    #[test]
    fn tracking_only_resolves_findings_in_analyzed_files() {
        let dir = TempDir::new().unwrap();
        let tracker = RemediationTracker::new(dir.path().join("state.json"));
        let mut first =
            BestPracticesAnalyzer::default().analyze_sources(&[("a.rs".into(), VULNERABLE.into())]);
        tracker.track(&mut first).unwrap();
        let mut other =
            BestPracticesAnalyzer::default().analyze_sources(&[("b.rs".into(), HARDENED.into())]);
        let progress = tracker.track(&mut other).unwrap();
        assert_eq!(progress.resolved_this_run, 0);
        assert_eq!(progress.open, first.findings.len());
    }

    #[test]
    fn reports_render_in_every_format() {
        let report = analyze(VULNERABLE);
        let text = report.render(ReportFormat::Text).unwrap();
        assert!(text.contains("Security Best Practices Report"));
        assert!(text.contains("SF-AUTH-001"));
        let md = report.render(ReportFormat::Markdown).unwrap();
        assert!(md.contains("## Recommendations"));
        let json: serde_json::Value =
            serde_json::from_str(&report.render(ReportFormat::Json).unwrap()).unwrap();
        assert!(json["score"]["score"].as_f64().is_some());
        let sarif: serde_json::Value =
            serde_json::from_str(&report.render(ReportFormat::Sarif).unwrap()).unwrap();
        assert_eq!(sarif["version"], "2.1.0");
        assert!(!sarif["runs"][0]["results"].as_array().unwrap().is_empty());
        assert!(ReportFormat::parse("pdf").is_err());
    }

    #[test]
    fn analyze_path_walks_directories() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), VULNERABLE).unwrap();
        fs::write(dir.path().join("target/gen.rs"), VULNERABLE).unwrap();
        let report = BestPracticesAnalyzer::default()
            .analyze_path(dir.path())
            .unwrap();
        assert_eq!(report.files.len(), 1);
        assert!(!report.findings.is_empty());
        assert!(BestPracticesAnalyzer::default()
            .analyze_path(&dir.path().join("missing"))
            .is_err());
    }
}
