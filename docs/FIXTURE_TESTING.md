# Fixture-Based Testing Guide

## Overview

StarForge uses recorded API responses ("fixtures") to test deployment logic offline without requiring live Horizon/Soroban RPC access. This ensures CI is fast, reliable, and deterministic.

## Fixture Locations

| Path | Purpose |
|------|---------|
| `tests/fixtures/soroban_rpc/` | Recorded Soroban RPC simulation responses |
| `tests/fixtures/soroban_rpc/FIXTURES.md` | Fixture inventory and metadata |
| `tests/deploy_dry_run.rs` | Dry-run tests using fixtures |

## Fixture Scenarios

### Success Path
- **`simulate_success.json`** — Valid deployment with fee estimate
  - Used by: `dry_run_flow_with_success_fixture()`
  - Contains: `minResourceFee`, `transactionData`, events

### Failure Modes (Required: ≥3)

1. **Insufficient Balance**
   - **File:** `insufficient_balance.json`
   - **Error code:** -32603 (Internal Error)
   - **Message:** "insufficient balance"
   - **Used by:** `fixture_insufficient_balance_prevents_deployment()`
   - **Scenario:** Account doesn't have enough native XLM to pay fees

2. **Malformed WASM Path**
   - **File:** `malformed_wasm_path.json`
   - **Error code:** -32600 (Invalid Request)
   - **Message:** "WASM file not found or is not readable"
   - **Used by:** `fixture_rpc_error_is_well_formed()`
   - **Scenario:** WASM file path is invalid or inaccessible

3. **Generic RPC Error**
   - **File:** `rpc_error.json`
   - **Error code:** -32600 (Invalid Request)
   - **Message:** "Invalid request"
   - **Used by:** `fixture_rpc_error_is_well_formed()`
   - **Scenario:** Malformed RPC request (e.g., missing params)

### Additional Fixtures

- **`simulate_error_top_level.json`** — RPC error at envelope level
- **`simulate_error_in_results.json`** — Simulation succeeded but invocation failed
- **`get_ledger_entries_success.json`** — Account found with sufficient balance
- **`get_ledger_entries_empty.json`** — Account not found

## Testing with Fixtures

### Loading Fixtures

```rust
use std::fs;

#[test]
fn my_fixture_test() {
    let response = fs::read_to_string("tests/fixtures/soroban_rpc/simulate_success.json")
        .expect("load fixture");
    
    let rpc_response: serde_json::Value = serde_json::from_str(&response)
        .expect("parse JSON");
    
    // Verify structure
    assert!(rpc_response.get("result").is_some());
}
```

### Asserting CLI Command Stability

The generated Stellar CLI command must be **deterministic** — same inputs always produce identical output:

```rust
#[test]
fn stellar_cli_command_remains_stable() {
    let plan1 = DeployPlan::from_wasm(wasm, "testnet", "deployer", pubkey);
    let cmd1 = plan1.generate_cli_command();
    
    let plan2 = DeployPlan::from_wasm(wasm, "testnet", "deployer", pubkey);
    let cmd2 = plan2.generate_cli_command();
    
    assert_eq!(cmd1, cmd2, "commands must be deterministic");
}
```

## Refreshing Fixtures Safely

### When to Refresh

- Soroban RPC API changes
- Fee structures change
- Error messages become outdated
- New failure scenarios discovered

### Refresh Procedure

1. **Record live RPC response** (with real credentials, if possible):
   ```bash norun
   curl -X POST https://soroban-testnet.stellar.org/rpc \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc": "2.0", "id": 1, "method": "simulateTransaction", ...}'
   ```

2. **Anonymize the response:**
   - Remove personal/private keys
   - Replace real account IDs with test keys (e.g., `GAAAA...`)
   - Redact any sensitive data

3. **Validate structure:**
   ```bash norun
   # Ensure it's valid JSON
   jq . tests/fixtures/soroban_rpc/new_fixture.json
   
   # Run tests to verify
   cargo test --test deploy_dry_run
   ```

4. **Commit and document:**
   ```bash norun
   git add tests/fixtures/soroban_rpc/new_fixture.json
   # Update FIXTURES.md with scenario description
   git commit -m "chore: refresh simulate_success fixture to v1.1"
   ```

### Example: Updating `simulate_success.json`

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "latestLedger": 12345,
    "minResourceFee": "58181",
    "cost": { "cpuInsns": 150000, "memBytes": 2048 },
    "transactionData": "...",
    "results": [ { "xdr": "...", "auth": [] } ],
    "events": [ "...", "..." ],
    "returnValue": "success_value"
  }
}
```

### Anonymization Checklist

- [ ] No real Stellar public/secret keys
- [ ] No real account IDs
- [ ] No real transaction hashes
- [ ] No real contract IDs
- [ ] Use test keys (e.g., `GAAAA...`, `SBBB...`)
- [ ] Verify JSON is well-formed

---

## Versioning

Fixtures follow semantic versioning in `FIXTURES.md`:

**Current version:** 1.0

When updating:
1. Increment version (1.0 → 1.1)
2. Document changes in changelog
3. Commit with clear message

```markdown
### v1.1 (2024-10-15)
- Updated simulate_success.json to reflect Soroban v20 fee structure
- Added get_network_info.json for network capability detection
```

---

## Fixture Lifecycle

```
Discovery → Recording → Anonymization → Validation → Archival → Usage
```

### 1. Discovery
- New failure scenario found in manual testing
- Real RPC response captured

### 2. Recording
- Saved to a JSON file

### 3. Anonymization
- Sensitive data removed (real keys, IDs)
- Test data substituted

### 4. Validation
- Structure verified (valid JSON, required fields)
- Tests run offline and pass

### 5. Archival
- Committed to git
- Version number incremented
- Changelog updated in FIXTURES.md

### 6. Usage
- Loaded by tests without network access
- Used in CI for fast, deterministic runs

---

## Best Practices

### ✓ Do

- Keep fixtures small (< 10 KB)
- Include comments explaining scenario
- Test both success and error paths
- Verify CLI command stability across versions
- Update fixtures when RPC API changes

### ✗ Don't

- Commit real credentials or secret keys
- Use fixtures with real account IDs
- Leave stale fixtures without tests
- Change fixture structure without updating tests
- Mix success and error cases in one fixture

---

## Integration with CI

The dry-run fixture tests run in CI:

```yaml
# .github/workflows/ci.yml
- name: Deploy dry-run tests (offline)
  run: cargo test --test deploy_dry_run
  # No network access required
```

### Offline CI Benefits

- **Fast:** No network latency
- **Reliable:** No flaky RPC servers
- **Deterministic:** Same result every run
- **Private:** No credentials needed

---

## Adding New Fixtures

1. Create the JSON file:
   ```bash norun
   touch tests/fixtures/soroban_rpc/my_scenario.json
   ```

2. Populate with recorded/anonymized response

3. Update `FIXTURES.md` with scenario description

4. Add a test that loads and validates:
   ```rust
   #[test]
   fn my_scenario_fixture_is_valid() {
       let response = load_fixture("my_scenario.json");
       assert!(response.get("error").is_some());
   }
   ```

5. Verify it loads offline:
   ```bash norun
   cargo test fixture_
   ```

---

## Troubleshooting

### "Fixture file not found"

**Check:**
- File is in `tests/fixtures/soroban_rpc/`
- Filename matches exactly (case-sensitive)
- File is committed to git

### "Invalid JSON in fixture"

**Fix:**
```bash norun
# Validate JSON syntax
jq . tests/fixtures/soroban_rpc/broken.json

# Pretty-print for editing
jq -S . tests/fixtures/soroban_rpc/broken.json > /tmp/fixed.json
mv /tmp/fixed.json tests/fixtures/soroban_rpc/broken.json
```

### "CLI command changed between runs"

**Cause:** Determinism issue in command generation

**Debug:**
```rust
let cmd1 = plan.generate_cli_command();
let cmd2 = plan.generate_cli_command();
assert_eq!(cmd1, cmd2, "Expected:\n{}\nGot:\n{}", cmd1, cmd2);
```

---

## See Also

- [FUZZING_GUIDE.md](FUZZING_GUIDE.md) — Property-based testing
- [.cargo-mutants.toml](.cargo-mutants.toml) — Mutation testing
- [tests/fixtures/soroban_rpc/FIXTURES.md](tests/fixtures/soroban_rpc/FIXTURES.md) — Fixture inventory
