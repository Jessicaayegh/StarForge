# AI Quality Gates & PR Bot Automation

StarForge provides automated, CI-friendly AI quality gates that evaluate smart contract codebases against security, reliability, documentation, licensing, gas efficiency, and test coverage standards before pull requests are merged.

---

## Quality Gate Presets

StarForge includes three curated preset profiles designed to match project maturity:

| Metric / Threshold | Conservative | Default (Recommended) | Strict (Mainnet / DeFi) |
| :--- | :---: | :---: | :---: |
| **Minimum Quality Score** | `50` | `70` | `90` |
| **Maximum Unwraps / Expects** | `10` | `0` | `0` |
| **Maximum TODO / FIXME Markers** | `10` | `0` | `0` |
| **Critical / High Security Findings** | `0` | `0` | `0` |
| **Medium Security Findings** | `10` | `5` | `0` |
| **Unbounded Loops** | `2` | `0` | `0` |
| **Storage Operations in Loops** | `2` | `0` | `0` |
| **Minimum Test Coverage** | `50.0%` | `80.0%` | `90.0%` |
| **Public API Documentation Completeness** | `50.0%` | `80.0%` | `95.0%` |
| **Maximum Benchmark Duration** | `null` | `null` | `100.0 ms` |
| **Allowed SPDX Licenses** | MIT, Apache-2.0, BSD-3, ISC, Unlicense | MIT, Apache-2.0, BSD-3 | MIT, Apache-2.0, BSD-3 |

### 1. Conservative (`--preset conservative`)
Ideal for:
- Migrating legacy codebases or prototyping experimental contracts.
- Warning on unwraps and TODOs without blocking early-stage PRs.
- Permissive licensing and relaxed coverage thresholds.

### 2. Default (`--preset default`)
Ideal for:
- General Soroban smart contract development.
- Zero tolerance for unchecked unwraps, TODO markers, or high-severity vulnerabilities.
- Balanced 80% coverage and documentation completeness.

### 3. Strict (`--preset strict`)
Ideal for:
- Financial-grade protocols, liquidity pools, and production mainnet deployments.
- Zero tolerance for medium or high vulnerabilities.
- 90% coverage and 95% API documentation requirements.

---

## CLI Commands

### 1. Initialize Configuration
Generate a starter configuration file (`starforge-gates.toml`):
```bash
# Initialize with default preset
starforge ai quality-gate init

# Initialize with strict preset
starforge ai quality-gate init --preset strict starforge-gates.toml
```

### 2. Evaluate Quality Gates
Run checks against a contract directory:
```bash
# Evaluate against local starforge-gates.toml
starforge ai quality-gate check

# Evaluate with inline preset override
starforge ai quality-gate check --preset strict --coverage 92.5

# Emit GitHub Actions workflow annotations
starforge ai quality-gate check --github-annotations --output quality-report.json
```

---

## PR Bot & GitHub Actions Integration

Use StarForge quality gates in PR workflows to automatically evaluate code quality and post workflow annotations without risk of secret leakage.

### Example Workflow (`.github/workflows/quality-gate.yml`)

```yaml
name: AI Quality Gate

on:
  pull_request:
    branches: [main, master]

jobs:
  quality-gate:
    name: Evaluate AI Quality Gates
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install StarForge
        run: cargo install --path . --locked

      - name: Run Test Coverage
        id: coverage
        run: |
          # Compute or mock line coverage
          echo "coverage=88.5" >> $GITHUB_OUTPUT

      - name: Run AI Quality Gate
        run: |
          starforge ai quality-gate check \
            --preset default \
            --coverage ${{ steps.coverage.outputs.coverage }} \
            --github-annotations \
            --output target/quality-gate-report.json

      - name: Upload Quality Gate Report
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: quality-gate-report
          path: target/quality-gate-report.json
```

---

## Secret Redaction Guarantee

To prevent supply chain and secret leakage risks, all quality gate evaluations, reports, stdout/stderr messages, and GitHub Actions annotations (`::error` / `::warning`) pass through StarForge's centralized secret redaction engine (`crate::utils::redaction::redact_secrets`).

The engine automatically sanitizes:
- Stellar secret keys (`S...`)
- BIP-39 mnemonic seed phrases (12/24 words)
- Hex-encoded private keys
- Bearer tokens, GitHub tokens (`ghp_`), and OpenAI/Anthropic API keys (`sk-...`)
- Basic-Auth URL credentials and signed XDR transaction envelopes

---

## Limitations & Human-Review Expectations

- **Static Analysis Boundaries**: AI quality gates perform static syntactic and heuristic checks. They do not replace formal verification, cryptographic review, or professional smart contract audits.
- **Dynamic Assertions**: Test coverage metrics should be measured with tools like `cargo-llvm-cov` or `cargo-tarpaulin` and fed via `--coverage`.
- **Human-in-the-Loop**: Quality gate pass status indicates baseline compliance with structural and security invariants; human code reviewers must still review domain business logic, access control lists, and economic mechanics.
