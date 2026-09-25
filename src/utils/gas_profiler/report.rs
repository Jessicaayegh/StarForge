//! Report rendering: plain-text tables with bar charts, Markdown (for PR
//! comments) and JSON (the saved form, reloadable by `gas report` and
//! `gas compare`).

use super::fees::FeeBreakdown;
use super::suggest::Suggestion;
use super::GasProfileReport;
use anyhow::Result;

/// Output format for gas reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
    Markdown,
}

impl Format {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "text" | "txt" => Ok(Format::Text),
            "json" => Ok(Format::Json),
            "markdown" | "md" => Ok(Format::Markdown),
            other => anyhow::bail!(
                "unknown format '{}': use text, json or markdown",
                other
            ),
        }
    }

    /// Guess a format from an output file extension.
    pub fn from_extension(path: &std::path::Path) -> Option<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Some(Format::Json),
            Some("md") | Some("markdown") => Some(Format::Markdown),
            Some("txt") => Some(Format::Text),
            _ => None,
        }
    }
}

/// A horizontal bar of `width` cells proportional to `value / max`.
pub fn bar(value: u64, max: u64, width: usize) -> String {
    if max == 0 || value == 0 {
        return String::new();
    }
    let cells = ((value as f64 / max as f64) * width as f64).round() as usize;
    "#".repeat(cells.clamp(1, width))
}

fn pad_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, c) in row.iter().enumerate().take(widths.len()) {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let line = |cells: Vec<&str>| -> String {
        let mut s = String::from("  ");
        for (i, c) in cells.iter().enumerate() {
            let pad = widths[i].saturating_sub(c.chars().count());
            s.push_str(c);
            if i + 1 < cells.len() {
                s.push_str(&" ".repeat(pad + 2));
            }
        }
        s.trim_end().to_string() + "\n"
    };
    let mut out = line(headers.to_vec());
    let dash: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&line(dash.iter().map(String::as_str).collect()));
    for row in rows {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

fn md_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = format!("| {} |\n", headers.join(" | "));
    out.push_str(&format!(
        "|{}\n",
        headers.iter().map(|_| "---|").collect::<String>()
    ));
    for r in rows {
        let cells: Vec<String> = r.iter().map(|c| c.replace('|', "\\|")).collect();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out
}

fn fee_rows(fee: &FeeBreakdown, width: usize) -> Vec<Vec<String>> {
    let max = fee.components().iter().map(|(_, v)| *v).max().unwrap_or(0);
    fee.components()
        .into_iter()
        .filter(|(_, v)| *v > 0)
        .map(|(k, v)| {
            let pct = if fee.total == 0 {
                0.0
            } else {
                v as f64 / fee.total as f64 * 100.0
            };
            vec![
                k.to_string(),
                v.to_string(),
                format!("{:.1}%", pct),
                bar(v, max, width),
            ]
        })
        .collect()
}

fn function_rows(r: &GasProfileReport, width: usize) -> Vec<Vec<String>> {
    use super::host_fns::HostCategory as C;
    let max = r.functions.iter().map(|f| f.cost_index).max().unwrap_or(0);
    r.functions
        .iter()
        .map(|f| {
            let loop_expensive: u32 = f
                .host_calls_in_loops
                .iter()
                .filter(|(c, _)| c.is_expensive())
                .map(|(_, n)| *n)
                .sum();
            vec![
                f.name.clone(),
                f.reachable_instructions.to_string(),
                f.sites(C::StorageRead).to_string(),
                f.sites(C::StorageWrite).to_string(),
                f.sites(C::CrossContract).to_string(),
                f.sites(C::Crypto).to_string(),
                f.sites(C::Event).to_string(),
                loop_expensive.to_string(),
                f.cost_index.to_string(),
                bar(f.cost_index, max, width),
            ]
        })
        .collect()
}

const FUNCTION_HEADERS: [&str; 10] = [
    "function",
    "instrs",
    "reads",
    "writes",
    "xcalls",
    "crypto",
    "events",
    "hot-in-loop",
    "cost-index",
    "",
];

fn measurement_rows(r: &GasProfileReport) -> Vec<Vec<String>> {
    r.measurements
        .iter()
        .map(|m| {
            vec![
                m.function.clone(),
                m.usage.instructions.to_string(),
                m.usage
                    .memory_bytes
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "-".into()),
                format!("{}/{}", m.usage.read_entries, m.usage.write_entries),
                format!("{}/{}", m.usage.read_bytes, m.usage.write_bytes),
                m.usage.contract_events_bytes.to_string(),
                m.reported_min_resource_fee
                    .map(|f| f.to_string())
                    .unwrap_or_else(|| "-".into()),
                m.model_fee.total.to_string(),
                m.utilization
                    .ranked()
                    .first()
                    .map(|(k, p)| format!("{} {:.0}%", k, p))
                    .unwrap_or_default(),
            ]
        })
        .collect()
}

const MEASUREMENT_HEADERS: [&str; 9] = [
    "function",
    "cpu insns",
    "mem bytes",
    "entries r/w",
    "bytes r/w",
    "event B",
    "sim fee",
    "model fee",
    "peak limit",
];

fn suggestion_line(i: usize, s: &Suggestion) -> String {
    let mut out = format!(
        "  {}. [{}] {} {} ({})\n",
        i + 1,
        s.severity.to_string().to_uppercase(),
        s.id,
        s.title,
        serde_json::to_value(s.basis)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    );
    out.push_str(&format!("     evidence: {}\n", s.evidence));
    out.push_str(&format!("     fix:      {}\n", s.recommendation));
    if let Some(sav) = s.estimated_savings_stroops {
        out.push_str(&format!("     saves:    ~{} stroops\n", sav));
    }
    out
}

/// Render the suggestions only (used by `gas suggest`).
pub fn render_suggestions_text(suggestions: &[Suggestion]) -> String {
    if suggestions.is_empty() {
        return "  No optimization suggestions.\n".to_string();
    }
    suggestions
        .iter()
        .enumerate()
        .map(|(i, s)| suggestion_line(i, s))
        .collect()
}

/// Plain-text report with tables and bar charts.
pub fn render_text(r: &GasProfileReport) -> String {
    let mut o = String::new();
    o.push_str(&format!("Gas profile: {}\n", r.contract_label));
    o.push_str(&format!("  wasm        {}\n", r.wasm_path));
    o.push_str(&format!("  sha256      {}\n", r.wasm_sha256));
    o.push_str(&format!(
        "  size        {} bytes ({} functions, {} exported, {} host imports)\n",
        r.size_bytes,
        r.module.defined_functions,
        r.module.exported_functions,
        r.module.host_imports
    ));
    o.push_str(&format!("  score       {}/100\n", r.score));
    o.push_str(&format!("  fee rates   {}\n\n", r.fee_config.fees.source));

    o.push_str("Contract functions (static, most expensive first)\n");
    if r.functions.is_empty() {
        o.push_str("  (no exported functions)\n");
    } else {
        o.push_str(&pad_table(&FUNCTION_HEADERS, &function_rows(r, 20)));
        o.push_str(
            "  instrs = static instructions in reachable code; reads/writes/... = host call \
             sites;\n  hot-in-loop = expensive host call sites inside loops; cost-index is \
             relative.\n",
        );
    }
    o.push('\n');

    if !r.measurements.is_empty() {
        o.push_str("Execution profile (measured)\n");
        o.push_str(&pad_table(&MEASUREMENT_HEADERS, &measurement_rows(r)));
        for m in &r.measurements {
            o.push_str(&format!(
                "\n  Resource fee breakdown: {} (model, stroops)\n",
                m.function
            ));
            o.push_str(&pad_table(
                &["resource", "stroops", "share", ""],
                &fee_rows(&m.model_fee, 30),
            ));
        }
        o.push('\n');
    }

    o.push_str("Upload fee (model, lower bound, excludes CPU and rent)\n");
    o.push_str(&pad_table(
        &["resource", "stroops", "share", ""],
        &fee_rows(&r.upload_fee, 30),
    ));
    o.push_str(&format!("  total: {} stroops\n\n", r.upload_fee.total));

    o.push_str(&format!("Suggestions ({})\n", r.suggestions.len()));
    o.push_str(&render_suggestions_text(&r.suggestions));
    o
}

/// Markdown report, suitable for a PR comment or CI summary.
pub fn render_markdown(r: &GasProfileReport) -> String {
    let mut o = format!("## Gas profile: `{}`\n\n", r.contract_label);
    o.push_str(&format!(
        "- **Size:** {} bytes · **Score:** {}/100 · **Upload fee (model):** {} stroops\n",
        r.size_bytes, r.score, r.upload_fee.total
    ));
    o.push_str(&format!("- **SHA-256:** `{}`\n\n", r.wasm_sha256));
    if !r.functions.is_empty() {
        o.push_str("### Contract functions (static)\n\n");
        let rows: Vec<Vec<String>> = function_rows(r, 0)
            .into_iter()
            .map(|mut row| {
                row.pop();
                row
            })
            .collect();
        o.push_str(&md_table(&FUNCTION_HEADERS[..9], &rows));
        o.push('\n');
    }
    if !r.measurements.is_empty() {
        o.push_str("### Execution profile (measured)\n\n");
        o.push_str(&md_table(&MEASUREMENT_HEADERS, &measurement_rows(r)));
        o.push('\n');
    }
    o.push_str(&format!("### Suggestions ({})\n\n", r.suggestions.len()));
    if r.suggestions.is_empty() {
        o.push_str("No optimization suggestions.\n");
    }
    for s in &r.suggestions {
        o.push_str(&format!(
            "- **{} {}** ({}){}: {}  \n  _Evidence:_ {}  \n  _Fix:_ {}\n",
            s.id,
            s.severity,
            s.category,
            s.function
                .as_ref()
                .map(|f| format!(" `{}`", f))
                .unwrap_or_default(),
            s.title,
            s.evidence,
            s.recommendation
        ));
    }
    o
}

/// Render in the requested format.
pub fn render(r: &GasProfileReport, format: Format) -> Result<String> {
    Ok(match format {
        Format::Text => render_text(r),
        Format::Markdown => render_markdown(r),
        Format::Json => serde_json::to_string_pretty(r)?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::fees::NetworkFeeConfig;
    use super::super::profile_bytes;
    use super::super::wasm::builder::{op, ModuleBuilder};
    use super::*;

    fn report() -> GasProfileReport {
        let wasm = ModuleBuilder::new()
            .import("l", "_")
            .func(
                Some("batch"),
                [
                    op::loop_start(),
                    op::call(0),
                    op::drop(),
                    op::i32_const_zero(),
                    op::br_if(0),
                    op::end(),
                ]
                .concat(),
            )
            .build();
        profile_bytes(&wasm, "demo", "demo.wasm", &NetworkFeeConfig::default()).unwrap()
    }

    #[test]
    fn bar_scales() {
        assert_eq!(bar(0, 10, 10), "");
        assert_eq!(bar(10, 10, 10), "##########");
        assert_eq!(bar(5, 10, 10), "#####");
        assert_eq!(bar(1, 1000, 10), "#");
    }

    #[test]
    fn text_contains_sections() {
        let t = render_text(&report());
        assert!(t.contains("Contract functions"));
        assert!(t.contains("batch"));
        assert!(t.contains("GAS-121"));
        assert!(t.contains("Upload fee"));
    }

    #[test]
    fn markdown_and_json_render() {
        let r = report();
        assert!(render(&r, Format::Markdown).unwrap().contains("| batch |"));
        let j = render(&r, Format::Json).unwrap();
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["schema_version"], 1);
    }

    #[test]
    fn format_parsing() {
        assert_eq!(Format::parse("MD").unwrap(), Format::Markdown);
        assert!(Format::parse("html").is_err());
        assert_eq!(
            Format::from_extension(std::path::Path::new("a.json")),
            Some(Format::Json)
        );
    }
}
