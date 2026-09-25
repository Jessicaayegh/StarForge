# Mutation Testing: Wallet Import/Backup

## Overview

Mutation testing validates that the test suite for wallet backup and import cryptographic code is sensitive enough to catch deliberate logic errors. This document explains how to run mutation tests, interpret results, and fix surviving mutants.

**Module under test:** `src/utils/wallet_import.rs` (backup parsing, envelope decryption, integrity verification)

**Kill rate threshold:** ≥ 85% (at least 85% of mutants must be killed by tests)

**Why 85%?** Wallet backup/import is high-impact cryptographic code. A logic error here could silently leak secret keys or accept forged backups.

---

## Quick Start

### Run mutation tests locally

```bash norun
# Install cargo-mutants (one-time)
cargo install cargo-mutants

# Run mutation tests on wallet_import.rs only
cargo mutants --file src/utils/wallet_import.rs --jobs 4

# With a limit (faster for initial checks)
cargo mutants --file src/utils/wallet_import.rs --jobs 4 --max-mutants 50
```

### Interpret the results

```
Generate: 47 mutants
Killed:   42 mutants ✓
Survived: 5 mutants  ✗
Timeout:  0 mutants
Unviable: 0 mutants

Kill rate: 89.4% (above 85% threshold) ✓
```

If kill rate < 85%, the CI gate will fail on PR checks.

---

## Understanding Mutants

A **mutant** is a small code change injected by cargo-mutants. Examples:

| Original code | Mutant | What it tests |
|---|---|---|
| `if tag == expected` | `if tag != expected` | Does validation check the integrity tag? |
| `len > MAX_SIZE` | `len < MAX_SIZE` | Is the size limit enforced correctly? |
| `return Ok(())` | `return Err(...)` | Are error paths exercised? |
| `byte_array[i]` | `byte_array[i ^ 0xFF]` | Are field values properly checked? |

**Killed mutant** = test suite caught the error → good

**Surviving mutant** = test suite missed the error → potential security gap

---

## CI Workflow

The mutation testing CI job (`.github/workflows/mutation-testing.yml`) runs:

1. **Generate mutants** for `src/utils/wallet_import.rs`
2. **Run all tests** against each mutant
3. **Score** how many were killed
4. **Report** results to PR comments + artifacts
5. **Gate** (fail PR if kill rate < threshold)

### Triggering manually

```bash norun
# From GitHub Actions UI, click "Run workflow" on "Mutation Testing (Wallet Import)"
# You can override:
#   - min_kill_rate (default 0.85)
#   - max_mutants (default unlimited)
```

Or via `gh` CLI:

```bash norun
gh workflow run mutation-testing.yml \
  -f min_kill_rate=0.90 \
  -f max_mutants=100
```

### Viewing results

1. **PR comment** — shows summary (kill rate, killed/survived count)
2. **Artifacts** — download `mutation-reports/` for detailed analysis
3. **JSON report** — machine-readable format for dashboards

---

## Fixing Surviving Mutants

When a mutant survives, it means a potential bug is undetected. Here's how to fix it:

### 1. Identify the survivor

From the `mutation-reports/wallet_import.json` artifact:

```json
{
  "survived": [
    {
      "line": 515,
      "function": "parse_wallet_backup",
      "original": "self.version != \"2\"",
      "mutant": "self.version == \"2\"",
      "description": "Changed != to =="
    }
  ]
}
```

### 2. Read the code

```rust
// Line 515 in src/utils/wallet_import.rs
pub fn parse_wallet_backup(contents: &str) -> Result<ParsedBackup> {
    // ...
    if backup.version != "2" && backup.version != "1" {
        bail!("Unsupported backup version: {}", backup.version);
    }
    // ...
}
```

The mutant changes `!=` to `==`, making it accept only version "2" and reject version "1". **If no test covers version "1" backups, the mutant survives.**

### 3. Add or strengthen a test

```rust
#[test]
fn v1_backup_is_accepted_with_migration_warning() {
    let v1_backup = serde_json::json!({
        "version": "1",
        "wallets": [{
            "name": "alice",
            "public_key": "GDRXMZDQW34QHX6F5U6FFWJZZZDQ4KYWJO65HS4CUT62X7Y7RXYWXE4T",
            "secret_key": "S...",
            "network": "testnet"
        }]
    });
    
    let result = parse_wallet_backup(&serde_json::to_string(&v1_backup).unwrap());
    assert!(result.is_ok(), "v1 backup should be accepted");
    // This test now KILLS the mutant that changes != to ==
}
```

### 4. Verify the mutant is killed

```bash norun
# Re-run with just that file
cargo mutants --file src/utils/wallet_import.rs --max-mutants 50
```

---

## Common Survivors and How to Kill Them

### Pattern 1: Missing boundary checks

**Mutant:** `len > MAX` → `len >= MAX` (or vice versa)

**Why it survives:** No test at exactly the boundary.

**Fix:**

```rust
#[test]
fn a_backup_at_the_wallet_limit_is_accepted_and_one_over_is_not() {
    let at_limit = create_backup_with_n_wallets(MAX_WALLETS);
    assert!(parse_wallet_backup(&at_limit).is_ok());
    
    let over_limit = create_backup_with_n_wallets(MAX_WALLETS + 1);
    assert!(parse_wallet_backup(&over_limit).is_err());
}
```

### Pattern 2: Missing null checks

**Mutant:** `value.is_some()` → `value.is_none()`

**Why it survives:** The code only tested the happy path.

**Fix:**

```rust
#[test]
fn missing_required_field_is_rejected() {
    let mut backup = valid_backup();
    backup.wallets[0].secret_key = None; // or set to null in JSON
    assert!(validate_entry(&backup.wallets[0]).is_err());
}
```

### Pattern 3: Comparison operator flips

**Mutant:** `<` → `<=`, `==` → `!=`

**Why it survives:** No test that exercises the condition in both directions.

**Fix:**

```rust
#[test]
fn accepts_valid_and_rejects_invalid_separately() {
    // Test the positive case
    assert!(check_something(&valid_input()).is_ok());
    
    // Test the negative case (not just trusting the opposite)
    assert!(check_something(&invalid_input()).is_err());
}
```

### Pattern 4: Missing postcondition checks

**Mutant:** Removal of an assertion or return value check

**Why it survives:** The test doesn't verify the output.

**Fix:**

```rust
#[test]
fn encryption_result_is_valid_base64() {
    let result = encrypt_secret("passphrase", "data", None).unwrap();
    
    // Verify the result is actually base64, not just non-empty
    assert!(base64::decode(&result).is_ok());
}
```

---

## Skipped Functions

Some functions are intentionally skipped from mutation testing (see `.cargo-mutants.toml`):

| Pattern | Reason |
|---|---|
| `.*::fmt` | Display/formatting (not cryptographic logic) |
| `prompt_.*` | Interactive prompts (require TTY, not security-critical) |
| `.*telemetry.*` | Telemetry helpers (diagnostic, not business logic) |

If a function **should** be tested but is being skipped, add a specific exclusion or remove the skip.

---

## Configuration

### `.cargo-mutants.toml`

| Setting | Value | Meaning |
|---|---|---|
| `timeout_multiplier` | 3.0 | Allow each test run up to 3× normal time |
| `minimum_test_timeout` | 30s | Minimum timeout is 30 seconds |
| `examine_globs` | `["src/utils/wallet_import.rs", ...]` | Only mutate these files |
| `skip_calls` | `[".*::fmt", ...]` | Skip these function patterns |
| `minimum_kill_rate` | 0.85 | Fail CI if kill rate < 85% |

---

## Local Development Workflow

### Before opening a PR

1. **Run tests** to ensure baseline is green:
   ```bash norun
   cargo test --lib wallet_import
   ```

2. **Run mutation tests** to see your coverage:
   ```bash norun
   cargo mutants --file src/utils/wallet_import.rs --jobs 4
   ```

3. **Review survivors** and decide whether to:
   - Add tests (if the mutant represents a real gap)
   - Skip the function (if it's not security-critical)

4. **Target 85%+** before pushing

### Interpreting local results

```
Killed:  42 ✓
Survived: 5 ✗
Timeout:  1 ?
Unviable: 0

Kill rate: 89.4%
```

**Survived mutants** are in `target/mutants.out/missed.txt`:

```
Line 515 in parse_wallet_backup: Changed != to ==
Line 620 in check_wallet_name: Removed bidi check
...
```

### CI vs. local runs

CI may see different results due to:
- Different test ordering (timing-dependent bugs)
- Different machine (cached artifacts, slower machine = more timeouts)
- Different `PROPTEST_CASES` (more cases = higher chance to kill random operators)

**Always run locally before pushing** to catch low kill rates early.

---

## Mutation Testing vs. Coverage

| Metric | What it measures | Use for |
|---|---|---|
| **Coverage** | % of lines executed | Spotting untouched code |
| **Kill rate** | % of intentional errors caught | Validating test sensitivity |

**Coverage ≠ Quality:**
- 100% line coverage + 50% kill rate = tests are too shallow
- 70% line coverage + 95% kill rate = tests are deep (even if some lines missed)

**For wallet_import.rs:** Target both 80%+ coverage AND 85%+ kill rate.

---

## FAQ

### Q: Why 85% and not 100%?

A: Some mutations are inherently unkillable (e.g., unreachable code after an early return). Pragmatically, 85% balances thoroughness with avoiding over-testing trivial branches.

### Q: Can I skip a survivor?

A: Only if:
1. The mutant is in a function on the `skip_calls` list, OR
2. The code is intentionally unreachable and you document why

For security-critical functions like `compute_integrity_tag` or `parse_encrypted_envelope`, **do not skip survivors**.

### Q: What if mutation tests timeout?

A: This usually means a mutant broke an infinite loop condition. The test harness times out waiting for the test to finish. **This counts as "killed"** (the mutant broke the code). If you see many timeouts:

1. Check the test baseline (ensure it's green)
2. Reduce `timeout_multiplier` in `.cargo-mutants.toml` if timeouts are legitimate
3. File an issue if a survivor consistently times out (may indicate a real bug)

### Q: How often should I run this?

A: 
- **Locally:** Before every PR (or use Git pre-commit hook)
- **CI:** On every PR (automated check) + weekly scheduled run (drift detection)

### Q: Can I run mutation tests on the whole codebase?

A: Yes, but it's **very slow** (hours to days). For focused testing, use:

```bash norun
# Just wallet_import
cargo mutants --file src/utils/wallet_import.rs

# Multiple files
cargo mutants --file src/utils/wallet_import.rs --file src/utils/crypto.rs
```

---

## See Also

- [FUZZING_GUIDE.md](FUZZING_GUIDE.md) — Property-based testing and fuzzing
- [CI_ENFORCEMENT.md](CI_ENFORCEMENT.md) — CI workflow details
- [cargo-mutants documentation](https://mutants.rs/)
