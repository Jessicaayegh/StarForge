use anyhow::Result;
use clap::Args;
use std::env;

#[derive(Args, Debug)]
pub struct BugReportArgs {
    /// Output format: "markdown" (default) or "url" to open a prefilled GitHub issue
    #[arg(short, long, default_value = "markdown")]
    pub output: String,

    /// Skip the preview/confirmation prompt and print directly
    #[arg(long)]
    pub no_confirm: bool,
}

pub fn handle(args: BugReportArgs) -> Result<()> {
    let report = collect_diagnostics();

    if !args.no_confirm {
        println!("Preview of the bug report that will be shared:\n");
        println!("{}", report);
        println!("\nDoes this look correct? No secrets or credentials are included.");
        println!("Press Enter to continue, or Ctrl+C to abort.");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
    }

    match args.output.as_str() {
        "markdown" => println!("{}", report),
        "url" => {
            // GitHub issue URL prefill is a planned addition (see issue #975)
            eprintln!("--output url is not yet implemented; printing markdown instead.");
            println!("{}", report);
        }
        other => anyhow::bail!(
            "Unknown output format {:?}. Use 'markdown' or 'url'.",
            other
        ),
    }

    Ok(())
}

fn collect_diagnostics() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    let shell = env::var("SHELL").unwrap_or_else(|_| "(unknown)".into());
    let term = env::var("TERM").unwrap_or_else(|_| "(unknown)".into());
    let term_program = env::var("TERM_PROGRAM").unwrap_or_else(|_| "(unknown)".into());

    format!(
        "## StarForge Bug Report\n\
         \n\
         ### Environment\n\
         \n\
         | Field          | Value |\n\
         |----------------|-------|\n\
         | starforge      | {version} |\n\
         | OS             | {os} ({arch}) |\n\
         | Shell          | {shell} |\n\
         | Terminal       | {term_program} ({term}) |\n\
         \n\
         ### Steps to Reproduce\n\
         \n\
         <!-- Please describe what you did before encountering the issue -->\n\
         \n\
         1. \n\
         2. \n\
         \n\
         ### Expected Behaviour\n\
         \n\
         <!-- What did you expect to happen? -->\n\
         \n\
         ### Actual Behaviour\n\
         \n\
         <!-- What actually happened? Paste any error output below -->\n\
         \n\
         ```\n\
         \n\
         ```\n"
    )
}
