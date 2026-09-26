//! Integration and schema validation tests for AI Quality Gate presets and PR bots.

use starforge::utils::ai_quality_gates::{
    evaluate, format_github_annotations, load_config, redact_report, write_preset_config,
    CustomGate, GateResult, QualityGateConfig, QualityGatePreset, QualityGateReport,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_all_presets_roundtrip_toml_serialization() {
    let temp = tempfile::tempdir().unwrap();

    for preset in [
        QualityGatePreset::Conservative,
        QualityGatePreset::Default,
        QualityGatePreset::Strict,
    ] {
        let file_path = temp.path().join(format!("gates_{:?}.toml", preset));
        write_preset_config(&file_path, preset).expect("Failed to write preset config");

        let loaded = load_config(&file_path).expect("Failed to load preset config");
        assert_eq!(loaded.preset, Some(preset));

        let generated_config = preset.config();
        assert_eq!(
            loaded.minimum_quality_score,
            generated_config.minimum_quality_score
        );
        assert_eq!(loaded.maximum_unwraps, generated_config.maximum_unwraps);
        assert_eq!(loaded.maximum_todos, generated_config.maximum_todos);
        assert_eq!(
            loaded.minimum_coverage_percent,
            generated_config.minimum_coverage_percent
        );
        assert_eq!(
            loaded.minimum_documentation_percent,
            generated_config.minimum_documentation_percent
        );
    }
}

#[test]
fn test_preset_threshold_hierarchies() {
    let conservative = QualityGatePreset::Conservative.config();
    let default = QualityGatePreset::Default.config();
    let strict = QualityGatePreset::Strict.config();

    // Strict is more demanding than Default, which is more demanding than Conservative
    assert!(conservative.minimum_quality_score < default.minimum_quality_score);
    assert!(default.minimum_quality_score < strict.minimum_quality_score);

    assert!(conservative.minimum_coverage_percent < default.minimum_coverage_percent);
    assert!(default.minimum_coverage_percent < strict.minimum_coverage_percent);

    assert!(conservative.maximum_unwraps > default.maximum_unwraps);
    assert_eq!(default.maximum_unwraps, 0);
    assert_eq!(strict.maximum_unwraps, 0);

    assert!(
        conservative.maximum_medium_security_findings >= default.maximum_medium_security_findings
    );
    assert_eq!(strict.maximum_medium_security_findings, 0);
}

#[test]
fn test_mock_project_evaluation_against_strict_and_conservative_presets() {
    let temp = tempfile::tempdir().unwrap();

    // Create a mock contract project with some unwraps and documentation
    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[package]
name = "mock-contract"
version = "0.1.0"
license = "MIT"
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("lib.rs"),
        r#"//! Mock contract library
pub fn calculate() -> u64 {
    // Contains unwrap
    let val: Option<u64> = Some(42);
    val.unwrap()
}
"#,
    )
    .unwrap();

    // Conservative should pass with 1 unwrap allowed
    let conservative = QualityGatePreset::Conservative.config();
    let rep_conservative = evaluate(temp.path(), &conservative, Some(60.0), None).unwrap();
    assert!(
        rep_conservative.passed,
        "Conservative preset should pass with 1 unwrap and 60% coverage"
    );

    // Strict should fail due to unwrap and coverage requirements
    let strict = QualityGatePreset::Strict.config();
    let rep_strict = evaluate(temp.path(), &strict, Some(60.0), None).unwrap();
    assert!(
        !rep_strict.passed,
        "Strict preset should fail when unwraps > 0 or coverage < 90%"
    );
}

#[test]
fn test_custom_gates_evaluation() {
    let temp = tempfile::tempdir().unwrap();

    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
    )
    .unwrap();

    fs::write(
        temp.path().join("lib.rs"),
        "pub fn run() -> Result<(), ()> { Ok(()) }\n",
    )
    .unwrap();

    let mut config = QualityGatePreset::Default.config();
    config.custom_gates.push(CustomGate {
        name: "check result signature".to_string(),
        rule: "contains".to_string(),
        value: "Result<".to_string(),
        required: true,
    });

    let report = evaluate(temp.path(), &config, Some(85.0), None).unwrap();
    let custom_res = report
        .results
        .iter()
        .find(|r| r.gate == "check result signature");
    assert!(custom_res.is_some());
    assert!(custom_res.unwrap().passed);
}

#[test]
fn test_github_annotations_formatting_and_secret_redaction() {
    let report = QualityGateReport {
        passed: false,
        project: PathBuf::from("contracts/vault"),
        quality_score: 65,
        coverage_percent: 75.0,
        documentation_percent: 70.0,
        results: vec![GateResult {
            category: "security".to_string(),
            gate: "audit finding".to_string(),
            passed: false,
            actual: "Secret key leaked: SCZANGBA5YHTNYVVV4C3U252E2B6P6IRKD4T2MHS574QWK5GNPONIN2K"
                .to_string(),
            expected: "No secrets".to_string(),
            remediation: "Revoke token ghp_999999999999999999999999999999999999 immediately"
                .to_string(),
        }],
        generated_at: "2026-09-24T12:00:00Z".to_string(),
    };

    let annotations = format_github_annotations(&report);
    assert_eq!(annotations.len(), 1);
    let annotation = &annotations[0];

    // Must start with GitHub Actions workflow command
    assert!(annotation.starts_with("::error file="));

    // Must NEVER leak secrets
    assert!(!annotation.contains("SCZANGBA5YHTNYVVV4C3U252E2B6P6IRKD4T2MHS574QWK5GNPONIN2K"));
    assert!(!annotation.contains("ghp_999999999999999999999999999999999999"));
    assert!(annotation.contains("[REDACTED]"));
}
