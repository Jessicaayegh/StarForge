# Fuzzing & Property-Based Testing Guide

StarForge ships with two complementary automated testing strategies for
discovering edge cases and vulnerabilities in the CLI's core logic:

1. **Property-based testing** with [`proptest`](https://proptest-rs.github.io/proptest/)
   — runs hundreds of auto-generated inputs against your code within the normal
   `cargo test` workflow.
2. **Coverage-guided fuzzing** with [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html)
   — mutates byte sequences to find panics, logic errors, and soundness bugs
   in security-sensitive code.

Additional tooling:
- **Mutation testing** via [`cargo-mutants`](https://mutants.rs/) — verifies that
  the test suite is sensitive enough to catch deliberate logic errors.
- **Coverage reporting** via [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) —
  produces LCOV/HTML reports to identify under-tested paths.

---

## Quick Start

### Soroban contract example

Keep contract invariants in a normal integration test so they run locally and
in CI. The same test suite is used by mutation testing and coverage reporting:

```rust
#[test]
fn transfer_preserves_total_supply() {
    let before = balance(&alice()) + balance(&bob());
    transfer(alice(), bob(), 10);
    assert_eq!(balance(&alice()) + balance(&bob()), before);
}
```

For randomized contract inputs, express the invariant with `proptest` and run
it with `cargo test --test property_tests`. Contract-specific tests can live
in the contract crate and use the same `PROPTEST_CASES` setting as this guide.

### Property-based tests (no extra tooling needed)

```bash norun
# Run property tests with default 256 cases per property.
cargo test --test property_tests

# Run contract property tests.
cargo test --test contract_property_tests

# Increase cases for a deeper search.
PROPTEST_CASES=5000 cargo test --test property_tests

# Run all tests (includes property tests).
cargo test
```

### Fuzzing (requires nightly)

```bash norun
# Install cargo-fuzz (one-time).
cargo install cargo-fuzz

# List all fuzz targets.
cargo fuzz list --fuzz-dir fuzz

# Run a specific target for 60 seconds.
cargo fuzz run fuzz_validate_public_key --fuzz-dir fuzz -- -max_total_time=60

# Run a contract fuzz target.
cargo fuzz run fuzz_wasm_validation --fuzz-dir fuzz -- -max_total_time=60

# Run with a size cap (good for initial exploration).
cargo fuzz run fuzz_passphrase_strength --fuzz-dir fuzz \
    -- -max_total_time=120 -max_len=1024
```

### Coverage (requires stable + cargo-llvm-cov)

```bash norun
# Install once.
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov

# Generate HTML report.
./scripts/coverage.sh

# CI mode with threshold check.
COV_THRESHOLD=60 ./scripts/coverage.sh --ci
```

### Mutation testing (requires cargo-mutants)

```bash norun
# Install once.
cargo install cargo-mutants

# Run on focused modules (fastest).
cargo mutants --file src/utils/config.rs --jobs 4

# Full run (slow — consider running overnight).
cargo mutants --jobs 4
```

---

## Property-Based Tests

File: [`tests/property_tests.rs`](tests/property_tests.rs)

| Property group | What it tests |
|---|---|
| `validate_public_key` | Valid G+55 base32 keys always pass; wrong prefix/length/charset always fail |
| `validate_secret_key` | Valid S+55 base32 keys always pass; wrong prefix/length/charset always fail |
| `validate_contract_id` | Valid C+55 base32 IDs always pass; wrong prefix/length/charset always fail |
| `validate_wallet_name` | Valid alphanumeric/dash/underscore names pass; spaces and specials fail |
| `validate_amount` | Positive finite amounts pass; zero/negative/non-numeric fail; result > 0 |
| `check_passphrase_strength` | Short passphrases always fail; long ones never panic; score ∈ [0,4] |
| WASM hash | Output is always 64 lowercase hex chars; same input → same output |
| Template slugs | Valid slugs pass; spaces, empty strings, and overly long slugs fail |
| Structural invariants | Validated keys satisfy structural constraints independently (cross-check) |
| `KdfOptions` | All-None is `is_default()`; any Some value is not |

Additional property suites live in their own files:

| File | Property group | What it tests |
|---|---|---|
| [`tests/config_property_tests.rs`](tests/config_property_tests.rs) | Config round trips | TOML/JSON serialization preserves every value; merging is identity-on-empty, idempotent, and overlay-wins; malformed combinations (unknown network, duplicate wallet, non-HTTP endpoint, unknown overlay key) are rejected |
| [`tests/wallet_import_property_tests.rs`](tests/wallet_import_property_tests.rs) | Wallet import/backup | The same invariants the wallet fuzz targets assert, run on stable in every `cargo test` sweep |
| [`tests/contract_property_tests.rs`](tests/contract_property_tests.rs) | Contract testing infrastructure | WASM validation, WASM hash computation, mock contract invocation, mock storage, mock addresses, and mock environment invariants |

### Writing new properties

Properties follow this pattern:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_my_invariant(input in some_strategy()) {
        let result = my_function(&input);
        prop_assert!(result.is_ok(), "expected Ok for input={:?}", input);
    }
}
```

Key rules:
- Use `prop_assume!(condition)` to skip inputs that don't satisfy preconditions.
- Use `prop_assert!` / `prop_assert_eq!` instead of `assert!` so failures include
  the shrunk counterexample.
- Keep each property focused on **one** invariant.

---

## Fuzz Targets

All harnesses live under `fuzz/fuzz_targets/`. Each file has a `fuzz_target!`
macro that receives raw bytes and should **never panic** regardless of input.

| Target | What it fuzzes |
|---|---|
| `fuzz_validate_public_key` | Public key validation, postcondition checks |
| `fuzz_validate_secret_key` | Secret key + encrypted bundle validation |
| `fuzz_validate_contract_id` | Contract ID validation |
| `fuzz_validate_wallet_name` | Wallet name validation |
| `fuzz_validate_amount` | Amount string parsing; result > 0 postcondition |
| `fuzz_passphrase_strength` | Passphrase strength evaluator; score range check |
| `fuzz_wasm_hash` | SHA-256 WASM hash; determinism and format checks |
| `fuzz_encrypted_bundle_parse` | Encrypted bundle parser via validate_secret_key |
| `fuzz_template_operations` | Structured template inputs via `arbitrary::Arbitrary` |
| `fuzz_wallet_backup_parse` | Wallet backup documents: malformed JSON, truncated files, invalid StrKeys, oversized inputs, Unicode names |
| `fuzz_wallet_import_envelope` | Encrypted backup envelopes: base64 fields, salt/nonce lengths, truncated ciphertext, KDF parameters, plaintext/encrypted classification |
| `fuzz_wallet_backup_structured` | Near-valid backup documents built with `arbitrary::Arbitrary`, to reach the semantic checks the byte-level harness rarely hits |
| `fuzz_wasm_validation` | WASM binary validation: magic header, minimum size, panic-freedom |
| `fuzz_contract_invocation` | Mock contract client invocation: call counting, return/error determinism, reset |
| `fuzz_contract_spec_parse` | Contract test spec JSON parsing: malformed JSON, truncated docs, hostile Unicode |
| `fuzz_test_generator` | Test case generation from source: malformed Rust, empty files, arbitrary fragments |

### Wallet import & backup harnesses

`starforge wallet import --file` and `starforge backup restore` read files that
came from outside the tool, so their parsers are a trust boundary. All three
harnesses drive [`src/utils/wallet_import.rs`](src/utils/wallet_import.rs),
which is deliberately free of prompting, disk access, and config writes.

```bash norun
# Byte-level: malformed JSON, truncated documents, byte soup.
cargo fuzz run fuzz_wallet_backup_parse -- -dict=fuzz/dicts/wallet_backup.dict

# Envelope: base64 fields, truncated ciphertext, bad KDF parameters.
cargo fuzz run fuzz_wallet_import_envelope -- -dict=fuzz/dicts/wallet_backup.dict

# Structured: near-valid documents that reach the semantic checks.
cargo fuzz run fuzz_wallet_backup_structured
```

Seed corpora ship under `fuzz/corpus/fuzz_wallet_backup_parse/` and
`fuzz/corpus/fuzz_wallet_import_envelope/`, covering a valid backup, a
watch-only backup, an empty wallet list, an unsupported version, a truncated
document, a name carrying a right-to-left override, 3/5/6-part envelopes, a
truncated ciphertext, and a non-base64 envelope.

The invariants asserted by the harnesses:

- **Totality** — every input returns a `WalletImportError`; nothing panics.
- **Size first** — the size limit is checked before the JSON parser runs, so an
  oversized file cannot drive a large allocation.
- **Accepted implies valid** — an accepted backup has version `1`, at least one
  wallet, no duplicate names, no control characters in a name, and a 56-character
  `G…` public key on every entry.
- **Envelope shape** — an accepted envelope has a 16-byte salt, a 12-byte nonce,
  a ciphertext of at least 16 bytes (one AES-GCM tag), and non-zero KDF
  parameters.
- **No misclassification** — a JSON document is never treated as an encrypted
  bundle, which would prompt for a passphrase that does not exist.

### Contract fuzzing harnesses

The contract fuzzing harnesses target the Soroban contract testing infrastructure
in StarForge. These harnesses exercise the mock Soroban environment, WASM
validation, contract spec parsing, and test case generation — all of which
process inputs that could come from untrusted contract source files or test
specifications.

```bash norun
# WASM validation: magic header, minimum size, panic-freedom.
cargo fuzz run fuzz_wasm_validation --fuzz-dir fuzz -- -max_total_time=60

# Mock contract invocation: call counting, return/error determinism.
cargo fuzz run fuzz_contract_invocation --fuzz-dir fuzz -- -max_total_time=60

# Contract test spec JSON parsing: malformed JSON, truncated docs.
cargo fuzz run fuzz_contract_spec_parse --fuzz-dir fuzz -- -max_total_time=60

# Test case generation from source: malformed Rust, empty files.
cargo fuzz run fuzz_test_generator --fuzz-dir fuzz -- -max_total_time=60
```

Seed corpora ship under `fuzz/corpus/fuzz_wasm_validation/`,
`fuzz/corpus/fuzz_contract_spec_parse/`, and
`fuzz/corpus/fuzz_test_generator/`, covering valid WASM binaries, valid
contract test specs, and valid Rust source fragments respectively.

The invariants asserted by the contract harnesses:

- **Totality** — every input is handled gracefully; nothing panics.
- **WASM validation** — inputs shorter than 8 bytes or without the `\0asm`
  magic header are rejected; valid headers with sufficient length are accepted.
- **Mock invocation** — call counts always match the number of invocations;
  pre-configured return values and errors are returned deterministically;
  errors take priority over return values; reset clears all state.
- **Spec parsing** — malformed JSON, truncated documents, and hostile Unicode
  produce errors, never panics.
- **Test generation** — malformed Rust source, empty files, and arbitrary
  fragments produce errors or empty results, never panics.

### Running a target

```bash norun
# Basic: run for 2 minutes.
cargo fuzz run fuzz_validate_public_key --fuzz-dir fuzz \
    -- -max_total_time=120

# With dictionary (helps find interesting inputs faster).
cargo fuzz run fuzz_validate_public_key --fuzz-dir fuzz \
    -- -max_total_time=120 -dict=fuzz/dicts/stellar_keys.dict

# Minimize a crash.
cargo fuzz tmin fuzz_validate_public_key --fuzz-dir fuzz \
    fuzz/artifacts/fuzz_validate_public_key/<crash-file>

# Show coverage for a target.
cargo fuzz coverage fuzz_validate_public_key --fuzz-dir fuzz
```

### Reproducing a crash

When cargo-fuzz finds a crash, it saves the input to
`fuzz/artifacts/<target>/<hash>`. To reproduce:

```bash norun
cargo fuzz run fuzz_validate_public_key --fuzz-dir fuzz \
    fuzz/artifacts/fuzz_validate_public_key/<hash>
```

### Writing a new harness

1. Add a new file `fuzz/fuzz_targets/fuzz_my_target.rs`.
2. Add a `[[bin]]` entry to `fuzz/Cargo.toml`.
3. Follow this template:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use starforge::utils::config::my_function;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else { return; };

    // Must never panic.
    let result = my_function(input);

    // Postconditions: check invariants when result is Ok.
    if result.is_ok() {
        assert!(!input.is_empty());
    }
});
```

---

## Mutation Testing

Mutation testing intentionally introduces small code changes ("mutants") and
checks whether the test suite catches them.  A surviving mutant means a bug
that looks like a code change would go undetected.

Config: [`.cargo-mutants.toml`](.cargo-mutants.toml)

```bash norun
# Focus on a single file.
cargo mutants --file src/utils/config.rs

# Run with 4 parallel workers.
cargo mutants --jobs 4 --file src/utils/crypto.rs

# Full output in a directory.
cargo mutants --output mutants.out
```

Results are written to `mutants.out/`:
- `caught.txt` — mutants caught by the test suite (good).
- `missed.txt` — surviving mutants (consider adding a test to cover these).
- `timeout.txt` — mutants that caused test timeouts.
- `unviable.txt` — mutants that didn't compile.

**Surviving mutants** in `validate_public_key`, `encrypt_secret`, or
`compute_local_wasm_hash` are potential security gaps — prioritize writing
tests that kill them.

---

## Coverage Reporting

```bash norun
# HTML report (opens in browser).
./scripts/coverage.sh

# LCOV for CI / Codecov upload.
./scripts/coverage.sh --ci

# JSON for programmatic analysis.
./scripts/coverage.sh --json
```

Reports are written to `target/llvm-cov/`.

---

## CI Integration

The fuzzing CI pipeline is defined in
[`.github/workflows/fuzzing.yml`](.github/workflows/fuzzing.yml) and runs:

| Job | Trigger | What it does |
|---|---|---|
| `property-tests` | Every push / PR | Runs `cargo test --test property_tests` and `cargo test --test contract_property_tests` with 2 000 cases |
| `fuzz-build` | Every push / PR | Compiles all fuzz targets (catches compilation errors) |
| `fuzz-smoke` | Every push / PR | 30-second smoke run per target in a matrix |
| `coverage` | Every push / PR | Generates LCOV + JSON; uploads to Codecov |
| `mutation-testing` | Manual / schedule | Full mutation testing (expensive) |

### Manual dispatch

Trigger the workflow manually from the GitHub Actions UI to customize:
- `fuzz_duration` — seconds per target (default 60)
- `proptest_cases` — cases per property (default 1000)

---

## Adding Tests for New Contract Templates

When you add a new contract template (e.g. a DeFi primitive), add coverage in
both systems:

### Property test

```rust
// In tests/contract_property_tests.rs
proptest! {
    #[test]
    fn prop_my_contract_validates_input(amount in valid_amount_string()) {
        let result = my_contract::validate_deposit(amount.parse().unwrap());
        if amount.parse::<f64>().unwrap() > 0.0 {
            prop_assert!(result.is_ok());
        }
    }
}
```

### Fuzz harness

```rust
// In fuzz/fuzz_targets/fuzz_my_contract.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = my_contract::parse_input(s); // must not panic
    }
});
```

---

## Security Focus Areas

The following functions are the highest-priority fuzz / property-test targets
because they process untrusted external input or handle cryptographic material:

| Function | Module | Risk |
|---|---|---|
| `validate_public_key` | `utils/config.rs` | Wallet address acceptance/rejection |
| `validate_secret_key` | `utils/config.rs` | Encrypted bundle parsing, base64 decode |
| `encrypt_secret` | `utils/crypto.rs` | AES-GCM encryption correctness |
| `decrypt_secret` | `utils/crypto.rs` | AES-GCM decryption, wrong-password handling |
| `check_passphrase_strength` | `utils/crypto.rs` | zxcvbn integration, minimum length gate |
| `compute_local_wasm_hash` | `commands/deploy.rs` | On-chain hash consistency |
| `validate_contract_id` | `utils/config.rs` | Contract address validation |
| `validate_wasm` | `utils/mock_soroban.rs` | WASM binary validation, magic header checks |
| `compute_wasm_hash` | `utils/wasm_hash.rs` | WASM hash computation, environment validation |
| `MockContractClient::invoke` | `utils/contract_mocks.rs` | Mock contract invocation, call logging |
| `load_contract_test_spec` | `utils/contract_testing.rs` | Contract test spec parsing (JSON/TOML) |
| `generate_from_source` | `utils/test_generator.rs` | Test case generation from Rust source |
