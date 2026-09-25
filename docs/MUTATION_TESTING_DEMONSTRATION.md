# Mutation Testing Demonstration

This document demonstrates how cargo-mutants works with the wallet_import module and shows an example of an intentional mutant being killed by the test suite.

## Setup

```bash norun
# Install cargo-mutants (one-time)
cargo install cargo-mutants

# Verify cargo-mutants is installed
cargo mutants --version
```

## Running Mutation Tests

### Quick check (first 10 mutants)

```bash norun
cd /path/to/StarForge
cargo mutants --file src/utils/wallet_import.rs --max-mutants 10 --jobs 4
```

Expected output:

```
Generate: 10 mutants
...
Killed:   8 mutants ✓
Survived: 2 mutants ✗
Timeout:  0 mutants
Unviable: 0 mutants

Kill rate: 80.0%
```

### Full run (all mutants in wallet_import.rs)

```bash norun
cargo mutants --file src/utils/wallet_import.rs --jobs 4
```

This will take 10-30 minutes depending on machine speed. Expected kill rate: **85%+**

### Example: Killing a specific mutant

The wallet_import module has many integrity checks. Let's trace through one:

#### Test that KILLS the mutant

```rust
// From src/utils/wallet_import.rs, line ~1110
#[test]
fn v2_backup_with_valid_tag_is_accepted() {
    let doc = backup_json_v2(&wallet_json("alice"));
    let backup: WalletBackup = serde_json::from_str(&doc).expect("valid json");
    
    // Compute the expected tag
    let tag = compute_integrity_tag(&backup, BACKUP_HMAC_KEY)
        .expect("tag computation");
    
    let mut backup = backup;
    backup.integrity_tag = Some(tag.clone());
    
    // This should parse and accept
    let parsed = parse_wallet_backup(&serde_json::to_string(&backup).unwrap())
        .expect("v2 backup with valid tag must parse");
    
    assert!(parsed.warnings.is_empty(), "no warnings for valid tag");
}
```

#### The code being tested

```rust
// From src/utils/wallet_import.rs, line ~499
pub fn verify_integrity_tag(backup: &WalletBackup, tag: &str, key: &[u8]) -> bool {
    let expected = compute_integrity_tag(backup, key)
        .ok()
        .map(|t| t.to_lowercase());
    
    expected.as_deref() == Some(tag) // ← This line is the one being mutated
}
```

#### Potential mutant

**Original code:**
```rust
expected.as_deref() == Some(tag)
```

**Mutant (comparison flip):**
```rust
expected.as_deref() != Some(tag)  // Changed == to !=
```

#### Why the test kills this mutant

1. The test calls `compute_integrity_tag()` to get a valid tag
2. It sets `backup.integrity_tag = Some(tag.clone())`
3. It parses the backup and expects `is_ok()` to be true
4. Inside parse_wallet_backup, `verify_integrity_tag()` is called
5. If the mutant flips the comparison to `!=`, the verification fails
6. The parse returns Err, not Ok
7. The test assertion `parse_wallet_backup(...).expect(...)` panics
8. **The test fails → the mutant is killed ✓**

### Running with survivors analysis

```bash norun
cargo mutants --file src/utils/wallet_import.rs --jobs 4 --output mutations.out
```

Then examine survivors:

```bash norun
cat mutations.out/missed.txt | head -20
```

Sample output:

```
src/utils/wallet_import.rs line 515 in parse_wallet_backup: 
  Removed call to check_wallet_name
  Original: check_wallet_name(&wallet.name)?;
  Mutant:   // check_wallet_name(&wallet.name)?;

src/utils/wallet_import.rs line 620 in is_deceptive_char:
  Changed return value
  Original: return false;
  Mutant:   return true;
```

### Fixing a survivor

If you see a survivor like the `is_deceptive_char` example above:

**Original code (line ~620):**
```rust
fn is_deceptive_char(c: char) -> bool {
    matches!(c, '\u{202E}' | '\u{061C}' | '\u{200F}')
}

#[test]
fn bidi_and_zero_width_names_are_rejected() {
    // This test exists but might not be comprehensive enough
}
```

**The survivor:** Mutant changes `false` to `true` at line 620, making some valid characters appear deceptive.

**How to kill it:** Add a test that checks a normal character:

```rust
#[test]
fn normal_characters_are_not_deceptive() {
    assert!(!is_deceptive_char('a'));
    assert!(!is_deceptive_char('5'));
    assert!(!is_deceptive_char('-'));
    
    // This test will fail if the mutant changes `return false` to `return true`
    // because `'a'` is not deceptive, but mutant would return true
}
```

Re-run:
```bash norun
cargo mutants --file src/utils/wallet_import.rs --max-mutants 20
```

The mutant is now **killed ✓**

---

## CI Integration

When you push a PR with changes to `src/utils/wallet_import.rs`:

1. **GitHub Actions** triggers the mutation-testing workflow
2. Cargo-mutants generates mutants and runs tests
3. **PR comment** shows the kill rate:

```
## 🧬 Mutation Testing Results

| Metric | Value |
|--------|-------|
| **Kill Rate** | 87.3% |
| **Killed** | 42/48 mutants |
| **Survived** | 6/48 mutants |
| **Threshold** | 85.0% |

Module: `src/utils/wallet_import.rs` (wallet backup/import cryptographic code)

✓ **PASSED**: Kill rate meets threshold
```

4. If kill rate < 85%, the PR check fails and you must add tests
5. Once fixed, re-run the workflow or push a new commit

---

## Key Takeaways

| Scenario | Action | Outcome |
|---|---|---|
| Run locally before push | `cargo mutants --file src/utils/wallet_import.rs --max-mutants 20` | Catch low kill rates early |
| Survivor found | Read the mutation, add a test case that would fail if the code were mutated that way | Mutant is killed ✓ |
| CI gate fails | Check PR comment for kill rate, download artifacts, fix survivors | Re-push and gate passes |
| Acceptable survivors | Only for functions on skip_calls list (fmt, prompt helpers, telemetry) | Document in PR why skipped |

---

## Common Questions

**Q: How long does a full run take?**
A: 10-30 minutes for wallet_import.rs on a modern machine. Use `--max-mutants` to speed up iteration.

**Q: Why does CI have a different kill rate than local?**
A: Different test ordering, cached state, machine speed, or timing-dependent test flakiness. Always verify locally.

**Q: Can I see the mutated code?**
A: Yes, cargo-mutants shows diffs. Use `--output mutations.out` and check `mutations.out/caught.txt` and `mutations.out/missed.txt`.

**Q: Do I need to kill every mutant?**
A: No, 85%+ is the threshold. Some mutants are inherently unkillable (unreachable code, no test for edge case). For security code, aim for 90%+.

---

## See Also

- [MUTATION_TESTING.md](MUTATION_TESTING.md) — Full documentation
- [cargo-mutants docs](https://mutants.rs/) — Official reference
- [.cargo-mutants.toml](.cargo-mutants.toml) — Local configuration
- [.github/workflows/mutation-testing.yml](.github/workflows/mutation-testing.yml) — CI workflow
