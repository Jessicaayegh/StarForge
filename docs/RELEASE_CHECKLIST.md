# Release Checklist

Before tagging and publishing a new release, maintainers must verify the following items:

- [ ] **Feature Matrix Passing**: The `feature-matrix` CI job has successfully passed for all required feature combinations:
  - Default features
  - `--no-default-features`
  - `--features ai`
  - `--features hardware-wallet`
- [ ] **Secure Defaults Audit**: The automated secure defaults audit (`cargo test --test secure_defaults_audit`) has passed, and manual attestation has been provided on the release PR (see [SECURE_DEFAULTS_AUDIT.md](SECURE_DEFAULTS_AUDIT.md)).
- [ ] **Changelog Updated**: All significant changes since the last release are documented in `CHANGELOG.md`.
