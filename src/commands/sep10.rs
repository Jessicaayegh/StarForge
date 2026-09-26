//! `starforge sep10` — SEP-10 (Stellar Web Authentication) tooling (#953).
//!
//! Anchor and dApp developers need to exercise the SEP-10 challenge/response
//! flow without throwing together an ad-hoc script. This command group wires
//! the protocol implementation in [`crate::utils::sep10`] to the CLI:
//! `sep10 auth` discovers a server from its `stellar.toml`, validates the
//! challenge the server returns, signs it with a saved wallet, and prints the
//! session JWT.

use crate::utils::sep10::{self, Sep10Client};
use crate::utils::{config, output, print as p, wallet_signer};
use anyhow::{Context, Result};
use clap::{Args, Subcommand};

/// `starforge sep10 <command>` — SEP-10 web authentication helpers.
#[derive(Subcommand)]
pub enum Sep10Commands {
    /// Authenticate against a SEP-10 server and print the session JWT
    Auth(Sep10AuthArgs),
}

/// Arguments for `starforge sep10 auth`.
#[derive(Args)]
pub struct Sep10AuthArgs {
    /// Home domain hosting the server's stellar.toml (for example `testanchor.stellar.org`)
    #[arg(long)]
    pub domain: String,

    /// Name of the saved wallet whose account authenticates
    #[arg(long)]
    pub wallet: String,

    /// Memo ID to ask the server to echo in the challenge
    #[arg(long)]
    pub memo: Option<u64>,

    /// `client_domain` to ask the server to pin in the challenge
    #[arg(long)]
    pub client_domain: Option<String>,

    /// Show every step of the challenge/response flow
    #[arg(long)]
    pub verbose: bool,

    /// Emit a machine-readable JSON envelope instead of the bare token
    #[arg(long)]
    pub json: bool,
}

pub async fn handle(cmd: Sep10Commands) -> Result<()> {
    match cmd {
        Sep10Commands::Auth(args) => auth(args).await,
    }
}

async fn auth(args: Sep10AuthArgs) -> Result<()> {
    let emit_json = args.json || output::is_json_mode_enabled();

    let cfg = config::load()?;
    let wallet = cfg
        .wallets
        .iter()
        .find(|w| w.name == args.wallet)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Wallet '{}' not found. Run `starforge wallet list`",
                args.wallet
            )
        })?;
    let secret = wallet_signer::resolve_local_secret(wallet, &args.wallet)?;

    if args.verbose {
        p::header("SEP-10 authentication");
        p::kv("home domain", &args.domain);
        p::kv("wallet", &wallet.name);
        p::kv("account", &wallet.public_key);
        p::step(
            1,
            3,
            &format!("Discovering {} via stellar.toml", args.domain),
        );
    }

    let server = sep10::fetch_stellar_toml(&args.domain)
        .await
        .with_context(|| format!("could not discover a SEP-10 server at {}", args.domain))?;

    if args.verbose {
        p::kv("signing key", &server.signing_key);
        p::kv("web auth endpoint", &server.web_auth_endpoint);
        p::kv("network", &server.network_passphrase);
        p::step(2, 3, "Requesting and validating a challenge");
    }

    let client = Sep10Client::new(server)?;
    let outcome = client
        .authenticate(secret.as_str(), args.memo, args.client_domain.as_deref())
        .await
        .with_context(|| format!("SEP-10 authentication against {} failed", args.domain))?;

    if args.verbose {
        p::step(3, 3, "Exchanging the signed challenge for a session token");
        for (label, value) in &outcome.checks {
            p::kv(label, value);
        }
        for step in &outcome.steps {
            p::info(&format!("{}: {}", step.step, step.detail));
        }
    }

    if emit_json {
        return output::print_json(&outcome);
    }

    // The token is the payload: print it on its own line so the output can be
    // piped straight into a request without post-processing.
    println!("{}", outcome.jwt);
    Ok(())
}
