use crate::utils::{config, print as p, privacy};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PrivacyCommands {
    /// Analyze a JSON payload for privacy risks and PII exposure
    Assess { payload: String },
    /// Anonymize freeform text input
    Anonymize { text: String },
    /// Minimize a payload to a set of allowed fields
    Minimize {
        payload: String,
        fields: Vec<String>,
    },
    /// Generate a privacy report for the current assessment
    Report { payload: String },
    /// Turn end-to-end strict privacy mode on or off, or show its status
    Mode {
        /// on | off | status
        #[arg(value_parser = ["on", "off", "status"])]
        action: String,
    },
}

fn print_privacy_status() -> Result<()> {
    let enabled = privacy::is_privacy_mode_enabled();
    let cfg = config::load()?;
    p::kv(
        "Privacy mode",
        if enabled {
            "enabled (strict)"
        } else {
            "disabled"
        },
    );
    p::kv(
        "Config (privacy.mode)",
        &cfg.privacy_mode.unwrap_or(false).to_string(),
    );
    let env_raw = std::env::var(privacy::PRIVACY_MODE_ENV).ok();
    p::kv(
        "Environment (STARFORGE_PRIVACY_MODE)",
        env_raw.as_deref().unwrap_or("(unset)"),
    );
    p::info(
        "When enabled, telemetry export, AI cloud calls, and marketplace/registry \
         auto-updates are blocked: no bytes leave this machine.",
    );
    Ok(())
}

pub async fn handle(cmd: PrivacyCommands) -> Result<()> {
    match cmd {
        PrivacyCommands::Assess { payload } => {
            let parsed: serde_json::Value = serde_json::from_str(&payload)?;
            let assessment = privacy::assess_privacy_impact(&parsed, "cli", true);
            p::header("Privacy Assessment");
            p::kv("Risk Level", &assessment.risk_level);
            p::kv("Risk Score", &assessment.risk_score.to_string());
            p::kv("PII Detected", &assessment.pii_detected.join(", "));
            p::kv("Compliant", &assessment.compliant.to_string());
        }
        PrivacyCommands::Anonymize { text } => {
            let anonymized = privacy::anonymize_text(&text);
            p::header("Anonymized Output");
            println!("{}", anonymized);
        }
        PrivacyCommands::Minimize { payload, fields } => {
            let parsed: serde_json::Value = serde_json::from_str(&payload)?;
            let minimized = privacy::minimize_payload(
                &parsed,
                &fields.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            println!("{}", serde_json::to_string_pretty(&minimized)?);
        }
        PrivacyCommands::Report { payload } => {
            let parsed: serde_json::Value = serde_json::from_str(&payload)?;
            let assessment = privacy::assess_privacy_impact(&parsed, "report", true);
            let consent = privacy::ConsentRecord::new("report", true);
            let report = privacy::build_privacy_report(&assessment, &consent);
            let path = privacy::persist_privacy_report(&report)?;
            p::header("Privacy Report");
            println!("{}", report);
            p::kv("Saved To", &path);
        }
        PrivacyCommands::Mode { action } => match action.as_str() {
            "on" => {
                privacy::set_privacy_mode(true)?;
                p::success("Strict privacy mode enabled. Telemetry export, AI cloud calls and marketplace auto-update are now blocked.");
                print_privacy_status()?;
            }
            "off" => {
                privacy::set_privacy_mode(false)?;
                p::success("Strict privacy mode disabled.");
                print_privacy_status()?;
            }
            _ => {
                p::header("Strict Privacy Mode");
                print_privacy_status()?;
            }
        },
    }
    Ok(())
}
