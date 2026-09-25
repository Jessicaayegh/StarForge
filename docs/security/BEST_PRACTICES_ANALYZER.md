# Security Best Practices Analyzer

`starforge security best-practices` reviews Soroban contracts against a library
of security best practices that are mapped to industry references (OWASP Smart
Contract Top 10 2025 and CWE). It produces a security score, prioritized
recommendations, and reports, and it tracks remediation across runs.

```bash norun
starforge security best-practices analyze contracts/token          # directory or file
starforge security best-practices analyze . --format markdown --output security.md
starforge security best-practices analyze . --fail-on high --min-score 80 --track
starforge security best-practices rules
starforge security best-practices status
```

## Rule engine

Every rule runs against a parsed view of each source file. The parsed view
knows about functions, whether each function sits inside a `#[contractimpl]`
block, whether it is documented, and which lines belong to `#[cfg(test)]`
modules (those lines are ignored). Directories are scanned recursively for
`.rs` files and `Cargo.toml`, skipping `target/` and hidden directories.

| Rule | Severity | Checks | References |
|---|---|---|---|
| `SF-AUTH-001` | critical | Public entry point writes storage without `require_auth` | OWASP-SC01, CWE-862 |
| `SF-AUTH-002` | high | Initializer can run twice (no `has()` guard) | OWASP-SC01, CWE-665 |
| `SF-AUTH-003` | critical | `update_current_contract_wasm` without `require_auth` | OWASP-SC01, CWE-284 |
| `SF-AUTH-004` | critical | `mock_all_auths` / `mock_auths` outside tests | CWE-489 |
| `SF-ARITH-001` | high | Unchecked `+ - *` on balances, amounts, supplies, and fees | OWASP-SC08, CWE-190/191 |
| `SF-ARITH-002` | medium | `[profile.release]` without `overflow-checks = true` | OWASP-SC08, CWE-190 |
| `SF-ERR-001` | medium | `unwrap()` / `expect()` in contract code | OWASP-SC05, CWE-248 |
| `SF-ERR-002` | low | String panics instead of `#[contracterror]` | OWASP-SC05, CWE-755 |
| `SF-STORE-001` | medium | Persistent or instance storage written without `extend_ttl` | CWE-400 |
| `SF-STORE-002` | medium | Growing `Vec`/`Map` stored in instance storage | OWASP-SC10, CWE-770 |
| `SF-EVT-001` | low | State-changing entry point emits no event | CWE-778 |
| `SF-EXT-001` | high | State written after an external contract call | OWASP-SC05, CWE-841 |
| `SF-DOS-001` | medium | Unbounded loop in a storage-backed entry point | OWASP-SC10, CWE-834 |
| `SF-RAND-001` | high | Ledger timestamp or sequence used as randomness | OWASP-SC09, CWE-338 |
| `SF-BASE-001` | low | Contract crate without `#![no_std]` | CWE-1104 |
| `SF-DOC-001` | info | Public entry point without a doc comment | CWE-1059 |

Run `best-practices rules --format json` for the full rationale and
recommendation text of each rule.

`--min-severity <level>` evaluates only rules at or above that severity, and
`--disable <RULE_ID>` (repeatable) skips a rule entirely.

The checks are heuristic, source-level analysis. They find likely problems
quickly but do not replace a manual audit or `starforge security audit`.

### Inline suppressions

To suppress a finding that has been reviewed, put a justification on the
flagged line or the line above it:

```rust
// starforge-allow(SF-ERR-001): key is always written by initialize
let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
```

The suppression applies only to the named rule. Suppressed findings and their
justifications are listed in every report.

## Scoring

The score starts at 100. Each rule that fires deducts its severity weight
(critical 25, high 15, medium 8, low 3, info 1). Each repeat of the same rule
adds a quarter of the weight, capped at twice the weight, so a single noisy
rule cannot zero the score on its own. Any critical finding caps the score at
69, which means a grade of D or worse. Grades: A ≥ 90, B ≥ 80, C ≥ 70,
D ≥ 60, F otherwise.

Reports also include a 0–100 score per category (access-control, arithmetic,
storage, error-handling, and so on), so you can see where the risk is
concentrated.

## Recommendations

Findings are grouped by rule into recommendations. Each one lists the action to
take, why it matters, the OWASP/CWE references, and every location it applies
to. Recommendations are numbered by score impact, so fixing them in order
raises the score fastest.

## Reports

`--format` takes one of:

- `text` (default): a terminal summary.
- `markdown`: a shareable report for PRs or wikis.
- `json`: the full `AnalysisReport` for tooling.
- `sarif`: SARIF 2.1.0 for GitHub code scanning. Results carry a stable
  `partialFingerprints` entry so alerts are deduplicated across runs.

```yaml
- run: starforge security best-practices analyze contracts --format sarif --output bp.sarif
- uses: github/codeql-action/upload-sarif@v3
  with: { sarif_file: bp.sarif }
```

## Remediation tracking

With `--track`, findings are recorded in
`.starforge/security/best-practices.json` (override with `--state`). Commit it
to share triage decisions. Each finding has a fingerprint derived from the
rule, file, function, and normalized evidence, and deliberately not the line
number, so it survives unrelated edits.

On each tracked run:

- A new finding is recorded as `open`.
- A finding that no longer appears in a file that was analyzed is marked
  `resolved`. Files outside the scanned path are left alone.
- A resolved finding that comes back is reopened.
- The score is appended to a history, and `status` shows the trend.

```bash norun
starforge security best-practices status            # open + accepted findings
starforge security best-practices status --all      # include resolved
starforge security best-practices accept 3f9a1c2e --reason "admin-only, tracked in #412"
starforge security best-practices reopen 3f9a1c2e
```

Accepted findings count as known risk and are excluded from `--fail-on` gating.

## CI gating

`analyze` exits non-zero in two cases:

- `--fail-on <severity>`: an open, non-accepted finding at or above that
  severity remains.
- `--min-score <N>`: the security score is below `N`.
