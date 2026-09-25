//! # Simulate Command
//!
//! CLI interface for the Network Simulation and Testing Environment.
//! Provides subcommands to create scenarios, run simulations, manage
//! state snapshots, control time, and inject failures.

use crate::utils::network_simulator::{
    failure::{FailureMode, FailureRule},
    scenarios::{BuiltInScenario, ScenarioResult, ScenarioRunner},
    simulator::NetworkSimulator,
};
use crate::utils::print as p;
use crate::utils::simulation_resources;
use anyhow::Result;
use clap::{Args, Subcommand};
use colored::*;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum SimulateCommands {
    /// List available built-in simulation scenarios
    ListScenarios,
    /// Run a built-in or custom simulation scenario
    Run(RunArgs),
    /// Create and save a custom scenario to a JSON file
    Create(CreateArgs),
    /// Import a scenario from a JSON file
    Import(ImportArgs),

    /// Take a state snapshot
    Snapshot(SnapshotArgs),
    /// List available snapshots
    Snapshots,
    /// Restore a snapshot by ID
    Restore(RestoreArgs),
    /// Export a snapshot to a JSON file
    ExportSnapshot(ExportSnapshotArgs),
    /// Import a snapshot from a JSON file
    ImportSnapshot(ImportSnapshotArgs),

    /// Show current time and ledger state
    Time,
    /// Advance the ledger by a number of sequences
    Advance(AdvanceArgs),
    /// Jump to a specific ledger sequence
    Jump(JumpArgs),
    /// Freeze or unfreeze time
    Freeze(FreezeArgs),
    /// Save a time point for later restoration
    SaveTime(SaveTimeArgs),
    /// Restore a saved time point
    RestoreTime(RestoreTimeArgs),

    /// List active failure injection rules
    Failures,
    /// Add a failure injection rule
    AddFailure(AddFailureArgs),
    /// Remove a failure injection rule
    RemoveFailure(RemoveFailureArgs),
    /// Enable or disable failure injection
    ToggleFailure(ToggleFailureArgs),

    /// Show simulator status (ledger, accounts, contracts)
    Status,

    /// Report CPU, memory, footprint, and minimum resource fee from a
    /// `simulateTransaction` response, and derive a submittable fee
    Resources(ResourcesArgs),

    /// Deploy a simulated contract
    Deploy(DeploySimArgs),
    /// Invoke a simulated contract function
    Invoke(InvokeSimArgs),
    /// List accounts in the simulator
    Accounts,
    /// List contracts in the simulator
    Contracts,

    /// Reset the simulator to its initial state
    Reset,
}

#[derive(Args)]
pub struct RunArgs {
    /// Name of the built-in scenario to run
    #[arg(long, default_value = "simple-counter")]
    pub scenario: String,

    /// Seed for deterministic execution
    #[arg(long, default_value = "42")]
    pub seed: u64,

    /// Show detailed output
    #[arg(long, default_value = "false")]
    pub verbose: bool,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Output file path for the scenario JSON
    #[arg(long)]
    pub output: PathBuf,

    /// Scenario name (simple-counter, token-transfer, escrow, multisig-vault, empty, load-test)
    #[arg(long, default_value = "simple-counter")]
    pub scenario: String,

    /// Seed for deterministic execution
    #[arg(long, default_value = "42")]
    pub seed: u64,
}

#[derive(Args)]
pub struct ImportArgs {
    /// Path to scenario JSON file
    #[arg(long)]
    pub path: PathBuf,

    /// Show detailed output
    #[arg(long, default_value = "false")]
    pub verbose: bool,
}

#[derive(Args)]
pub struct SnapshotArgs {
    /// Label for the snapshot
    pub label: String,
}

#[derive(Args)]
pub struct RestoreArgs {
    /// Snapshot ID to restore
    pub id: String,
}

#[derive(Args)]
pub struct ExportSnapshotArgs {
    /// Snapshot ID
    pub id: String,
    /// Output file path
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Args)]
pub struct ImportSnapshotArgs {
    /// Path to snapshot JSON file
    #[arg(long)]
    pub path: PathBuf,
}

#[derive(Args)]
pub struct AdvanceArgs {
    /// Number of ledger closes to advance
    #[arg(default_value = "1")]
    pub count: u32,
}

#[derive(Args)]
pub struct JumpArgs {
    /// Target ledger sequence number
    pub sequence: u32,
}

#[derive(Args)]
pub struct FreezeArgs {
    /// Action: freeze or unfreeze
    pub action: String,
}

#[derive(Args)]
pub struct SaveTimeArgs {
    /// Label for the time point
    pub label: String,
}

#[derive(Args)]
pub struct RestoreTimeArgs {
    /// Label of the time point to restore
    pub label: String,
}

#[derive(Args)]
pub struct AddFailureArgs {
    /// Rule name
    #[arg(long)]
    pub name: String,

    /// Failure mode (rpc-timeout, connection-refused, insufficient-fee, bad-auth,
    /// contract-panic, account-not-found, contract-not-found, insufficient-balance,
    /// budget-exceeded)
    #[arg(long)]
    pub mode: String,

    /// Optional RPC method filter
    #[arg(long)]
    pub rpc_method: Option<String>,

    /// Optional contract ID filter
    #[arg(long)]
    pub contract: Option<String>,

    /// Optional account filter
    #[arg(long)]
    pub account: Option<String>,

    /// Probability of firing (0.0 to 1.0, default 1.0)
    #[arg(long, default_value = "1.0")]
    pub probability: f64,

    /// Maximum activations (0 = unlimited)
    #[arg(long, default_value = "0")]
    pub max_activations: u64,
}

#[derive(Args)]
pub struct RemoveFailureArgs {
    /// Rule name to remove
    pub name: String,
}

#[derive(Args)]
pub struct ToggleFailureArgs {
    /// Enable or disable
    pub action: String,
}

#[derive(Args)]
pub struct DeploySimArgs {
    /// WASM hash to deploy
    #[arg(long)]
    pub wasm_hash: String,

    /// Deployer public key
    #[arg(long)]
    pub deployer: String,

    /// Contract name (optional)
    #[arg(long)]
    pub name: Option<String>,
}

/// Arguments for `starforge simulate resources`.
///
/// Either `--file` (offline: a saved `simulateTransaction` response) or
/// `--contract` + `--function` (live: simulate against a Soroban RPC endpoint)
/// must be supplied.
#[derive(Args)]
pub struct ResourcesArgs {
    /// Path to a saved `simulateTransaction` JSON response (offline planning,
    /// e.g. a response captured in CI)
    #[arg(long, conflicts_with = "contract")]
    pub file: Option<PathBuf>,

    /// Contract ID to simulate against a live Soroban RPC endpoint
    #[arg(long)]
    pub contract: Option<String>,

    /// Contract function to simulate (required with --contract)
    #[arg(long)]
    pub function: Option<String>,

    /// Function argument (repeat for multiple arguments)
    #[arg(long = "arg", value_name = "VALUE")]
    pub args: Vec<String>,

    /// Type for the corresponding --arg (repeat; defaults to inferred types)
    #[arg(long = "arg-type", value_name = "TYPE")]
    pub arg_types: Vec<String>,

    /// Network to simulate against when using --contract
    #[arg(long, default_value = "testnet")]
    pub network: String,

    /// Safety margin, in percent, applied to the minimum resource fee
    #[arg(long, default_value_t = simulation_resources::DEFAULT_FEE_MARGIN_PERCENT)]
    pub margin: u32,

    /// Per-operation inclusion (base) fee in stroops
    #[arg(long, default_value_t = simulation_resources::DEFAULT_INCLUSION_FEE_STROOPS)]
    pub inclusion_fee: u64,

    /// Deterministic simulation profile (ci-smoke, ci-full, dev-fast) for resource ceiling assertions
    #[arg(long)]
    pub profile: Option<String>,

    /// Emit the report as machine-readable JSON
    #[arg(long, default_value = "false")]
    pub json: bool,
}

#[derive(Args)]
pub struct InvokeSimArgs {
    /// Contract ID to invoke
    pub contract_id: String,

    /// Function to invoke
    pub function: String,

    /// Arguments to the function
    #[arg(last = true)]
    pub args: Vec<String>,
}

// ── Global simulator state (lazy) ─────────────────────────────────────────────

use once_cell::sync::Lazy;
use std::sync::Mutex;

static CURRENT_SIM: Lazy<Mutex<Option<NetworkSimulator>>> = Lazy::new(|| Mutex::new(None));
static LAST_RESULT: Lazy<Mutex<Option<ScenarioResult>>> = Lazy::new(|| Mutex::new(None));

fn get_or_create_sim() -> std::sync::MutexGuard<'static, Option<NetworkSimulator>> {
    let mut guard = CURRENT_SIM.lock().unwrap();
    if guard.is_none() {
        *guard = Some(NetworkSimulator::new());
    }
    guard
}

// ── Handler ───────────────────────────────────────────────────────────────────

pub async fn handle(cmd: SimulateCommands) -> Result<()> {
    match cmd {
        SimulateCommands::ListScenarios => list_scenarios(),
        SimulateCommands::Run(args) => run_scenario(args),
        SimulateCommands::Create(args) => create_scenario(args),
        SimulateCommands::Import(args) => import_scenario(args),
        SimulateCommands::Snapshot(args) => take_snapshot(args),
        SimulateCommands::Snapshots => list_snapshots(),
        SimulateCommands::Restore(args) => restore_snapshot(args),
        SimulateCommands::ExportSnapshot(args) => export_snapshot(args),
        SimulateCommands::ImportSnapshot(args) => import_snapshot(args),
        SimulateCommands::Time => show_time(),
        SimulateCommands::Advance(args) => advance(args),
        SimulateCommands::Jump(args) => jump(args),
        SimulateCommands::Freeze(args) => toggle_freeze(args),
        SimulateCommands::SaveTime(args) => save_time(args),
        SimulateCommands::RestoreTime(args) => restore_time(args),
        SimulateCommands::Failures => list_failures(),
        SimulateCommands::AddFailure(args) => add_failure(args),
        SimulateCommands::RemoveFailure(args) => remove_failure(args),
        SimulateCommands::ToggleFailure(args) => toggle_failure(args),
        SimulateCommands::Status => show_status(),
        SimulateCommands::Resources(args) => resource_report(args).await,
        SimulateCommands::Deploy(args) => deploy_contract(args),
        SimulateCommands::Invoke(args) => invoke_contract(args),
        SimulateCommands::Accounts => list_accounts(),
        SimulateCommands::Contracts => list_contracts(),
        SimulateCommands::Reset => reset_sim(),
    }
}

// ── Scenario commands ─────────────────────────────────────────────────────────

fn list_scenarios() -> Result<()> {
    p::header("Available Built-in Scenarios");
    p::separator();

    for scenario in &[
        BuiltInScenario::SimpleCounter,
        BuiltInScenario::TokenTransfer,
        BuiltInScenario::Escrow,
        BuiltInScenario::MultisigVault,
        BuiltInScenario::Empty,
        BuiltInScenario::LoadTest,
    ] {
        println!(
            "  {} {}",
            scenario.name().cyan().bold(),
            format!("— {}", scenario.description()).dimmed()
        );
    }

    p::separator();
    p::info("Run a scenario with: starforge simulate run --scenario <name>");
    Ok(())
}

fn run_scenario(args: RunArgs) -> Result<()> {
    let scenario_name = args.scenario.to_lowercase().replace('-', "_");

    let built_in = match scenario_name.as_str() {
        "simple_counter" => BuiltInScenario::SimpleCounter,
        "token_transfer" => BuiltInScenario::TokenTransfer,
        "escrow" => BuiltInScenario::Escrow,
        "multisig_vault" => BuiltInScenario::MultisigVault,
        "empty" => BuiltInScenario::Empty,
        "load_test" => BuiltInScenario::LoadTest,
        _ => anyhow::bail!(
            "Unknown scenario '{}'. Use 'starforge simulate list-scenarios' to see available ones.",
            args.scenario
        ),
    };

    p::header(&format!("Running Scenario: {}", built_in.name()));

    let scenario = ScenarioRunner::load_built_in(built_in, args.seed);
    let (sim, result) = ScenarioRunner::run(scenario);

    // Store in global state.
    {
        let mut guard = CURRENT_SIM.lock().unwrap();
        *guard = Some(sim);
    }
    {
        let mut guard = LAST_RESULT.lock().unwrap();
        *guard = Some(result.clone());
    }

    p::kv("Accounts", &result.accounts.len().to_string());
    p::kv("Contracts", &result.contracts.len().to_string());

    if args.verbose {
        println!();
        p::info("Account mappings:");
        for (name, pk) in &result.accounts {
            p::kv(&format!("  {}", name), pk);
        }

        println!();
        p::info("Contract mappings:");
        for (name, cid) in &result.contracts {
            p::kv(&format!("  {}", name), cid);
        }
    }

    p::success(&format!("Scenario '{}' loaded and ready", built_in.name()));
    Ok(())
}

fn create_scenario(args: CreateArgs) -> Result<()> {
    let scenario_name = args.scenario.to_lowercase().replace('-', "_");

    let built_in = match scenario_name.as_str() {
        "simple_counter" => BuiltInScenario::SimpleCounter,
        "token_transfer" => BuiltInScenario::TokenTransfer,
        "escrow" => BuiltInScenario::Escrow,
        "multisig_vault" => BuiltInScenario::MultisigVault,
        "empty" => BuiltInScenario::Empty,
        "load_test" => BuiltInScenario::LoadTest,
        _ => anyhow::bail!("Unknown scenario '{}'", args.scenario),
    };

    let scenario = ScenarioRunner::load_built_in(built_in, args.seed);
    let json = serde_json::to_string_pretty(&scenario)?;

    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.output, json)?;

    p::success(&format!(
        "Scenario '{}' saved to {}",
        built_in.name(),
        args.output.display()
    ));
    Ok(())
}

fn import_scenario(args: ImportArgs) -> Result<()> {
    crate::utils::config::validate_file_path(&args.path, Some("json"))?;
    let json = std::fs::read_to_string(&args.path)?;
    let scenario: crate::utils::network_simulator::scenarios::Scenario =
        serde_json::from_str(&json)?;

    let (sim, result) = ScenarioRunner::run(scenario);

    {
        let mut guard = CURRENT_SIM.lock().unwrap();
        *guard = Some(sim);
    }
    {
        let mut guard = LAST_RESULT.lock().unwrap();
        *guard = Some(result.clone());
    }

    p::success(&format!(
        "Imported scenario '{}' with {} accounts and {} contracts",
        result.scenario_name,
        result.accounts.len(),
        result.contracts.len()
    ));

    if args.verbose {
        for (name, pk) in &result.accounts {
            p::kv(&format!("  Account '{}'", name), pk);
        }
        for (name, cid) in &result.contracts {
            p::kv(&format!("  Contract '{}'", name), cid);
        }
    }

    Ok(())
}

// ── Snapshot commands ─────────────────────────────────────────────────────────

fn take_snapshot(args: SnapshotArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    let snap_id = sim.take_snapshot(&args.label);
    p::success(&format!(
        "Snapshot '{}' taken (ID: {})",
        args.label, snap_id
    ));
    Ok(())
}

fn list_snapshots() -> Result<()> {
    let guard = get_or_create_sim();
    let sim = guard.as_ref().unwrap();
    let snapshots = sim.snapshot_manager.list();

    if snapshots.is_empty() {
        p::info("No snapshots available. Take one with: starforge simulate snapshot <label>");
        return Ok(());
    }

    p::header("State Snapshots");
    p::separator();

    for (id, label, created_at) in &snapshots {
        println!(
            "  {} {} (created: {})",
            id.cyan().bold(),
            label,
            created_at.dimmed()
        );
    }

    p::separator();
    p::kv("Total", &snapshots.len().to_string());
    Ok(())
}

fn restore_snapshot(args: RestoreArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    sim.restore_snapshot(&args.id)
        .map_err(|e| anyhow::anyhow!("Restore failed: {}", e))?;
    p::success(&format!("State restored from snapshot '{}'", args.id));
    Ok(())
}

fn export_snapshot(args: ExportSnapshotArgs) -> Result<()> {
    let guard = get_or_create_sim();
    let sim = guard.as_ref().unwrap();
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    sim.snapshot_manager
        .export_to_file(&args.id, &args.output)
        .map_err(|e| anyhow::anyhow!("Export failed: {}", e))?;
    p::success(&format!(
        "Snapshot '{}' exported to {}",
        args.id,
        args.output.display()
    ));
    Ok(())
}

fn import_snapshot(args: ImportSnapshotArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    let id = sim
        .snapshot_manager
        .import_from_file(&args.path)
        .map_err(|e| anyhow::anyhow!("Import failed: {}", e))?;
    p::success(&format!(
        "Snapshot imported from {} with ID '{}'",
        args.path.display(),
        id
    ));
    Ok(())
}

// ── Time commands ─────────────────────────────────────────────────────────────

fn show_time() -> Result<()> {
    let guard = get_or_create_sim();
    let sim = guard.as_ref().unwrap();
    let tc = &sim.time_controller;

    p::header("Simulation Time");
    p::separator();
    p::kv("Ledger Sequence", &tc.ledger_time.sequence.to_string());
    p::kv("Timestamp", &tc.current_time_string());
    p::kv("Unix Timestamp", &tc.ledger_time.timestamp.to_string());
    p::kv(
        "Close Interval",
        &format!("{}s", tc.ledger_time.close_seconds),
    );
    p::kv("Frozen", if tc.ledger_time.frozen { "yes" } else { "no" });

    let save_points = tc.list_save_points();
    if !save_points.is_empty() {
        println!();
        p::info("Saved time points:");
        for (label, lt) in save_points {
            p::kv(&format!("  {}", label), &format!("seq {}", lt.sequence));
        }
    }

    p::separator();
    Ok(())
}

fn advance(args: AdvanceArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    sim.advance_ledgers(args.count);
    p::success(&format!(
        "Advanced to ledger sequence {}",
        sim.current_ledger()
    ));
    Ok(())
}

fn jump(args: JumpArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    sim.time_controller.jump_to_sequence(args.sequence);
    p::success(&format!(
        "Jumped to ledger sequence {}",
        sim.time_controller.ledger_time.sequence
    ));
    Ok(())
}

fn toggle_freeze(args: FreezeArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();

    match args.action.as_str() {
        "freeze" => {
            sim.time_controller.freeze();
            p::success("Time frozen");
        }
        "unfreeze" => {
            sim.time_controller.unfreeze();
            p::success("Time unfrozen");
        }
        _ => anyhow::bail!("Action must be 'freeze' or 'unfreeze'"),
    }
    Ok(())
}

fn save_time(args: SaveTimeArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    sim.time_controller.save_point(&args.label);
    p::success(&format!("Time point '{}' saved", args.label));
    Ok(())
}

fn restore_time(args: RestoreTimeArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    if sim.time_controller.restore_point(&args.label).is_some() {
        p::success(&format!(
            "Restored time point '{}' (seq {})",
            args.label, sim.time_controller.ledger_time.sequence
        ));
    } else {
        anyhow::bail!("Time point '{}' not found", args.label);
    }
    Ok(())
}

// ── Failure commands ──────────────────────────────────────────────────────────

fn list_failures() -> Result<()> {
    let guard = get_or_create_sim();
    let sim = guard.as_ref().unwrap();
    let injector = &sim.failure_injector;

    p::header("Failure Injection Rules");
    p::kv("Enabled", if injector.enabled { "yes" } else { "no" });
    p::separator();

    if injector.rule_count() == 0 {
        p::info("No failure rules configured.");
        return Ok(());
    }

    for rule in injector.rules() {
        let mode_str = format!("{:?}", rule.mode);
        println!(
            "  {} ({}){}",
            rule.name.cyan().bold(),
            mode_str,
            if rule.max_activations > 0 {
                format!(" [max: {}]", rule.max_activations)
            } else {
                String::new()
            }
        );
        if let Some(ref method) = rule.rpc_method_filter {
            p::kv("    RPC method", method);
        }
        if let Some(ref cid) = rule.contract_id_filter {
            p::kv("    Contract", cid);
        }
        if let Some(ref acct) = rule.account_filter {
            p::kv("    Account", acct);
        }
        p::kv("    Probability", &rule.probability.to_string());
        p::kv("    Fired", &rule.times_fired.to_string());
    }

    p::separator();
    Ok(())
}

fn add_failure(args: AddFailureArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();

    let mode = match args.mode.as_str() {
        "rpc-timeout" => FailureMode::RpcTimeout,
        "connection-refused" => FailureMode::RpcConnectionRefused,
        "insufficient-fee" => FailureMode::InsufficientFee,
        "bad-auth" => FailureMode::BadAuth,
        "contract-panic" => FailureMode::ContractPanic,
        "account-not-found" => FailureMode::AccountNotFound,
        "contract-not-found" => FailureMode::ContractNotFound,
        "insufficient-balance" => FailureMode::InsufficientBalance,
        "budget-exceeded" => FailureMode::BudgetExceeded,
        _ => anyhow::bail!(
            "Unknown failure mode '{}'. Available: rpc-timeout, connection-refused, \
             insufficient-fee, bad-auth, contract-panic, account-not-found, \
             contract-not-found, insufficient-balance, budget-exceeded",
            args.mode
        ),
    };

    let mut rule = FailureRule::new(&args.name, mode)
        .with_probability(args.probability.clamp(0.0, 1.0))
        .with_max_activations(args.max_activations);

    if let Some(ref method) = args.rpc_method {
        rule = rule.with_rpc_method(method);
    }
    if let Some(ref cid) = args.contract {
        rule = rule.with_contract(cid);
    }
    if let Some(ref acct) = args.account {
        rule = rule.with_account(acct);
    }

    sim.failure_injector.add_rule(rule);
    sim.failure_injector.enable();

    p::success(&format!("Failure rule '{}' added", args.name));
    Ok(())
}

fn remove_failure(args: RemoveFailureArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();

    if sim.failure_injector.remove_rule(&args.name) {
        p::success(&format!("Failure rule '{}' removed", args.name));
    } else {
        p::warn(&format!("Failure rule '{}' not found", args.name));
    }
    Ok(())
}

fn toggle_failure(args: ToggleFailureArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();

    match args.action.as_str() {
        "enable" => {
            sim.failure_injector.enable();
            p::success("Failure injection enabled");
        }
        "disable" => {
            sim.failure_injector.disable();
            p::success("Failure injection disabled");
        }
        _ => anyhow::bail!("Action must be 'enable' or 'disable'"),
    }
    Ok(())
}

// ── Status command ────────────────────────────────────────────────────────────

fn show_status() -> Result<()> {
    let guard = get_or_create_sim();
    let sim = guard.as_ref().unwrap();
    let status = sim.get_status();

    p::header("Network Simulator Status");
    p::separator();
    p::kv("Mode", status["mode"].as_str().unwrap_or("unknown"));
    p::kv(
        "Ledger",
        &format!(
            "seq {} | protocol v{}",
            status["ledger"]["sequence"].as_u64().unwrap_or(0),
            status["ledger"]["protocol_version"].as_u64().unwrap_or(0)
        ),
    );
    p::kv(
        "Accounts",
        &status["accounts"].as_u64().unwrap_or(0).to_string(),
    );
    p::kv(
        "Contracts",
        &status["contracts"].as_u64().unwrap_or(0).to_string(),
    );
    p::kv(
        "Transactions",
        &status["transactions"].as_u64().unwrap_or(0).to_string(),
    );
    p::kv("Seed", &status["seed"].as_u64().unwrap_or(0).to_string());

    println!();
    p::info("Time:");
    p::kv(
        "  Sequence",
        &status["time"]["sequence"].as_u64().unwrap_or(0).to_string(),
    );
    p::kv(
        "  Frozen",
        status["time"]["frozen"]
            .as_bool()
            .unwrap_or(false)
            .to_string()
            .as_str(),
    );
    p::kv(
        "  Failure Injection",
        status["failure_injection"]
            .as_bool()
            .unwrap_or(false)
            .to_string()
            .as_str(),
    );

    if let Some(ref result) = *LAST_RESULT.lock().unwrap() {
        println!();
        p::info("Last scenario result:");
        p::kv("  Name", &result.scenario_name);
        p::kv("  Accounts", &result.accounts.len().to_string());
        p::kv("  Contracts", &result.contracts.len().to_string());
    }

    p::separator();
    Ok(())
}

// ── Resource reporting ────────────────────────────────────────────────────────

/// Validate the argument combination for `simulate resources`.
///
/// Split out from the async handler so the input rules are unit-testable
/// without a network or a filesystem.
pub(crate) fn validate_resources_args(args: &ResourcesArgs) -> Result<()> {
    if args.file.is_none() && args.contract.is_none() {
        anyhow::bail!(
            "Provide either --file <simulation.json> to read a saved simulateTransaction \
             response, or --contract <id> --function <name> to simulate against a live \
             Soroban RPC endpoint."
        );
    }
    if args.contract.is_some() && args.function.is_none() {
        anyhow::bail!("--function is required when simulating a live contract with --contract");
    }
    if args.margin > simulation_resources::MAX_FEE_MARGIN_PERCENT {
        anyhow::bail!(
            "--margin {} is out of range (expected 0..={})",
            args.margin,
            simulation_resources::MAX_FEE_MARGIN_PERCENT
        );
    }
    if !args.arg_types.is_empty() && args.arg_types.len() != args.args.len() {
        anyhow::bail!(
            "--arg-type was supplied {} time(s) but --arg {} time(s); they must match one-to-one",
            args.arg_types.len(),
            args.args.len()
        );
    }
    if let Some(profile_name) = &args.profile {
        simulation_resources::get_profile(profile_name).map_err(|e| anyhow::anyhow!("{}", e))?;
    }
    Ok(())
}

async fn resource_report(args: ResourcesArgs) -> Result<()> {
    validate_resources_args(&args)?;

    let resources = match (&args.file, &args.contract) {
        (Some(path), _) => {
            crate::utils::config::validate_file_path(path, Some("json"))?;
            let raw = std::fs::read_to_string(path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read simulation response {}: {}",
                    path.display(),
                    e
                )
            })?;
            simulation_resources::parse_simulation_response_str(&raw)
                .map_err(|e| anyhow::anyhow!("{}", e))?
        }
        (None, Some(contract_id)) => {
            crate::utils::config::validate_contract_id(contract_id)?;
            crate::utils::config::validate_network(&args.network)?;

            let function = args.function.as_deref().unwrap_or_default();
            let arg_types = if args.arg_types.is_empty() {
                vec!["auto".to_string(); args.args.len()]
            } else {
                args.arg_types.clone()
            };

            let simulation = crate::utils::soroban::simulate_transaction(
                contract_id,
                function,
                &args.args,
                &arg_types,
                &args.network,
            )
            .await?;

            simulation.resources.ok_or_else(|| {
                anyhow::anyhow!(
                    "Soroban RPC did not return resource accounting for this call{}",
                    if simulation.errors.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", simulation.errors.join("; "))
                    }
                )
            })?
        }
        (None, None) => unreachable!("validated above"),
    };

    let profile = match &args.profile {
        Some(name) => {
            Some(simulation_resources::get_profile(name).map_err(|e| anyhow::anyhow!("{}", e))?)
        }
        None => None,
    };

    let margin = match &profile {
        Some(p) if args.margin == simulation_resources::DEFAULT_FEE_MARGIN_PERCENT => {
            p.default_margin_percent
        }
        _ => args.margin,
    };

    let plan = simulation_resources::plan_fee(&resources, margin, args.inclusion_fee)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let profile_eval = profile.as_ref().map(|p| {
        let violations = simulation_resources::validate_against_profile(&resources, &plan, p);
        (p, violations)
    });

    if args.json {
        let json_eval = profile_eval.as_ref().map(|(p, v)| (*p, v.as_slice()));
        println!(
            "{}",
            serde_json::to_string_pretty(&simulation_resources::report_json_with_profile(
                &resources, &plan, json_eval
            ))?
        );
    } else {
        simulation_resources::render_report(&resources, &plan);
        if let Some((p, violations)) = &profile_eval {
            simulation_resources::render_profile_report(p, violations);
        }
        p::info(
            "Submit with at least the recommended fee; ledger state can move between \
             simulation and submission.",
        );
    }

    if let Some((p, violations)) = &profile_eval {
        if !violations.is_empty() {
            anyhow::bail!(
                "Simulation failed profile '{}' assertion with {} violation(s)",
                p.name,
                violations.len()
            );
        }
    }

    Ok(())
}

// ── Contract commands ─────────────────────────────────────────────────────────

fn deploy_contract(args: DeploySimArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();

    let contract = sim
        .deploy_contract(&args.wasm_hash, &args.deployer)
        .map_err(|e| anyhow::anyhow!("Deployment failed: {}", e))?;

    p::success("Contract deployed");
    p::kv_accent("Contract ID", &contract.contract_id);
    p::kv("WASM Hash", &contract.wasm_hash);
    p::kv("Deployer", &contract.deployer);

    if let Some(name) = &args.name {
        p::kv("Name", name);
    }

    Ok(())
}

fn invoke_contract(args: InvokeSimArgs) -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();

    // Try to find a source account – use the first account in the simulator.
    let source = match sim.list_accounts().first() {
        Some(acct) => acct.public_key.clone(),
        None => anyhow::bail!("No accounts in simulator. Create one or run a scenario first."),
    };

    // Simulate first.
    p::info(&format!(
        "Simulating {}::{}({})...",
        &args.contract_id[..8],
        args.function,
        args.args.join(", ")
    ));

    match sim.simulate_invoke(&args.contract_id, &args.function, &args.args, &source) {
        Ok(outcome) => {
            p::kv_accent("Return Value", &outcome.return_value);
            p::kv("Fee (stroops)", &outcome.fee_stroops.to_string());
            p::kv("Events", &outcome.events.len().to_string());
            if !outcome.events.is_empty() {
                for event in &outcome.events {
                    p::kv("  •", event);
                }
            }

            // Submit.
            p::info("Submitting transaction...");
            match sim.submit_invoke(
                &args.contract_id,
                &args.function,
                &args.args,
                &source,
                outcome.fee_stroops,
            ) {
                Ok(receipt) => {
                    p::success("Transaction submitted");
                    p::kv_accent("TX Hash", &receipt.hash);
                    p::kv("Ledger", &receipt.ledger.to_string());
                    p::kv("Return Value", &receipt.return_value);
                }
                Err(e) => {
                    p::warn(&format!("Submission failed (simulated): {}", e));
                }
            }
        }
        Err(e) => {
            p::warn(&format!("Simulation failed (simulated): {}", e));
        }
    }

    Ok(())
}

fn list_accounts() -> Result<()> {
    let guard = get_or_create_sim();
    let sim = guard.as_ref().unwrap();
    let accounts = sim.list_accounts();

    if accounts.is_empty() {
        p::info("No accounts in simulator.");
        return Ok(());
    }

    p::header("Simulator Accounts");
    p::separator();

    for acct in &accounts {
        println!(
            "  {}  {:>10.2} XLM  seq:{}",
            acct.public_key.cyan().bold(),
            acct.balance,
            acct.sequence
        );
    }

    p::separator();
    p::kv("Total", &accounts.len().to_string());
    Ok(())
}

fn list_contracts() -> Result<()> {
    let guard = get_or_create_sim();
    let sim = guard.as_ref().unwrap();
    let contracts = sim.list_contracts();

    if contracts.is_empty() {
        p::info("No contracts deployed in simulator.");
        return Ok(());
    }

    p::header("Simulator Contracts");
    p::separator();

    for ctr in &contracts {
        println!(
            "  {}  wasm:{}  deployer:{}  storage:{} entries",
            ctr.contract_id.cyan().bold(),
            &ctr.wasm_hash[..12],
            &ctr.deployer[..12],
            ctr.storage.len()
        );
    }

    p::separator();
    p::kv("Total", &contracts.len().to_string());
    Ok(())
}

fn reset_sim() -> Result<()> {
    let mut guard = get_or_create_sim();
    let sim = guard.as_mut().unwrap();
    sim.reset();
    p::success("Simulator reset to initial state");
    Ok(())
}

#[cfg(test)]
mod resource_arg_tests {
    use super::*;

    fn args() -> ResourcesArgs {
        ResourcesArgs {
            file: None,
            contract: None,
            function: None,
            args: Vec::new(),
            arg_types: Vec::new(),
            network: "testnet".to_string(),
            margin: simulation_resources::DEFAULT_FEE_MARGIN_PERCENT,
            inclusion_fee: simulation_resources::DEFAULT_INCLUSION_FEE_STROOPS,
            profile: None,
            json: false,
        }
    }

    #[test]
    fn accepts_offline_file_source() {
        let mut a = args();
        a.file = Some(PathBuf::from("sim.json"));
        assert!(validate_resources_args(&a).is_ok());
    }

    #[test]
    fn accepts_valid_simulation_profile() {
        let mut a = args();
        a.file = Some(PathBuf::from("sim.json"));
        a.profile = Some("ci-smoke".to_string());
        assert!(validate_resources_args(&a).is_ok());

        a.profile = Some("dev-fast".to_string());
        assert!(validate_resources_args(&a).is_ok());
    }

    #[test]
    fn rejects_unknown_simulation_profile_loudly() {
        let mut a = args();
        a.file = Some(PathBuf::from("sim.json"));
        a.profile = Some("invalid-profile".to_string());
        let err = validate_resources_args(&a).unwrap_err();
        assert!(err.to_string().contains("unknown simulation profile"));
        assert!(err.to_string().contains("invalid-profile"));
    }

    #[test]
    fn accepts_live_contract_source() {
        let mut a = args();
        a.contract = Some("C".repeat(56));
        a.function = Some("balance".to_string());
        assert!(validate_resources_args(&a).is_ok());
    }

    #[test]
    fn rejects_missing_source() {
        let err = validate_resources_args(&args()).unwrap_err();
        assert!(err.to_string().contains("--file"));
        assert!(err.to_string().contains("--contract"));
    }

    #[test]
    fn rejects_contract_without_function() {
        let mut a = args();
        a.contract = Some("C".repeat(56));
        let err = validate_resources_args(&a).unwrap_err();
        assert!(err.to_string().contains("--function"));
    }

    #[test]
    fn accepts_boundary_margin_and_rejects_one_over() {
        let mut a = args();
        a.file = Some(PathBuf::from("sim.json"));
        a.margin = simulation_resources::MAX_FEE_MARGIN_PERCENT;
        assert!(validate_resources_args(&a).is_ok());

        a.margin = simulation_resources::MAX_FEE_MARGIN_PERCENT + 1;
        assert!(validate_resources_args(&a)
            .unwrap_err()
            .to_string()
            .contains("--margin"));
    }

    #[test]
    fn rejects_mismatched_arg_types() {
        let mut a = args();
        a.contract = Some("C".repeat(56));
        a.function = Some("transfer".to_string());
        a.args = vec!["1".to_string(), "2".to_string()];
        a.arg_types = vec!["u32".to_string()];
        assert!(validate_resources_args(&a)
            .unwrap_err()
            .to_string()
            .contains("one-to-one"));
    }
}
