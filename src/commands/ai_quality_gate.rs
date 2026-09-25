use crate::utils::{ai_quality_gates as gates, print as p};
use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AiQualityGateCommands {
    /// Create a documented quality-gate policy using a preset (conservative, default, strict)
    Init {
        #[arg(default_value = "starforge-gates.toml")]
        output: PathBuf,
        /// Preset threshold configuration (conservative, default, strict)
        #[arg(long, value_enum, default_value = "default")]
        preset: gates::QualityGatePreset,
    },
    /// Evaluate all configured quality gates; exits non-zero when a required gate fails
    Check {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, default_value = "starforge-gates.toml")]
        config: PathBuf,
        /// Override or specify a quality gate preset directly (conservative, default, strict)
        #[arg(long, value_enum)]
        preset: Option<gates::QualityGatePreset>,
        /// Measured line/branch coverage percentage from the CI coverage tool
        #[arg(long)]
        coverage: Option<f64>,
        /// Measured benchmark duration in milliseconds
        #[arg(long)]
        benchmark_ms: Option<f64>,
        #[arg(long)]
        json: bool,
        /// Emit GitHub Actions workflow annotations (`::error`) for PR bot automation
        #[arg(long)]
        github_annotations: bool,
        /// Write the JSON report while retaining human-readable terminal output
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

pub fn handle(command: AiQualityGateCommands) -> Result<()> {
    match command {
        AiQualityGateCommands::Init { output, preset } => {
            gates::write_preset_config(&output, preset)?;
            p::success(&format!(
                "Quality gate configuration ({:?} preset) written to {}",
                preset,
                output.display()
            ));
        }
        AiQualityGateCommands::Check {
            dir,
            config,
            preset,
            coverage,
            benchmark_ms,
            json,
            github_annotations,
            output,
        } => {
            let gate_config = if let Some(p) = preset {
                p.config()
            } else if config.exists() {
                gates::load_config(&config)?
            } else {
                gates::QualityGateConfig::default()
            };

            let report = gates::evaluate(&dir, &gate_config, coverage, benchmark_ms)?;
            let serialized = serde_json::to_string_pretty(&report)?;
            if let Some(path) = output {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, &serialized)?;
            }

            if github_annotations {
                for annotation in gates::format_github_annotations(&report) {
                    println!("{annotation}");
                }
            }

            if json {
                println!("{serialized}");
            } else {
                p::header("AI Quality Gates");
                for result in &report.results {
                    let marker = if result.passed {
                        "PASS".green().bold()
                    } else {
                        "FAIL".red().bold()
                    };
                    println!(
                        "  [{}] {:<15} {} (actual {}, expected {})",
                        marker, result.category, result.gate, result.actual, result.expected
                    );
                    if !result.passed {
                        println!("         {}", result.remediation.dimmed());
                    }
                }
                println!();
                p::kv("Quality score", &report.quality_score.to_string());
                p::kv("Coverage", &format!("{:.1}%", report.coverage_percent));
                p::kv(
                    "Documentation",
                    &format!("{:.1}%", report.documentation_percent),
                );
            }
            if !report.passed {
                anyhow::bail!("One or more required quality gates failed");
            }
            p::success("All required quality gates passed");
        }
    }
    Ok(())
}
