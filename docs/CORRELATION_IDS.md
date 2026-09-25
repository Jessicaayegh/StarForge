# Correlation IDs in Structured Logs

Every `starforge` invocation carries exactly one **correlation ID**. It is
attached to the root command span, so everything that happens underneath —
retries, network requests, plugin calls, deployment steps — logs the same ID.
One invocation can then be reconstructed from an aggregated log stream, even
when several StarForge processes run concurrently in the same pipeline.

---

## Quick start

```bash norun
# Generated automatically; JSON logs carry it on every record.
RUST_LOG=info starforge deploy --wasm ./token.wasm --log-format json

# Supply your own, e.g. a CI job ID, so StarForge logs join your pipeline logs.
starforge deploy --wasm ./token.wasm --correlation-id "$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"

# Or through the environment.
export STARFORGE_CORRELATION_ID="pipeline-2026-07-29-a41f"
starforge deploy --wasm ./token.wasm --log-format json
```

Precedence: `--correlation-id` → `$STARFORGE_CORRELATION_ID` → freshly generated
(a UUIDv4 with hyphens stripped).

---

## What the logs look like

With `--log-format json`, each record carries the current span and the full
span stack, so the correlation ID is present on nested events too:

```json
{
  "timestamp": "2026-07-29T12:01:04.882Z",
  "level": "DEBUG",
  "fields": { "message": "request sent" },
  "span": { "kind": "network_request", "method": "POST",
            "url": "https://soroban-testnet.stellar.org", "correlation_id": "9f2c…" },
  "spans": [
    { "kind": "command", "command": "deploy", "correlation_id": "9f2c…" },
    { "kind": "deploy_step", "step": "upload-wasm", "index": 1, "total": 4, "correlation_id": "9f2c…" },
    { "kind": "network_request", "method": "POST", "url": "https://soroban-testnet.stellar.org", "correlation_id": "9f2c…" }
  ]
}
```

With the default human format the span context is printed inline:

```
INFO command{kind=command command=deploy correlation_id=9f2c…}: starforge: command started
```

### Span kinds

| Kind | Emitted by | Fields |
|---|---|---|
| `command` | Root span, entered once in `main` | `command`, `correlation_id` |
| `retry` | `correlation::retry_span` | `operation`, `attempt`, `max_attempts` |
| `network_request` | `correlation::network_span` | `method`, `url` (host + path only) |
| `plugin_call` | `correlation::plugin_span` | `plugin`, `entrypoint` |
| `deploy_step` | `correlation::deploy_step_span` | `step`, `index`, `total` |

---

## Using it from code

```rust
use starforge::utils::correlation;

// Inside a retried operation:
let span = correlation::retry_span("horizon-submit", attempt, max_attempts);
let _entered = span.enter();
tracing::warn!("attempt failed, backing off");

// Around an outbound request:
let span = correlation::network_span("POST", &rpc_url);
let _entered = span.enter();
```

Do not build these spans by hand. The helpers run every attribute through
`correlation::sanitize` and every URL through `correlation::redact_url`, which
is what keeps secrets out of the log.

---

## Validation rules

A supplied correlation ID must be:

- 8 to 64 characters long,
- made only of `A–Z`, `a–z`, `0–9`, `-`, and `_`.

Leading and trailing whitespace is trimmed first. A value that fails validation
is a **fatal error** (exit code `2`), not a silent fallback — generating a
different ID would break exactly the log join the caller asked for.

```bash norun
$ starforge info --correlation-id "run 12"
Invalid correlation ID: correlation ID contains ' '; only letters, digits, '-' and '_' are allowed
```

---

## Security

A correlation ID appears on every log line and is usually forwarded to a log
aggregator, so it must never carry anything sensitive.

- **Generated IDs are random.** UUIDv4 — derived from nothing about the user,
  the machine, or any key.
- **Supplied IDs are screened.** A value that looks like key material — a
  Stellar `S…` secret seed, an encrypted `salt:nonce:ciphertext` bundle, or a
  long mixed-case base64 blob — is rejected with
  `correlation ID looks like key material`.
- **Span attributes are sanitized.** Values that look like secrets become
  `[REDACTED]`; control characters are replaced with spaces so a crafted value
  cannot forge an extra log record; attributes are truncated at 120 characters.
- **URLs are reduced to scheme, host, and path.** Query strings and
  `user:password@` userinfo are dropped, not sanitized — API keys live in query
  strings and there is no reason for a log to hold them.

```
https://ci:s3cr3t@rpc.example.com/soroban?apiKey=AKIA…  →  https://rpc.example.com/soroban
```

This complements the existing redaction helpers in
[`utils::logging`](../src/utils/logging.rs) — see
[SECURITY_LOGGING_GUIDE.md](../SECURITY_LOGGING_GUIDE.md).

---

## Compatibility

- **Additive.** Existing log consumers see new fields, no removed ones.
- JSON output now sets `with_current_span(true)` and `with_span_list(true)`, so
  records gain `span` and `spans` members. A consumer that reads only
  `fields.message` is unaffected.
- `--correlation-id` is a global flag, accepted before or after the subcommand.
- Re-entrant contexts (the `shell` REPL, plugin hosts) reuse the first installed
  ID rather than minting a new one per inner command, so an interactive session
  stays one correlated unit of work.

---

## See also

- [SECURITY_LOGGING_GUIDE.md](../SECURITY_LOGGING_GUIDE.md) — what may and may not be logged
- [SECURITY_LOGGING_BEST_PRACTICES.md](../SECURITY_LOGGING_BEST_PRACTICES.md)
- [docs/COMMAND_REFERENCE.md](COMMAND_REFERENCE.md) — global flags
