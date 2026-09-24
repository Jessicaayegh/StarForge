#![allow(dead_code, unused_imports)]

/// Integration tests for WASM pre-flight validation (#692)
/// Covers primary flow, boundary cases, and failure cases.
#[cfg(test)]
mod wasm_preflight_tests {
    use starforge::utils::wasm_preflight::{
        validate_wasm_bytes, validate_wasm_file, WasmPolicy, WASM_SIZE_LIMIT_BYTES,
    };
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn minimal_wasm() -> Vec<u8> {
        // Minimal valid WASM: 4-byte magic + 4-byte version (WASM 1.0)
        vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
    }

    fn default_policy() -> WasmPolicy {
        WasmPolicy::default()
    }

    // ── Primary flow ─────────────────────────────────────────────────────────

    #[test]
    fn valid_wasm_passes_default_policy() {
        let report = validate_wasm_bytes(&minimal_wasm(), "contract.wasm", &default_policy());
        assert!(
            report.is_valid_wasm,
            "minimal WASM should be recognised as valid"
        );
        assert!(
            report.passes_policy,
            "minimal WASM should pass the default policy"
        );
        assert!(report.violations.is_empty(), "no violations expected");
        assert!(report.is_ok(), "report.is_ok() must be true");
    }

    #[test]
    fn oversized_wasm_emits_size_exceeded_violation() {
        let mut bytes = minimal_wasm();
        bytes.extend(vec![0u8; WASM_SIZE_LIMIT_BYTES + 1]);
        let report = validate_wasm_bytes(&bytes, "big.wasm", &default_policy());
        assert!(report.is_valid_wasm, "should still be valid WASM");
        assert!(!report.is_ok(), "should fail policy");
        assert!(
            report.violations.iter().any(|v| v.code == "SIZE_EXCEEDED"),
            "expected SIZE_EXCEEDED violation"
        );
    }

    #[test]
    fn required_export_present_passes() {
        // Build a tiny WASM with an export section that exports "__invoke"
        // For simplicity we embed the raw bytes of a handcrafted minimal module.
        // Minimal WASM with one exported function named "__invoke":
        // (module (func) (export "__invoke" (func 0)))
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
            // type section (id=1, size=4): one type: () -> ()
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00,
            // function section (id=3, size=2): func 0 has type 0
            0x03, 0x02, 0x01, 0x00,
            // export section (id=7, size=10): export "__invoke" as func 0
            0x07, 0x0a, 0x01,
            0x08, // name length = 8
            b'_', b'_', b'i', b'n', b'v', b'o', b'k', b'e', // "__invoke"
            0x00, // kind = function
            0x00, // func index = 0
            // code section (id=10, size=4): one function body
            0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b,
        ];

        let mut policy = default_policy();
        policy.required_exports = vec!["__invoke".to_string()];
        let report = validate_wasm_bytes(&bytes, "contract.wasm", &policy);
        assert!(
            report.is_ok(),
            "should pass when required export is present"
        );
    }

    // ── Boundary cases ────────────────────────────────────────────────────────

    #[test]
    fn wasm_at_exact_limit_passes() {
        let header = minimal_wasm();
        let padding = WASM_SIZE_LIMIT_BYTES.saturating_sub(header.len());
        let mut bytes = header;
        bytes.extend(vec![0u8; padding]);
        // bytes.len() == WASM_SIZE_LIMIT_BYTES exactly
        let report = validate_wasm_bytes(&bytes, "exact.wasm", &default_policy());
        assert!(
            !report.violations.iter().any(|v| v.code == "SIZE_EXCEEDED"),
            "exact-limit size should NOT trigger SIZE_EXCEEDED"
        );
    }

    #[test]
    fn wasm_above_85_percent_limit_warns_but_passes_policy() {
        let mut bytes = minimal_wasm();
        // 110 KiB is > 85% of 128 KiB but ≤ 128 KiB
        bytes.extend(vec![0u8; 110 * 1024]);
        let report = validate_wasm_bytes(&bytes, "warn.wasm", &default_policy());
        assert!(report.is_valid_wasm);
        assert!(
            report.passes_policy,
            "should still pass policy (no violations)"
        );
        assert!(
            !report.warnings.is_empty(),
            "should produce a near-limit warning"
        );
    }

    #[test]
    fn custom_policy_no_forbidden_imports_always_ok() {
        let mut policy = default_policy();
        policy.forbidden_imports = vec![];
        let report = validate_wasm_bytes(&minimal_wasm(), "t.wasm", &policy);
        assert!(report.is_ok());
    }

    // ── Failure cases ─────────────────────────────────────────────────────────

    #[test]
    fn empty_bytes_fail_with_invalid_magic() {
        let report = validate_wasm_bytes(&[], "empty.wasm", &default_policy());
        assert!(!report.is_valid_wasm);
        assert_eq!(report.violations[0].code, "INVALID_MAGIC");
        assert!(!report.is_ok());
    }

    #[test]
    fn wrong_magic_fails() {
        let bytes = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x01, 0x00, 0x00, 0x00];
        let report = validate_wasm_bytes(&bytes, "jvm.class", &default_policy());
        assert!(!report.is_valid_wasm);
        assert_eq!(report.violations[0].code, "INVALID_MAGIC");
    }

    #[test]
    fn required_export_missing_fails() {
        let mut policy = default_policy();
        policy.required_exports = vec!["__invoke".to_string()];
        let report = validate_wasm_bytes(&minimal_wasm(), "t.wasm", &policy);
        assert!(!report.is_ok());
        assert!(
            report.violations.iter().any(|v| v.code == "MISSING_EXPORT"),
            "expected MISSING_EXPORT violation"
        );
    }

    #[test]
    fn validate_file_nonexistent_returns_io_error() {
        let result = validate_wasm_file(
            std::path::Path::new("/no/such/file/contract.wasm"),
            &default_policy(),
        );
        assert!(result.is_err(), "missing file should return Err");
    }

    #[test]
    fn validate_file_roundtrip_with_temp_file() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&minimal_wasm()).unwrap();
        let report = validate_wasm_file(tmp.path(), &default_policy()).unwrap();
        assert!(report.is_ok());
    }

    #[test]
    fn violation_message_mentions_size_limit() {
        let mut bytes = minimal_wasm();
        bytes.extend(vec![0u8; WASM_SIZE_LIMIT_BYTES + 100]);
        let report = validate_wasm_bytes(&bytes, "t.wasm", &default_policy());
        let size_viol = report
            .violations
            .iter()
            .find(|v| v.code == "SIZE_EXCEEDED")
            .expect("SIZE_EXCEEDED expected");
        assert!(
            size_viol.message.contains("128"),
            "violation message should mention the 128 KiB limit"
        );
    }

    #[test]
    fn unexpected_imports_generate_findings() {
        let bytes: Vec<u8> = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
            // import section (id=2, size=15)
            // 1 import: "bad" "import" func 0
            0x02, 0x0f, 0x01, 0x03, b'b', b'a', b'd', 0x06, b'i', b'm', b'p', b'o', b'r', b't',
            0x00, 0x00,
        ];

        let mut policy = default_policy();
        policy.allowed_imports = Some(vec!["good".to_string()]);

        let report = validate_wasm_bytes(&bytes, "t.wasm", &policy);
        assert!(
            report.is_ok(),
            "Findings should not cause is_ok to be false"
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].risk, "Medium");
        assert!(report.findings[0].message.contains("bad::import"));
    }
}
