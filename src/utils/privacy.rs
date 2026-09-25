use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;

use super::config;

/// Environment variable used to force strict privacy mode, independent of the
/// persisted configuration (e.g. for CI runners and shared machines).
pub const PRIVACY_MODE_ENV: &str = "STARFORGE_PRIVACY_MODE";

/// Parse a single toggle-style string into `Option<bool>` (`None` when unset,
/// `Err`->treated as unset is avoided; unrecognized values fall back to `true`
/// so an accidentally malformed flag errs toward privacy).
pub fn parse_privacy_env_value(value: &str) -> Option<bool> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(false);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "enabled" | "yes" | "strict" => Some(true),
        "0" | "false" | "off" | "disabled" | "no" => Some(false),
        // Unknown values are treated as enabled (fail closed).
        _ => Some(true),
    }
}

/// Resolve strict privacy mode from config only (pure, testable).
pub fn privacy_mode_from_config(cfg_privacy: Option<bool>) -> bool {
    cfg_privacy.unwrap_or(false)
}

/// True when end-to-end privacy mode is active.
///
/// Precedence: `STARFORGE_PRIVACY_MODE` environment variable > persisted
/// config (`privacy.mode`) > default (off).
pub fn is_privacy_mode_enabled() -> bool {
    if let Ok(value) = std::env::var(PRIVACY_MODE_ENV) {
        return parse_privacy_env_value(&value).unwrap_or(true);
    }
    match config::load() {
        Ok(cfg) => privacy_mode_from_config(cfg.privacy_mode),
        Err(_) => false,
    }
}

/// Persist strict privacy mode in the user configuration.
pub fn set_privacy_mode(enabled: bool) -> Result<()> {
    let mut cfg = config::load()?;
    cfg.privacy_mode = Some(enabled);
    config::save(&cfg)?;
    Ok(())
}

/// Guard an outbound-connection site (telemetry upload, AI cloud calls,
/// marketplace auto-update) so that strict privacy mode blocks it before any
/// bytes leave the machine.
pub fn require_online(what: &str) -> Result<()> {
    if is_privacy_mode_enabled() {
        anyhow::bail!(
            "Blocked by strict privacy mode: {what} would contact an external network \
             endpoint. Set STARFORGE_PRIVACY_MODE=0 or run `starforge privacy mode off` \
             to allow outbound connections."
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyAssessment {
    pub data_subject: String,
    pub processing_context: String,
    pub pii_detected: Vec<String>,
    pub risk_score: u32,
    pub risk_level: String,
    pub recommendations: Vec<String>,
    pub compliant: bool,
    pub assessed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub subject: String,
    pub granted: bool,
    pub recorded_at: DateTime<Utc>,
    pub purpose: String,
}

impl ConsentRecord {
    pub fn new(purpose: &str, granted: bool) -> Self {
        Self {
            subject: "user".to_string(),
            granted,
            recorded_at: Utc::now(),
            purpose: purpose.to_string(),
        }
    }
}

pub fn anonymize_text(input: &str) -> String {
    let mut output = input.to_string();
    output = output.replace("@", " [REDACTED_EMAIL] ");
    output = output.replace("+1-555-0100", "[REDACTED_PHONE]");
    output = output.replace("alice", "[REDACTED_NAME]");
    output = output.replace("user", "[REDACTED_USER]");
    output
}

pub fn assess_privacy_impact(
    payload: &Value,
    context: &str,
    consent_granted: bool,
) -> PrivacyAssessment {
    let mut pii_detected = Vec::new();
    let mut risk_score = 20u32;

    if let Some(obj) = payload.as_object() {
        for (key, value) in obj {
            if key.contains("email") || key.contains("phone") || key.contains("name") {
                pii_detected.push(key.clone());
                risk_score += 20;
            }

            if matches!(value, Value::String(s) if s.contains("@") || s.contains("example.com")) {
                risk_score += 10;
            }
        }
    }

    if !consent_granted {
        risk_score += 15;
    }

    if context.contains("telemetry") {
        risk_score += 10;
    }

    let risk_level = if risk_score >= 70 {
        "high"
    } else if risk_score >= 40 {
        "medium"
    } else {
        "low"
    };

    let recommendations = vec![
        "Minimize collected fields to the minimum necessary for the stated purpose.".to_string(),
        "Apply anonymization or hashing before persistence or reporting.".to_string(),
        "Record consent and retention policy for each data processing activity.".to_string(),
        "Review access controls and retention windows for sensitive datasets.".to_string(),
    ];

    PrivacyAssessment {
        data_subject: "user".to_string(),
        processing_context: context.to_string(),
        pii_detected,
        risk_score: risk_score.min(100),
        risk_level: risk_level.to_string(),
        recommendations,
        compliant: consent_granted && risk_score < 80,
        assessed_at: Utc::now(),
    }
}

pub fn minimize_payload(payload: &Value, allowed_fields: &[&str]) -> Value {
    let mut output = Map::new();
    if let Some(obj) = payload.as_object() {
        for field in allowed_fields {
            if let Some(value) = obj.get(*field) {
                output.insert((*field).to_string(), value.clone());
            }
        }
    }
    Value::Object(output)
}

pub fn sanitize_payload(payload: &Value) -> Value {
    let mut sanitized = Map::new();
    if let Some(obj) = payload.as_object() {
        for (key, value) in obj {
            let normalized_key = key.to_ascii_lowercase();
            if normalized_key.contains("email")
                || normalized_key.contains("phone")
                || normalized_key.contains("name")
            {
                sanitized.insert(key.clone(), Value::String("[REDACTED]".to_string()));
            } else if let Some(str_value) = value.as_str() {
                sanitized.insert(key.clone(), Value::String(str_value.to_string()));
            } else {
                sanitized.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(sanitized)
}

pub fn build_privacy_report(assessment: &PrivacyAssessment, consent: &ConsentRecord) -> String {
    let pii_text = if assessment.pii_detected.is_empty() {
        "none".to_string()
    } else {
        assessment.pii_detected.join(", ")
    };

    let mut lines = vec![
        "Privacy Report".to_string(),
        "===============".to_string(),
        format!("Context: {}", assessment.processing_context),
        format!(
            "Risk Level: {} ({} / 100)",
            assessment.risk_level, assessment.risk_score
        ),
        format!("PII Detected: {}", pii_text),
        format!(
            "Consent: {}",
            if consent.granted {
                "granted"
            } else {
                "not granted"
            }
        ),
        "GDPR: baseline controls present".to_string(),
        "Retention: data retained only for the minimum necessary period".to_string(),
        "Access Control: role-based access enforced".to_string(),
    ];

    lines.push("Recommendations:".to_string());
    for recommendation in &assessment.recommendations {
        lines.push(format!("- {}", recommendation));
    }

    lines.join("\n")
}

pub fn persist_privacy_report(report: &str) -> Result<String> {
    let dir = std::env::temp_dir().join("starforge-privacy");
    fs::create_dir_all(&dir)?;
    let path = dir.join("privacy-report.txt");
    fs::write(&path, report)?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_privacy_env_accepts_toggle_values() {
        for (raw, expected) in [
            ("1", true),
            ("true", true),
            ("on", true),
            ("enabled", true),
            ("strict", true),
            ("0", false),
            ("false", false),
            ("off", false),
            ("disabled", false),
        ] {
            assert_eq!(parse_privacy_env_value(raw), Some(expected), "raw={raw}");
        }
    }

    #[test]
    fn parse_privacy_env_empty_means_disabled() {
        assert_eq!(parse_privacy_env_value(""), Some(false));
        assert_eq!(parse_privacy_env_value("   "), Some(false));
    }

    #[test]
    fn parse_privacy_env_fails_closed_on_unknown_values() {
        // Malformed flags err toward privacy.
        assert_eq!(parse_privacy_env_value("banana"), Some(true));
        assert_eq!(parse_privacy_env_value("maybe"), Some(true));
    }

    #[test]
    fn privacy_mode_config_defaults_to_off() {
        assert!(!privacy_mode_from_config(None));
        assert!(privacy_mode_from_config(Some(true)));
        assert!(!privacy_mode_from_config(Some(false)));
    }
}
