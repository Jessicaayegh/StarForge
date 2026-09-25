use crate::utils::deploy_orchestrator::{
    build_plan, default_concurrency, execute_plan, execute_plan_parallel, list_states,
    load_manifest, load_state, render_dag, rollback, save_state,
};
use crate::utils::print as p;
use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum OrchestrateCommands {
    /// Validate manifest and build deployment plan
    Plan(PlanArgs),
    /// Execute deployment plan (use --dry-run to simulate)
    Execute(ExecuteArgs),
    /// Roll back a completed deployment
    Rollback(RollbackArgs),
    /// List saved deployment states
    List,
    /// Visualize dependency graph
    Visualize(VisualizeArgs),
    /// Show deployment state by ID
    Status(StatusArgs),
}

#[derive(Args)]
pub struct PlanArgs {
    #[arg(long)]
    pub file: PathBuf,
}

#[derive(Args)]
pub struct ExecuteArgs {
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long, default_value = "true")]
    pub dry_run: bool,
    /// Worker count for parallel deployment: a number >= 1, or "auto" to size
    /// the pool from the host's parallel CPU count (default: 1 = sequential).
    #[arg(long, default_value = "1")]
    pub concurrency: String,
}

#[derive(Args)]
pub struct RollbackArgs {
    #[arg(long)]
    pub id: String,
}

#[derive(Args)]
pub struct VisualizeArgs {
    #[arg(long)]
    pub file: PathBuf,
}

#[derive(Args)]
pub struct StatusArgs {
    #[arg(long)]
    pub id: String,
}

pub async fn handle(cmd: OrchestrateCommands) -> Result<()> {
    match cmd {
        OrchestrateCommands::Plan(args) => handle_plan(args),
        OrchestrateCommands::Execute(args) => handle_execute(args),
        OrchestrateCommands::Rollback(args) => handle_rollback(args),
        OrchestrateCommands::List => handle_list(),
        OrchestrateCommands::Visualize(args) => handle_visualize(args),
        OrchestrateCommands::Status(args) => handle_status(args),
    }
}

fn handle_plan(args: PlanArgs) -> Result<()> {
    p::header("Deployment Orchestration — Plan");
    let manifest = load_manifest(&args.file)?;
    let state = build_plan(&manifest)?;
    let path = save_state(&state)?;

    p::kv("Deployment ID", &state.id);
    p::kv("Manifest", &manifest.name);
    p::kv("Network", &state.network);
    p::kv("Contracts", &state.steps.len().to_string());
    p::kv("State file", &path.display().to_string());

    println!();
    for step in &state.steps {
        p::kv(
            &format!("  {}. {}", step.order, step.contract_id),
            &format!("{} ({})", step.wasm.display(), &step.wasm_hash[..12]),
        );
    }

    p::success("Deployment plan created");
    Ok(())
}

fn handle_execute(args: ExecuteArgs) -> Result<()> {
    p::header("Deployment Orchestration — Execute");

    let mut state = if let Some(id) = args.id {
        load_state(&id)?
    } else if let Some(file) = args.file {
        let manifest = load_manifest(&file)?;
        build_plan(&manifest)?
    } else {
        anyhow::bail!("Specify --file or --id");
    };

    let concurrency = if args.concurrency == "auto" {
        default_concurrency()
    } else {
        args.concurrency.parse::<usize>().map_err(|_| {
            anyhow::anyhow!(
                "--concurrency must be a positive integer or 'auto', got '{}'",
                args.concurrency
            )
        })?
    };

    if concurrency > 1 {
        execute_plan_parallel(&mut state, args.dry_run, concurrency)?;
    } else {
        execute_plan(&mut state, args.dry_run)?;
    }

    p::kv("Deployment ID", &state.id);
    p::kv("Status", &state.status);
    for step in &state.steps {
        println!(
            "  {} — {:?} {}",
            step.contract_id,
            step.status,
            step.deployed_address.as_deref().unwrap_or("-")
        );
    }

    p::success(if args.dry_run {
        "Dry-run execution complete"
    } else {
        "Deployment execution complete"
    });
    Ok(())
}

fn handle_rollback(args: RollbackArgs) -> Result<()> {
    p::header("Deployment Orchestration — Rollback");
    let mut state = load_state(&args.id)?;
    let rolled = rollback(&mut state)?;

    p::kv("Deployment ID", &state.id);
    p::kv("Rolled back", &rolled.join(", "));
    p::success("Rollback complete");
    Ok(())
}

fn handle_list() -> Result<()> {
    p::header("Deployment Orchestration — List");
    let states = list_states()?;
    if states.is_empty() {
        p::info("No deployment states found");
        return Ok(());
    }
    for state in states {
        println!(
            "  {} | {} | {} | {} contracts | {}",
            state.id,
            state.manifest_name,
            state.network,
            state.steps.len(),
            state.status
        );
    }
    Ok(())
}

fn handle_visualize(args: VisualizeArgs) -> Result<()> {
    p::header("Deployment Orchestration — Visualization");
    let manifest = load_manifest(&args.file)?;
    println!("{}", render_dag(&manifest)?);
    Ok(())
}

fn handle_status(args: StatusArgs) -> Result<()> {
    p::header("Deployment Orchestration — Status");
    let state = load_state(&args.id)?;
    p::kv("ID", &state.id);
    p::kv("Manifest", &state.manifest_name);
    p::kv("Network", &state.network);
    p::kv("Status", &state.status);
    p::kv("Created", &state.created_at);
    p::kv("Updated", &state.updated_at);

    for step in &state.steps {
        println!(
            "  {}. {} [{:?}] hash={} addr={}",
            step.order,
            step.contract_id,
            step.status,
            &step.wasm_hash[..12],
            step.deployed_address.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}
