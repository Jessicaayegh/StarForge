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
}
