# Security Logging Best Practices

This document provides practical implementation patterns for adding security logging to StarForge operations.

## Table of Contents

1. [Core Principles](#core-principles)
2. [Wallet Logging Patterns](#wallet-logging-patterns)
3. [Plugin Logging Patterns](#plugin-logging-patterns)
4. [Deployment Logging Patterns](#deployment-logging-patterns)
5. [Error and Exception Logging](#error-and-exception-logging)
6. [Testing Security Logging](#testing-security-logging)
7. [Code Review Checklist](#code-review-checklist)
8. [Common Mistakes](#common-mistakes)

---

## Core Principles

### 1. Never Log Secrets

```rust
// ❌ WRONG
error!("Failed to decrypt: passphrase={}", passphrase);

// ✅ CORRECT
error!("Failed to decrypt wallet - wrong passphrase provided");
```

### 2. Always Log Operation Outcomes

```rust
// ❌ WRONG - no indication of success or failure
fn create_wallet(name: &str) -> Result<Wallet> {
    Wallet::new(name)
}

// ✅ CORRECT - logs the outcome
fn create_wallet(name: &str) -> Result<Wallet> {
    match Wallet::new(name) {
        Ok(wallet) => {
            info!(wallet = name, status = "success", "Wallet created");
            Ok(wallet)
        }
        Err(e) => {
            error!(wallet = name, error = %e, status = "failed", "Wallet creation failed");
            Err(e)
        }
    }
}
```

### 3. Include Timing Information

```rust
// ❌ WRONG - no timing info
info!(operation = "encrypt", "Encryption complete");

// ✅ CORRECT - includes duration
let start = std::time::Instant::now();
perform_encryption()?;
let duration_ms = start.elapsed().as_millis() as u64;
info!(operation = "encrypt", duration_ms = duration_ms, "Encryption complete");
```

### 4. Use Structured Logging

```rust
use tracing::info;

// ❌ WRONG - unstructured message
info!("Wallet alice on testnet encrypted with aes-256-gcm");

// ✅ CORRECT - structured fields
info!(
    wallet = "alice",
    network = "testnet",
    algorithm = "aes-256-gcm",
    "Wallet encrypted"
);
```

### 5. Provide Debugging Context

```rust
// ❌ WRONG - generic error
error!("Operation failed");

// ✅ CORRECT - specific context
error!(
    operation = "fund_wallet",
    wallet = "alice",
    network = "testnet",
    requested_amount = 1000,
    error_type = "InsufficientFunds",
    "Failed to fund wallet - faucet has insufficient balance"
);
```

---

## Wallet Logging Patterns

### Wallet Creation Logging

```rust
use tracing::info;
use std::time::Instant;

pub fn handle_create(cmd: CreateCommand) -> Result<()> {
    let start = Instant::now();
    
    // Create wallet
    let wallet = Wallet::new(&cmd.name, &cmd.network)?;
    
    // Encrypt if requested
    if cmd.encrypt {
        wallet.encrypt(&cmd.passphrase)?;
    }
    
    let duration_ms = start.elapsed().as_millis() as u64;
    
    // Log creation
    info!(
        wallet = &cmd.name,
        network = &cmd.network,
        encrypted = cmd.encrypt,
        duration_ms = duration_ms,
        status = "success",
        "Wallet created successfully"
    );
    
    Ok(())
}
```

### Wallet Access Logging

```rust
use tracing::info;
use crate::utils::logging::redact_public_key;
use tracing::Level;

pub fn handle_show(cmd: ShowCommand) -> Result<()> {
    let wallet = load_wallet(&cmd.name)?;
    
    if cmd.reveal {
        // Log sensitive operation
        info!(
            wallet = &cmd.name,
            reveal_requested = true,
            decryption_status = "success",
            status = "success",
            "Secret key revealed for wallet"
        );
    }
    
    // Log balance query
    let public_key = wallet.public_key();
    let balance = query_balance(&public_key)?;
    
    info!(
        wallet = &cmd.name,
        public_key = %redact_public_key(public_key, Level::INFO),
        balance = balance,
        status = "success",
        "Wallet details retrieved"
    );
    
    Ok(())
}
```

### Wallet Encryption Logging

```rust
use tracing::info;
use std::time::Instant;

pub fn handle_encrypt(cmd: EncryptCommand) -> Result<()> {
    let start = Instant::now();
    
    let wallet = load_wallet(&cmd.name)?;
    
    // Perform encryption
    wallet.encrypt(&cmd.passphrase)?;
    
    let duration_ms = start.elapsed().as_millis() as u64;
    
    // Log encryption operation
    info!(
        wallet = &cmd.name,
        algorithm = "aes-256-gcm",
        kdf_algorithm = "argon2",
        kdf_iterations = 100000,  // Parameter size, not the actual value
        duration_ms = duration_ms,
        status = "success",
        "Wallet encrypted with strong encryption"
    );
    
    Ok(())
}
```

---

## Plugin Logging Patterns

### Plugin Load Logging

```rust
use tracing::info;

pub fn load_plugin(path: &str, name: &str) -> Result<Plugin> {
    let plugin = unsafe {
        pm.load_plugin(path)
            .context("Failed to load plugin")?
    };
    
    let plugin_version = plugin.version();
    let starforge_version = env!("CARGO_PKG_VERSION");
    
    // Verify compatibility
    let compatible = verify_compatibility(plugin_version, starforge_version);
    
    if !compatible {
        error!(
            plugin = name,
            plugin_version = plugin_version,
            starforge_version = starforge_version,
            compatible = false,
            status = "failed",
            "Plugin version incompatible with StarForge"
        );
        return Err(anyhow::anyhow!("Version incompatibility"));
    }
    
    // Log successful load
    info!(
        plugin = name,
        plugin_version = plugin_version,
        starforge_version = starforge_version,
        compatible = true,
        source_path = path,
        status = "success",
        "Plugin loaded successfully"
    );
    
    Ok(plugin)
}
```

### Plugin Execution Logging

```rust
use tracing::info;
use std::time::Instant;

pub fn execute_plugin(plugin: &Plugin, args: &[String]) -> Result<()> {
    let start = Instant::now();
    
    // Execute with sanitized arguments
    let sanitized_args = sanitize_args(args);
    
    let result = plugin.execute(&sanitized_args)?;
    
    let duration_ms = start.elapsed().as_millis() as u64;
    
    // Log execution
    info!(
        plugin = plugin.name(),
        command = "execute",
        arg_count = args.len(),  // Number of args, not the args themselves
        duration_ms = duration_ms,
        exit_code = result.exit_code,
        status = "success",
        "Plugin executed successfully"
    );
    
    Ok(())
}
```

---

## Deployment Logging Patterns

### Contract Validation Logging

```rust
use tracing::info;

pub fn handle_validate(cmd: ValidateCommand) -> Result<()> {
    let wasm_data = std::fs::read(&cmd.wasm_path)?;
    let file_size = wasm_data.len();
    
    let result = validate_contract(&wasm_data)?;
    
    info!(
        operation = "contract_validate",
        file_path = &cmd.wasm_path,
        file_size = file_size,
        validation_result = "success",
        warning_count = result.warnings.len(),
        error_count = result.errors.len(),
        status = "success",
        "Contract validated"
    );
    
    Ok(())
}
```

### Contract Deployment Logging

```rust
use tracing::info;
use crate::utils::logging::redact_public_key;
use tracing::Level;

pub fn handle_deploy(cmd: DeployCommand) -> Result<()> {
    // Validate contract
    validate_contract(&cmd.wasm_path)?;
    
    // Calculate WASM hash
    let wasm_hash = calculate_wasm_hash(&cmd.wasm_path)?;
    
    // Perform deployment
    let result = deploy_contract(
        &cmd.wasm_path,
        &cmd.network,
        &cmd.account,
    )?;
    
    // Log deployment
    info!(
        operation = "contract_deploy",
        network = &cmd.network,
        account = %redact_public_key(&cmd.account, Level::INFO),
        wasm_hash = %wasm_hash,
        contract_address = %result.address,
        gas_used = result.gas_used,
        transaction_hash = %result.tx_hash,
        status = "success",
        "Contract deployed successfully"
    );
    
    Ok(())
}
```

---

## Error and Exception Logging

### Safe Error Logging

```rust
use tracing::error;

// ✅ CORRECT - specific error type without exposing secrets
match wallet.decrypt(passphrase) {
    Ok(decrypted) => {
        info!(wallet = name, "Wallet decrypted");
        Ok(decrypted)
    }
    Err(e) => {
        error!(
            wallet = name,
            error_type = "DecryptionFailed",
            attempt = 1,
            status = "failed",
            "Failed to decrypt wallet"
        );
        Err(e)
    }
}
```

### Error Context Without Secrets

```rust
use tracing::error;

// ❌ WRONG - includes sensitive information
error!(
    wallet = name,
    error = format!("Decryption failed: {}", e),
    status = "failed"
);

// ✅ CORRECT - sanitized error message
error!(
    wallet = name,
    error_type = "DecryptionFailed",
    error_cause = "InvalidKeyDerivation",
    status = "failed"
);
```

### Retry Logic Logging

```rust
use tracing::{warn, error};

fn fund_with_retry(wallet: &str, amount: u64, max_retries: usize) -> Result<()> {
    for attempt in 1..=max_retries {
        match request_funds(wallet, amount) {
            Ok(tx_hash) => {
                info!(
                    wallet = wallet,
                    attempt = attempt,
                    status = "success",
                    "Wallet funded successfully"
                );
                return Ok(());
            }
            Err(e) if attempt < max_retries => {
                warn!(
                    wallet = wallet,
                    attempt = attempt,
                    error_type = format!("{:?}", e),
                    will_retry = true,
                    "Funding attempt failed, will retry"
                );
            }
            Err(e) => {
                error!(
                    wallet = wallet,
                    attempt = attempt,
                    error_type = format!("{:?}", e),
                    will_retry = false,
                    status = "failed",
                    "Funding failed after {} attempts",
                    max_retries
                );
                return Err(e);
            }
        }
    }
    Ok(())
}
```

---

## Testing Security Logging

### Unit Test for Logging

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tracing::Level;
    use tracing_subscriber::EnvFilter;

    #[test]
    fn test_wallet_creation_logs() {
        // Setup logging capture
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .with_env_filter(EnvFilter::new("debug"))
            .finish();

        let _guard = tracing::subscriber::set_default(subscriber);

        // Perform operation
        let result = create_wallet("test", "testnet", false);
        assert!(result.is_ok());

        // Verify operation completed (logs would be captured by subscriber)
    }

    #[test]
    fn test_no_secret_keys_in_logs() {
        // This test ensures secret keys are never logged
        let secret_key = "SBZJ3SHRVGPXDT3XNQDRT5PHZFSF6OKOPECJ...";
        
        // Create wallet
        let wallet = Wallet::with_key(secret_key).unwrap();
        
        // Attempt to log wallet (should NOT contain secret)
        let log_output = format!("{:?}", wallet);
        assert!(!log_output.contains("SBZJ3SHRVGPXDT3XNQDRT5PHZFSF6OKO"));
    }
}
```

### Integration Test for Audit Trails

```rust
#[test]
fn test_wallet_operations_create_audit_trail() {
    // Create temporary log file
    let log_dir = tempfile::tempdir().unwrap();
    
    // Initialize with JSON logging
    let config = LogConfig {
        format: LogFormat::Json,
        log_dir: Some(log_dir.path().to_path_buf()),
        ..Default::default()
    };
    let _ = logging::init(config);
    
    // Perform wallet operations
    create_wallet("alice", "testnet", false).unwrap();
    fund_wallet("alice", "testnet", 1000).unwrap();
    
    // Verify logs were written
    let log_files = std::fs::read_dir(log_dir.path()).unwrap();
    assert!(log_files.count() > 0, "Logs should have been written");
    
    // Parse and verify log content
    let log_content = std::fs::read_to_string(
        log_dir.path().join("starforge.log")
    ).unwrap();
    
    assert!(log_content.contains("wallet_create"));
    assert!(log_content.contains("wallet_fund"));
    assert!(!log_content.contains("secret"));
}
```

---

## Code Review Checklist

When reviewing code for security logging, verify:

- [ ] **No secrets logged** - Secret keys, passphrases, or derived keys never appear
- [ ] **Operation outcome logged** - Success and failure both recorded
- [ ] **Timing included** - Duration for security operations tracked
- [ ] **Structured fields** - Using `info!(field = value, ...)` format
- [ ] **Consistent naming** - Field names match audit trail spec
- [ ] **Error details safe** - Error messages don't expose sensitive data
- [ ] **Appropriate log level** - INFO for operations, DEBUG for detailed info
- [ ] **Context sufficient** - Logs can be understood in isolation
- [ ] **No performance impact** - Logging doesn't slow down operations
- [ ] **Tests included** - Logging behavior verified in tests

---

## Common Mistakes

### Mistake 1: Logging Passphrases

```rust
// ❌ WRONG
let passphrase = prompt_for_passphrase();
info!("User provided passphrase: {}", passphrase);  // NEVER DO THIS

// ✅ CORRECT
let passphrase = prompt_for_passphrase();
info!("Passphrase accepted");  // Just log that it was provided
```

### Mistake 2: Logging Function Entry/Exit for Everything

```rust
// ❌ WRONG - too verbose
fn process_item(item: &Item) -> Result<()> {
    trace!("Entering process_item()");
    
    for sub_item in &item.sub_items {
        trace!("Processing sub_item: {:?}", sub_item);  // Way too verbose
    }
    
    trace!("Exiting process_item()");
    Ok(())
}

// ✅ CORRECT - log only security-relevant operations
fn process_item(item: &Item) -> Result<()> {
    let result = perform_operation(item)?;
    info!(operation = "process_item", status = "success");
    Ok(result)
}
```

### Mistake 3: Inconsistent Field Names

```rust
// ❌ WRONG - inconsistent naming
info!(
    wallet = "alice",       // "wallet" name here
    account = "alice",      // but "account" here
    operation = "create",   // "operation" here
    cmd = "create",         // and "cmd" here
);

// ✅ CORRECT - consistent field names
info!(
    wallet = "alice",
    operation = "create",
    network = "testnet",
    status = "success",
);
```

### Mistake 4: Redacting When Not Needed

```rust
// ❌ WRONG - redacting non-sensitive public key
info!(
    wallet = "alice",
    public_key = %redact_public_key(key, Level::INFO),  // Unnecessary
);

// ✅ CORRECT - public keys are safe to log at INFO level
info!(
    wallet = "alice",
    public_key = %key,  // Safe to log
);
```

### Mistake 5: Not Logging Failures

```rust
// ❌ WRONG - only logs success
fn deploy_contract(path: &str) -> Result<()> {
    let result = execute_deploy(path)?;
    info!(contract = path, status = "success");
    Ok(())
}

// ✅ CORRECT - logs both success and failure
fn deploy_contract(path: &str) -> Result<()> {
    match execute_deploy(path) {
        Ok(result) => {
            info!(
                contract = path,
                address = %result.address,
                status = "success",
            );
            Ok(())
        }
        Err(e) => {
            error!(
                contract = path,
                error_type = format!("{:?}", e),
                status = "failed",
            );
            Err(e)
        }
    }
}
```

---

## Further Reading

- [SECURITY_LOGGING_GUIDE.md](SECURITY_LOGGING_GUIDE.md) - Complete security logging guide
- [AUDIT_TRAIL_DOCUMENTATION.md](AUDIT_TRAIL_DOCUMENTATION.md) - Audit trail setup and review
- [src/utils/logging.rs](src/utils/logging.rs) - Logging infrastructure implementation

---

*Last updated: 2026-06-01*  
*Issue #223: Improve security logging and auditability*
