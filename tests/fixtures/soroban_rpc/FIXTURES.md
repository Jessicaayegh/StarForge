# Soroban RPC Fixtures

This directory contains recorded Soroban RPC and Horizon API responses used for offline dry-run testing.

## Fixture Format

All files are JSON responses from:
- **Soroban RPC** (`soroban_rpc_url/rpc`): Transaction simulation, ledger inspection
- **Horizon** (`horizon_url`): Account information, balances

## Available Fixtures

### Success Path
- `simulate_success.json` — Successful `simulateTransaction` response with estimated fees

### Error Scenarios
- `rpc_error.json` — JSON-RPC protocol error (e.g., malformed request)
- `simulate_error_top_level.json` — Error at top level of RPC response (invalid params)
- `simulate_error_in_results.json` — Transaction simulation failed (invocation error)
- `insufficient_balance.json` — Account has insufficient native XLM to pay fees
- `malformed_wasm_path.json` — WASM file path doesn't exist or is inaccessible

### Account/Ledger
- `get_ledger_entries_success.json` — Account exists with sufficient balance
- `get_ledger_entries_empty.json` — Account not found or has zero balance

## Versioning

**Current version:** 1.0

Fixtures are frozen after creation to ensure offline CI stability. To update:

1. Run integration tests against live RPC (or record manually)
2. Validate responses for PII (remove private keys, real account IDs)
3. Anonymize with test data (e.g., GAAAA... test keys)
4. Document the scenario in this file
5. Update version number and changelog entry

## Usage in Tests

```rust
use std::fs;

#[test]
fn deploy_dry_run_handles_insufficient_balance() {
    let response = fs::read_to_string("tests/fixtures/soroban_rpc/insufficient_balance.json")
        .expect("load fixture");
    
    let rpc_response: serde_json::Value = serde_json::from_str(&response)
        .expect("parse JSON");
    
    // Verify the fixture structure
    assert!(rpc_response.get("error").is_some(), "fixture should contain error");
    assert_eq!(
        rpc_response["error"]["code"].as_i64(),
        Some(-32603),
        "insufficient balance error code"
    );
}
```

## Fixture Lifecycle

1. **Generation** — Recorded from live RPC when new scenarios are discovered
2. **Anonymization** — PII removed, real keys/IDs replaced with test data
3. **Validation** — Checked to ensure correct structure + expected error codes
4. **Archival** — Committed to repo with changelog entry
5. **Usage** — Loaded in tests without network access (CI, offline dev)

## Changelog

### v1.0 (2024-09-23)
- Initial fixtures: success, rpc_error, simulate_error_top_level, simulate_error_in_results
- Added insufficient_balance, malformed_wasm_path scenarios
- Ledger entry fixtures for account balance checks
