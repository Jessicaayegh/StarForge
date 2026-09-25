# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in StarForge, please open a
[GitHub issue](https://github.com/Nanle-code/StarForge/issues) or contact
the maintainers directly rather than disclosing it publicly, so a fix can
be prepared before details are shared widely.

## Secrets & PII handling

For a full inventory of where secrets (private keys, API tokens, auth
credentials) and PII/metadata (local paths, usernames, IP addresses, AI
prompt text) can flow within the CLI — config files, logs, telemetry
payloads, AI prompt requests, and stdout/stderr — along with the controls
applied at each point and known gaps, see
[docs/DATA_FLOW_INVENTORY.md](docs/DATA_FLOW_INVENTORY.md).

## Related documentation

- [docs/WALLET_IMPORT_SECURITY.md](docs/WALLET_IMPORT_SECURITY.md) — parser hardening for untrusted wallet backups
- [docs/RECOVERY_SHARES_SECURITY.md](docs/RECOVERY_SHARES_SECURITY.md) — threat model for Shamir recovery shares
- [SECURITY_LOGGING_GUIDE.md](SECURITY_LOGGING_GUIDE.md) — what may and may not be logged
- [TELEMETRY_PRIVACY.md](TELEMETRY_PRIVACY.md) — what telemetry collects and how to disable it

## Dependency Updates Policy

We use Dependabot to automate dependency updates. To reduce maintenance overhead without compromising security, we enforce a tiered auto-merge policy:

- **Patch Updates**: Minor low-risk updates (`patch`) are eligible for auto-merging. They will only merge if all CI checks pass and there are no merge conflicts.
- **Major/Minor Updates**: Upgrades across `major` or `minor` semantic versions require explicit human review.
- **Crypto & Security Crates**: Any updates (including patches) to cryptography or security-sensitive crates (such as `ed25519-dalek`, `aes-gcm`, etc.) are excluded from auto-merging and require human review.
- **Ownership**: Maintainers are responsible for reviewing and merging major and security updates. Dependabot PRs should not be merged recklessly.
