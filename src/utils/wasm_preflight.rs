use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Soroban on-chain WASM size ceiling: 128 KiB.
pub const WASM_SIZE_LIMIT_BYTES: usize = 128 * 1024;

/// Governs what the pre-flight validator accepts or rejects.
#[derive(Debug, Clone)]
pub struct WasmPolicy {
    /// Maximum WASM size (bytes). Default: Soroban 128 KiB limit.
    pub max_size_bytes: usize,
    /// Import names that must not appear in the module.
    pub forbidden_imports: Vec<String>,
    /// Export names that *must* appear in the module (empty = no requirement).
    pub required_exports: Vec<String>,
    /// Optional allowlist of import namespaces/names.
    pub allowed_imports: Option<Vec<String>>,
    /// Optional allowlist of permitted exports.
    pub allowed_exports: Option<Vec<String>>,
}

impl Default for WasmPolicy {
    fn default() -> Self {
        Self {
            max_size_bytes: WASM_SIZE_LIMIT_BYTES,
            // Soroban forbids arbitrary WASI / OS-level host functions.
            forbidden_imports: vec![
                "proc_exit".to_string(),
                "fd_write".to_string(),
                "fd_read".to_string(),
                "environ_get".to_string(),
                "args_get".to_string(),
                "path_open".to_string(),
                "sock_accept".to_string(),
                "sock_recv".to_string(),
                "sock_send".to_string(),
            ],
            required_exports: vec![],
            // By default, we expect typical Soroban single-character module namespaces (plus maybe _).
            allowed_imports: Some(
                vec![
                    "a", "b", "c", "d", "e", "f", "g", "h", "i", "l", "m", "p", "r", "s", "t", "u",
                    "v", "x", "z", "_",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            ),
            allowed_exports: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightViolation {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightFinding {
    pub risk: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightReport {
    pub path: String,
    pub size_bytes: usize,
    pub is_valid_wasm: bool,
    pub passes_policy: bool,
    pub violations: Vec<PreflightViolation>,
    pub findings: Vec<PreflightFinding>,
    pub warnings: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
}

impl PreflightReport {
    pub fn is_ok(&self) -> bool {
        self.is_valid_wasm && self.passes_policy && self.violations.is_empty()
    }
}

/// Read a `.wasm` file and validate it against `policy`.
/// Returns `Err` only on I/O failure; validation errors are in `violations`.
pub fn validate_wasm_file(path: &Path, policy: &WasmPolicy) -> Result<PreflightReport> {
    let bytes = std::fs::read(path).with_context(|| format!("Cannot read {:?}", path))?;
    Ok(validate_wasm_bytes(&bytes, &path.to_string_lossy(), policy))
}

/// Validate raw WASM bytes against `policy`.
pub fn validate_wasm_bytes(bytes: &[u8], label: &str, policy: &WasmPolicy) -> PreflightReport {
    let mut violations: Vec<PreflightViolation> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // ── 1. Magic header + minimum length ─────────────────────────────────────
    let is_valid_wasm = bytes.len() >= 8 && &bytes[..4] == b"\0asm";
    if !is_valid_wasm {
        violations.push(PreflightViolation {
            code: "INVALID_MAGIC".to_string(),
            message: "File is not a valid WebAssembly binary (missing \\0asm magic header)."
                .to_string(),
        });
        return PreflightReport {
            path: label.to_string(),
            size_bytes: bytes.len(),
            is_valid_wasm: false,
            passes_policy: false,
            violations,
            findings: vec![],
            warnings,
            imports: vec![],
            exports: vec![],
        };
    }

    // ── 2. Size limit ─────────────────────────────────────────────────────────
    if bytes.len() > policy.max_size_bytes {
        violations.push(PreflightViolation {
            code: "SIZE_EXCEEDED".to_string(),
            message: format!(
                "Module is {} bytes ({:.1} KiB) — exceeds the {:.1} KiB policy limit. \
                 Run `starforge gas optimize` or `wasm-opt -Oz` to reduce size.",
                bytes.len(),
                bytes.len() as f64 / 1024.0,
                policy.max_size_bytes as f64 / 1024.0,
            ),
        });
    } else if bytes.len() as f64 / policy.max_size_bytes as f64 > 0.85 {
        warnings.push(format!(
            "Module is {:.1} KiB — {:.0}% of the {:.1} KiB limit. Consider optimizing.",
            bytes.len() as f64 / 1024.0,
            bytes.len() as f64 / policy.max_size_bytes as f64 * 100.0,
            policy.max_size_bytes as f64 / 1024.0,
        ));
    }

    // ── 3. Parse import / export sections ────────────────────────────────────
    let (imports, exports) = parse_wasm_sections(bytes);

    // ── 4. Forbidden imports ─────────────────────────────────────────────────
    for forbidden in &policy.forbidden_imports {
        for import in &imports {
            if import.contains(forbidden.as_str()) {
                violations.push(PreflightViolation {
                    code: "FORBIDDEN_IMPORT".to_string(),
                    message: format!(
                        "Module imports forbidden symbol '{}' (found in '{}'). \
                         Soroban contracts must not use WASI or OS-level host functions.",
                        forbidden, import
                    ),
                });
                break; // one violation per forbidden name is enough
            }
        }
    }

    // ── 5. Required exports ──────────────────────────────────────────────────
    for required in &policy.required_exports {
        if !exports.iter().any(|e| e.contains(required.as_str())) {
            violations.push(PreflightViolation {
                code: "MISSING_EXPORT".to_string(),
                message: format!(
                    "Module must export '{}' but it was not found in the export section.",
                    required
                ),
            });
        }
    }

    let mut findings = Vec::new();

    // ── 6. Unexpected imports ────────────────────────────────────────────────
    if let Some(allowed) = &policy.allowed_imports {
        for import in &imports {
            let ns = import.split("::").next().unwrap_or(import);
            let is_allowed = allowed
                .iter()
                .any(|a| ns == a || import == a || import.starts_with(&format!("{}::", a)));
            if !is_allowed {
                findings.push(PreflightFinding {
                    risk: "Medium".to_string(),
                    message: format!("Module imports '{}' which is not in the allowlist", import),
                });
            }
        }
    }

    // ── 7. Unexpected exports ────────────────────────────────────────────────
    if let Some(allowed) = &policy.allowed_exports {
        for export in &exports {
            let is_allowed = allowed.iter().any(|a| export == a || export.starts_with(a));
            if !is_allowed {
                findings.push(PreflightFinding {
                    risk: "Low".to_string(),
                    message: format!("Module exports '{}' which is not in the allowlist", export),
                });
            }
        }
    }

    let passes_policy = violations.is_empty();
    PreflightReport {
        path: label.to_string(),
        size_bytes: bytes.len(),
        is_valid_wasm: true,
        passes_policy,
        violations,
        findings,
        warnings,
        imports,
        exports,
    }
}

/// Lightweight extraction of import and export names from a WASM binary.
///
/// This is a best-effort section scanner, not a full WASM parser. It handles
/// correctly formed binaries and degrades gracefully on malformed input.
fn parse_wasm_sections(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut imports = Vec::new();
    let mut exports = Vec::new();

    if bytes.len() < 8 {
        return (imports, exports);
    }

    let mut pos = 8usize; // skip 4-byte magic + 4-byte version

    while pos < bytes.len() {
        let section_id = bytes[pos];
        pos += 1;

        let (section_size, consumed) = read_leb128_u32(&bytes[pos..]);
        pos += consumed;
        if consumed == 0 || pos + section_size as usize > bytes.len() {
            break;
        }
        let section_end = pos + section_size as usize;
        let section_bytes = &bytes[pos..section_end];

        match section_id {
            2 => imports = parse_import_names(section_bytes),
            7 => exports = parse_export_names(section_bytes),
            _ => {}
        }

        pos = section_end;
    }

    (imports, exports)
}

/// Parse the import section: returns `"module::name"` strings.
fn parse_import_names(data: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut pos = 0usize;

    let (count, consumed) = read_leb128_u32(data);
    pos += consumed;

    for _ in 0..count {
        if pos >= data.len() {
            break;
        }
        // module name
        let (mod_len, c) = read_leb128_u32(&data[pos..]);
        pos += c;
        if pos + mod_len as usize > data.len() {
            break;
        }
        let mod_name = std::str::from_utf8(&data[pos..pos + mod_len as usize])
            .unwrap_or("<invalid>")
            .to_string();
        pos += mod_len as usize;

        // field name
        let (field_len, c) = read_leb128_u32(&data[pos..]);
        pos += c;
        if pos + field_len as usize > data.len() {
            break;
        }
        let field_name = std::str::from_utf8(&data[pos..pos + field_len as usize])
            .unwrap_or("<invalid>")
            .to_string();
        pos += field_len as usize;

        names.push(format!("{}::{}", mod_name, field_name));

        // skip import descriptor (kind byte + type index encoded as LEB128)
        if pos < data.len() {
            pos += 1; // kind
            let (_, skip) = read_leb128_u32(&data[pos..]);
            pos += skip;
        }
    }

    names
}

/// Parse the export section: returns export name strings.
fn parse_export_names(data: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut pos = 0usize;

    let (count, consumed) = read_leb128_u32(data);
    pos += consumed;

    for _ in 0..count {
        if pos >= data.len() {
            break;
        }
        let (name_len, c) = read_leb128_u32(&data[pos..]);
        pos += c;
        if pos + name_len as usize > data.len() {
            break;
        }
        let name = std::str::from_utf8(&data[pos..pos + name_len as usize])
            .unwrap_or("<invalid>")
            .to_string();
        pos += name_len as usize;
        names.push(name);

        // skip export descriptor (kind byte + index LEB128)
        if pos < data.len() {
            pos += 1;
            let (_, skip) = read_leb128_u32(&data[pos..]);
            pos += skip;
        }
    }

    names
}

/// Decode one unsigned LEB128-encoded u32.
/// Returns `(value, bytes_consumed)`. Returns `(0, 0)` on empty input.
fn read_leb128_u32(data: &[u8]) -> (u32, usize) {
    let mut result: u32 = 0;
    let mut shift = 0u32;
    let mut pos = 0usize;
    loop {
        if pos >= data.len() || shift > 28 {
            break;
        }
        let byte = data[pos];
        pos += 1;
        result |= ((byte & 0x7f) as u32) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    (result, pos)
}

// ── Section breakdown & size budgets (#804) ──────────────────────────────────

/// WASM section id for function bodies.
pub const WASM_SECTION_CODE_ID: u8 = 10;
/// WASM section id for embedded data.
pub const WASM_SECTION_DATA_ID: u8 = 11;

/// One WASM section as observed in the binary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmSectionInfo {
    /// Raw section id (0 = custom, 10 = code, 11 = data, …).
    pub id: u8,
    /// Human-readable section kind ("custom", "type", "code", "data", …).
    pub name: String,
    /// For custom sections (id 0), the embedded name field (e.g. "name",
    /// "producers") — where debug info and toolchain metadata hide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_name: Option<String>,
    /// Bytes the section occupies in the file: id byte + size LEB128 + payload.
    pub size_bytes: usize,
}

/// Size of a WASM module split by section category.
///
/// The four category fields partition the whole module: the 8-byte header
/// counts as `other`, and every section byte belongs to exactly one
/// category, so the four fields always sum to `total_bytes`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmSectionBreakdown {
    /// Whole module size (header + all sections).
    pub total_bytes: usize,
    /// Bytes in code sections (id 10) — function bodies.
    pub code_bytes: usize,
    /// Bytes in data sections (id 11) — embedded data and static strings.
    pub data_bytes: usize,
    /// Bytes in custom sections (id 0) — names, producers, debug info.
    pub custom_bytes: usize,
    /// Everything else (header, type/import/function/memory/global/export/…).
    pub other_bytes: usize,
    /// Per-section detail in file order.
    pub sections: Vec<WasmSectionInfo>,
}

fn section_kind_name(id: u8) -> &'static str {
    match id {
        0 => "custom",
        1 => "type",
        2 => "import",
        3 => "function",
        4 => "table",
        5 => "memory",
        6 => "global",
        7 => "export",
        8 => "start",
        9 => "element",
        10 => "code",
        11 => "data",
        12 => "data-count",
        13 => "tag",
        _ => "unknown",
    }
}

/// Best-effort extraction of a custom section's name from its payload
/// (LEB128 length + UTF-8 bytes). Returns `None` on malformed input.
fn custom_section_name(payload: &[u8]) -> Option<String> {
    let (name_len, consumed) = read_leb128_u32(payload);
    if consumed == 0 {
        return None;
    }
    let start = consumed;
    let end = start + name_len as usize;
    if end > payload.len() {
        return None;
    }
    std::str::from_utf8(&payload[start..end])
        .ok()
        .map(|s| s.to_string())
}

/// Walk a WASM module's sections and categorize their sizes.
///
/// Best-effort like the import/export scanner: correctly formed binaries are
/// fully categorized, malformed input yields a partial breakdown instead of
/// an error (callers decide whether to fail on `is_valid_wasm` first).
pub fn wasm_section_breakdown(bytes: &[u8]) -> WasmSectionBreakdown {
    let mut breakdown = WasmSectionBreakdown {
        total_bytes: bytes.len(),
        code_bytes: 0,
        data_bytes: 0,
        custom_bytes: 0,
        other_bytes: 0,
        sections: Vec::new(),
    };

    if bytes.len() < 8 || &bytes[..4] != b"\0asm" {
        // Not a module: the whole input is uncategorized. Keep the sections
        // empty; callers gate on their own validity checks first.
        breakdown.other_bytes = bytes.len();
        return breakdown;
    }

    // The 8-byte header belongs to no section; count it as "other" so the
    // categories always sum to the module size.
    breakdown.other_bytes = 8;

    let mut pos = 8usize; // 4-byte magic + 4-byte version
    while pos < bytes.len() {
        let section_start = pos;
        let section_id = bytes[pos];
        pos += 1;

        let (section_size, consumed) = read_leb128_u32(&bytes[pos..]);
        if consumed == 0 {
            // Truncated size field: count the remaining tail as "other" so the
            // categories still sum to the module size, then stop.
            breakdown.other_bytes += bytes.len() - section_start;
            break;
        }
        pos += consumed;
        if pos + section_size as usize > bytes.len() {
            breakdown.other_bytes += bytes.len() - section_start;
            break;
        }
        let payload = &bytes[pos..pos + section_size as usize];
        pos += section_size as usize;

        let section_bytes = pos - section_start;
        let custom_name = if section_id == 0 {
            custom_section_name(payload)
        } else {
            None
        };

        match section_id {
            WASM_SECTION_CODE_ID => breakdown.code_bytes += section_bytes,
            WASM_SECTION_DATA_ID => breakdown.data_bytes += section_bytes,
            0 => breakdown.custom_bytes += section_bytes,
            _ => breakdown.other_bytes += section_bytes,
        }
        breakdown.sections.push(WasmSectionInfo {
            id: section_id,
            name: section_kind_name(section_id).to_string(),
            custom_name,
            size_bytes: section_bytes,
        });
    }

    breakdown
}

/// Configurable WASM size budgets for a project or pipeline.
///
/// `deny_unknown_fields` makes typos in the budget file fail loudly instead
/// of silently leaving a limit unset. Any field left out is simply unchecked
/// (except `max_total_bytes`, which defaults to the Soroban ceiling).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WasmSizeBudget {
    /// Maximum total module size in bytes. Defaults to the Soroban 128 KiB
    /// on-chain limit.
    pub max_total_bytes: Option<usize>,
    /// Maximum combined code-section (id 10) bytes.
    pub max_code_bytes: Option<usize>,
    /// Maximum combined data-section (id 11) bytes.
    pub max_data_bytes: Option<usize>,
    /// Maximum combined custom-section (id 0) bytes.
    pub max_custom_bytes: Option<usize>,
}

impl Default for WasmSizeBudget {
    fn default() -> Self {
        Self {
            max_total_bytes: Some(WASM_SIZE_LIMIT_BYTES),
            max_code_bytes: None,
            max_data_bytes: None,
            max_custom_bytes: None,
        }
    }
}

/// Parse a [`WasmSizeBudget`] from TOML, rejecting unknown keys.
pub fn parse_budget_str(contents: &str) -> Result<WasmSizeBudget> {
    toml::from_str(contents).context("Invalid WASM size budget TOML")
}

/// Load a [`WasmSizeBudget`] from a TOML file.
pub fn load_budget_file(path: &Path) -> Result<WasmSizeBudget> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read size budget file {:?}", path))?;
    parse_budget_str(&contents).with_context(|| format!("Invalid size budget file {:?}", path))
}

/// One budget line-item and how the module measured against it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetCheckEntry {
    /// Which budget was checked: "total", "code", "data", or "custom".
    pub label: String,
    pub actual_bytes: usize,
    pub budget_bytes: usize,
    /// How far over the budget (0 when within).
    pub overage_bytes: usize,
    pub within_budget: bool,
}

/// Result of checking a module against a [`WasmSizeBudget`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetReport {
    /// True when every configured budget is met.
    pub passed: bool,
    /// One entry per configured budget, in a stable order.
    pub entries: Vec<BudgetCheckEntry>,
    /// Actionable next commands for budgets that failed.
    pub suggestions: Vec<String>,
}

/// Compare a module's section breakdown against a budget.
///
/// Only budgets that are set produce entries; the Soroban total-size default
/// applies unless `max_total_bytes` is explicitly overridden.
pub fn check_size_budget(
    breakdown: &WasmSectionBreakdown,
    budget: &WasmSizeBudget,
) -> BudgetReport {
    let mut entries = Vec::new();

    let mut push = |label: &str, actual: usize, limit: usize| {
        entries.push(BudgetCheckEntry {
            label: label.to_string(),
            actual_bytes: actual,
            budget_bytes: limit,
            overage_bytes: actual.saturating_sub(limit),
            within_budget: actual <= limit,
        });
    };

    if let Some(limit) = budget.max_total_bytes {
        push("total", breakdown.total_bytes, limit);
    }
    if let Some(limit) = budget.max_code_bytes {
        push("code", breakdown.code_bytes, limit);
    }
    if let Some(limit) = budget.max_data_bytes {
        push("data", breakdown.data_bytes, limit);
    }
    if let Some(limit) = budget.max_custom_bytes {
        push("custom", breakdown.custom_bytes, limit);
    }

    let passed = entries.iter().all(|e| e.within_budget);
    BudgetReport {
        passed,
        entries,
        suggestions: Vec::new(),
    }
}

/// Fill a [`BudgetReport`] with deterministic, actionable next commands.
///
/// Suggestions are derived only from the budgets that actually failed, so a
/// code-section overage does not nag about data strings and vice versa.
/// A module that passes every budget gets no suggestions (callers show a
/// success line instead).
pub fn add_budget_suggestions(
    wasm_label: &str,
    breakdown: &WasmSectionBreakdown,
    report: &mut BudgetReport,
) {
    if report.passed {
        return;
    }
    let mut suggestions = Vec::new();
    let failed = |label: &str| {
        report
            .entries
            .iter()
            .any(|e| e.label == label && !e.within_budget)
    };

    if failed("total") {
        suggestions.push(format!(
            "Run `starforge gas optimize --target {wasm} --output {wasm}.opt` for a lightweight shrink pass.",
            wasm = wasm_label
        ));
    }
    if failed("custom") {
        suggestions.push(
            "Strip debug/toolchain metadata: `wasm-opt --strip-debug --strip-producers` \
             (or add `strip = true` to [profile.release] and rebuild)."
                .to_string(),
        );
    }
    if failed("code") {
        suggestions.push(format!(
            "Inspect per-issue findings: `starforge optimize analyse --wasm {}`.",
            wasm_label
        ));
        suggestions.push(
            "Shrink function bodies: `starforge optimize transform --src <contract.rs> --dry-run`, \
             plus `lto = true`, `codegen-units = 1`, and `opt-level = \"z\"` in [profile.release]."
                .to_string(),
        );
    }
    if failed("data") {
        suggestions.push(
            "Trim embedded data: shorten error strings and remove unused static data \
             (see docs/GAS_OPTIMIZATION_GUIDE.md, section \"Binary Size Reduction\")."
                .to_string(),
        );
    }

    report.suggestions = suggestions;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_wasm() -> Vec<u8> {
        // magic (4) + version (4) = empty valid WASM module
        vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
    }

    fn default_policy() -> WasmPolicy {
        WasmPolicy::default()
    }

    // ── Primary flow ─────────────────────────────────────────────────────────

    #[test]
    fn valid_minimal_wasm_passes() {
        let report = validate_wasm_bytes(&minimal_wasm(), "test.wasm", &default_policy());
        assert!(report.is_valid_wasm);
        assert!(report.passes_policy);
        assert!(report.violations.is_empty());
        assert!(report.is_ok());
    }

    // ── Boundary cases ────────────────────────────────────────────────────────

    #[test]
    fn wasm_at_exact_limit_passes() {
        let mut bytes = minimal_wasm();
        // Fill to exactly the limit (the 8 header bytes count too)
        bytes.extend(vec![0u8; WASM_SIZE_LIMIT_BYTES - bytes.len()]);
        let report = validate_wasm_bytes(&bytes, "at_limit.wasm", &default_policy());
        assert!(report.is_valid_wasm);
        assert!(
            report.violations.iter().all(|v| v.code != "SIZE_EXCEEDED"),
            "exact-limit should not trigger SIZE_EXCEEDED"
        );
    }

    #[test]
    fn wasm_near_limit_emits_warning() {
        let mut bytes = minimal_wasm();
        // 110 KiB > 85% of 128 KiB but ≤ 128 KiB
        bytes.extend(vec![0u8; 110 * 1024]);
        let report = validate_wasm_bytes(&bytes, "near_limit.wasm", &default_policy());
        assert!(report.is_valid_wasm);
        assert!(report.passes_policy, "should still pass policy");
        assert!(!report.warnings.is_empty(), "should emit a warning");
    }

    #[test]
    fn wasm_one_byte_over_limit_fails() {
        let mut bytes = minimal_wasm();
        bytes.extend(vec![0u8; WASM_SIZE_LIMIT_BYTES + 1]);
        let report = validate_wasm_bytes(&bytes, "too_big.wasm", &default_policy());
        assert!(!report.is_ok());
        assert!(report.violations.iter().any(|v| v.code == "SIZE_EXCEEDED"));
    }

    // ── Failure cases ─────────────────────────────────────────────────────────

    #[test]
    fn invalid_magic_fails() {
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x00, 0x00, 0x00];
        let report = validate_wasm_bytes(&bytes, "bad.wasm", &default_policy());
        assert!(!report.is_valid_wasm);
        assert!(!report.is_ok());
        assert_eq!(report.violations[0].code, "INVALID_MAGIC");
    }

    #[test]
    fn empty_bytes_fail() {
        let report = validate_wasm_bytes(&[], "empty.wasm", &default_policy());
        assert!(!report.is_valid_wasm);
        assert_eq!(report.violations[0].code, "INVALID_MAGIC");
    }

    #[test]
    fn required_export_missing_fails() {
        let mut policy = default_policy();
        policy.required_exports = vec!["__invoke".to_string()];
        let report = validate_wasm_bytes(&minimal_wasm(), "t.wasm", &policy);
        assert!(!report.is_ok());
        assert!(report.violations.iter().any(|v| v.code == "MISSING_EXPORT"));
    }

    #[test]
    fn no_forbidden_imports_policy_always_passes() {
        let mut policy = default_policy();
        policy.forbidden_imports = vec![];
        let report = validate_wasm_bytes(&minimal_wasm(), "t.wasm", &policy);
        assert!(report.is_ok());
    }

    // ── LEB128 codec ──────────────────────────────────────────────────────────

    #[test]
    fn leb128_single_byte() {
        assert_eq!(read_leb128_u32(&[0x05]), (5, 1));
    }

    #[test]
    fn leb128_multi_byte() {
        // 300 = [0xAC, 0x02]
        assert_eq!(read_leb128_u32(&[0xAC, 0x02]), (300, 2));
    }

    #[test]
    fn leb128_empty() {
        assert_eq!(read_leb128_u32(&[]), (0, 0));
    }

    // ── File validation ───────────────────────────────────────────────────────

    #[test]
    fn validate_file_not_found_returns_err() {
        let result = validate_wasm_file(
            std::path::Path::new("/nonexistent/path/contract.wasm"),
            &default_policy(),
        );
        assert!(result.is_err());
    }

    // ── Section breakdown (#804) ─────────────────────────────────────────────

    /// Minimal LEB128 encoding for section sizes used in fixtures.
    fn leb(n: u32) -> Vec<u8> {
        assert!(n < 128, "fixture helper only encodes single-byte LEB128");
        vec![n as u8]
    }

    /// Append one section (id byte + size + payload) to a module skeleton.
    fn section(id: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![id];
        out.extend(leb(payload.len() as u32));
        out.extend_from_slice(payload);
        out
    }

    /// A module with one custom ("name"), one code, and one data section.
    fn fixture_module() -> Vec<u8> {
        let mut bytes = minimal_wasm();
        // Custom section: name field "name" + arbitrary metadata payload.
        let mut custom_payload = vec![4];
        custom_payload.extend_from_slice(b"name");
        custom_payload.extend_from_slice(b"abc");
        bytes.extend(section(0, &custom_payload));
        bytes.extend(section(10, &[0xAA; 5]));
        bytes.extend(section(11, &[0xBB; 7]));
        bytes
    }

    #[test]
    fn breakdown_header_only_module() {
        let b = wasm_section_breakdown(&minimal_wasm());
        assert_eq!(b.total_bytes, 8);
        assert_eq!(b.other_bytes, 8);
        assert_eq!(
            b.code_bytes + b.data_bytes + b.custom_bytes + b.other_bytes,
            b.total_bytes
        );
        assert!(b.sections.is_empty());
    }

    #[test]
    fn breakdown_categorizes_code_data_custom() {
        let bytes = fixture_module();
        let b = wasm_section_breakdown(&bytes);
        // 1 id byte + 1 size byte + payload
        assert_eq!(b.code_bytes, 2 + 5);
        assert_eq!(b.data_bytes, 2 + 7);
        assert_eq!(b.custom_bytes, 2 + 1 + 4 + 3);
        assert_eq!(b.other_bytes, 8);
        assert_eq!(b.total_bytes, bytes.len());
        assert_eq!(
            b.code_bytes + b.data_bytes + b.custom_bytes + b.other_bytes,
            b.total_bytes
        );
    }

    #[test]
    fn breakdown_records_section_metadata_in_file_order() {
        let b = wasm_section_breakdown(&fixture_module());
        let ids: Vec<u8> = b.sections.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![0, 10, 11]);
        let names: Vec<&str> = b.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["custom", "code", "data"]);
        assert_eq!(b.sections[0].custom_name.as_deref(), Some("name"));
        assert_eq!(b.sections[1].custom_name, None);
    }

    #[test]
    fn breakdown_survives_truncated_section_size() {
        let mut bytes = fixture_module();
        // Trailing section whose size LEB128 never terminates.
        bytes.extend_from_slice(&[12, 0x80, 0x80]);
        let b = wasm_section_breakdown(&bytes);
        // The categorized prefix is unchanged…
        assert_eq!(b.code_bytes, 2 + 5);
        // …and the categories still sum to the module size.
        assert_eq!(
            b.code_bytes + b.data_bytes + b.custom_bytes + b.other_bytes,
            b.total_bytes
        );
        assert_eq!(b.total_bytes, bytes.len());
    }

    #[test]
    fn breakdown_non_module_counts_everything_as_other() {
        let b = wasm_section_breakdown(&[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(b.total_bytes, 4);
        assert_eq!(b.other_bytes, 4);
        assert!(b.sections.is_empty());
    }

    // ── Size budgets (#804) ───────────────────────────────────────────────────

    #[test]
    fn default_budget_checks_total_only() {
        let budget = WasmSizeBudget::default();
        let b = wasm_section_breakdown(&minimal_wasm());
        let report = check_size_budget(&b, &budget);
        assert!(report.passed);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].label, "total");
        assert_eq!(report.entries[0].budget_bytes, WASM_SIZE_LIMIT_BYTES);
        assert!(report.suggestions.is_empty());
    }

    #[test]
    fn budget_without_limits_produces_no_entries() {
        let budget = WasmSizeBudget {
            max_total_bytes: None,
            max_code_bytes: None,
            max_data_bytes: None,
            max_custom_bytes: None,
        };
        let b = wasm_section_breakdown(&fixture_module());
        let report = check_size_budget(&b, &budget);
        assert!(report.passed);
        assert!(report.entries.is_empty());
    }

    #[test]
    fn budget_total_failure_reports_overage() {
        let bytes = fixture_module();
        let budget = WasmSizeBudget {
            max_total_bytes: Some(20),
            ..WasmSizeBudget::default()
        };
        let b = wasm_section_breakdown(&bytes);
        let report = check_size_budget(&b, &budget);
        assert!(!report.passed);
        let total = report.entries.iter().find(|e| e.label == "total").unwrap();
        assert_eq!(total.overage_bytes, total.actual_bytes - 20);
        assert_eq!(total.actual_bytes, bytes.len());
    }

    #[test]
    fn budget_failure_at_exact_limit_still_passes() {
        let bytes = fixture_module();
        let budget = WasmSizeBudget {
            max_total_bytes: Some(bytes.len()),
            ..WasmSizeBudget::default()
        };
        let report = check_size_budget(&wasm_section_breakdown(&bytes), &budget);
        assert!(report.passed);
    }

    #[test]
    fn code_budget_failure_suggests_code_commands_only() {
        let budget = WasmSizeBudget {
            max_total_bytes: None,
            max_code_bytes: Some(4),
            max_data_bytes: None,
            max_custom_bytes: None,
        };
        let b = wasm_section_breakdown(&fixture_module());
        let mut report = check_size_budget(&b, &budget);
        assert!(!report.passed);
        add_budget_suggestions("contract.wasm", &b, &mut report);
        assert_eq!(report.suggestions.len(), 2);
        assert!(report.suggestions[0].contains("optimize analyse --wasm contract.wasm"));
        assert!(report.suggestions[1].contains("optimize transform"));
        // No data/custom advice for a code overage.
        assert!(!report.suggestions.iter().any(|s| s.contains("strip-debug")));
        assert!(!report
            .suggestions
            .iter()
            .any(|s| s.contains("error strings")));
    }

    #[test]
    fn custom_budget_failure_suggests_stripping_metadata() {
        let budget = WasmSizeBudget {
            max_total_bytes: None,
            max_code_bytes: None,
            max_data_bytes: None,
            max_custom_bytes: Some(4),
        };
        let b = wasm_section_breakdown(&fixture_module());
        let mut report = check_size_budget(&b, &budget);
        assert!(!report.passed);
        add_budget_suggestions("c.wasm", &b, &mut report);
        assert!(report
            .suggestions
            .iter()
            .any(|s| s.contains("--strip-debug")));
    }

    #[test]
    fn data_budget_failure_links_optimization_guide() {
        let budget = WasmSizeBudget {
            max_total_bytes: None,
            max_code_bytes: None,
            max_data_bytes: Some(4),
            max_custom_bytes: None,
        };
        let b = wasm_section_breakdown(&fixture_module());
        let mut report = check_size_budget(&b, &budget);
        assert!(!report.passed);
        add_budget_suggestions("c.wasm", &b, &mut report);
        assert!(report
            .suggestions
            .iter()
            .any(|s| s.contains("GAS_OPTIMIZATION_GUIDE.md")));
    }

    #[test]
    fn total_budget_failure_suggests_gas_optimize() {
        let budget = WasmSizeBudget::default();
        let b = wasm_section_breakdown(&fixture_module());
        let mut report = check_size_budget(&b, &budget);
        report.entries[0].within_budget = false;
        report.passed = false;
        add_budget_suggestions("c.wasm", &b, &mut report);
        assert!(report
            .suggestions
            .iter()
            .any(|s| s.contains("gas optimize")));
    }

    #[test]
    fn passing_budget_gets_no_suggestions() {
        let budget = WasmSizeBudget::default();
        let b = wasm_section_breakdown(&minimal_wasm());
        let mut report = check_size_budget(&b, &budget);
        assert!(report.passed);
        add_budget_suggestions("c.wasm", &b, &mut report);
        assert!(report.suggestions.is_empty());
    }

    #[test]
    fn parse_budget_rejects_unknown_keys() {
        let result = parse_budget_str("max_total_bytes = 1024\nbogus = 1");
        assert!(result.is_err());
    }

    #[test]
    fn parse_budget_accepts_section_limits() {
        let budget = parse_budget_str(
            "max_code_bytes = 65536\nmax_data_bytes = 4096\nmax_custom_bytes = 1024\n",
        )
        .unwrap();
        assert_eq!(budget.max_code_bytes, Some(65536));
        assert_eq!(budget.max_data_bytes, Some(4096));
        assert_eq!(budget.max_custom_bytes, Some(1024));
        // Total falls back to the Soroban ceiling.
        assert_eq!(budget.max_total_bytes, Some(WASM_SIZE_LIMIT_BYTES));
    }
}
