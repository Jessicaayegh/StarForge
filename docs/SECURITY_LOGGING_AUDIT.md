# Security Logging Audit

This audit ensures that secret material is never emitted at info level in the command handlers and logging helpers.

## Sensitive data categories

- Secret keys
- Passphrases
- Signed XDR envelopes or transactions

## Grep-based checks

Use these commands from the repository root to validate the audit:

```bash norun
grep -R --line-number -E 'p::info\(|tracing::info!|info!\(' src/commands/wallet.rs src/commands/deploy.rs
grep -R --line-number -E 'Secret Key|passphrase|transaction_xdr|signed_xdr|XDR' src/commands/wallet.rs src/commands/deploy.rs
```

## Implementation notes

- `src/utils/logging.rs` now exposes helpers for redacting sensitive log data:
  - `redact_public_key(public_key, level)` hides public keys in info-level logs but preserves them in debug and trace logs.
  - `redact_secret_value(_)` always returns `"[REDACTED]"` for secret keys and passphrases.
  - `redact_signed_xdr(_)` always returns `"[REDACTED]"` for signed XDR payloads.

- `tests/security_logging_audit.rs` contains a grep-based regression test that scans `src/commands/wallet.rs` and `src/commands/deploy.rs` for suspicious info-level patterns.

## Running the audit

```bash norun
cargo test --test security_logging_audit
```
