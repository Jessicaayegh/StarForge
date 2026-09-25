# ADR 0003: External Plugin ABI & Sandboxing Boundaries

* **Status**: Accepted
* **Deciders**: StarForge Core Maintainers
* **Date**: 2026-09-10
* **Technical Area**: Plugins & Extensibility

## Context and Problem Statement

Third-party extensions to StarForge (e.g. custom linters, gas estimators, or enterprise multisig backends) must execute safely without compromising host secret keys or destabilizing the core CLI runtime.

## Decision Drivers

* Process isolation between StarForge CLI core and untrusted third-party plugins.
* Clear JSON-RPC / IPC messaging contract for plugin lifecycle (`init`, `execute`, `cleanup`).
* Strict permission declarations in `plugin.json` (filesystem access, network access, environment access).

## Decision Outcome

Chosen option: External sub-process plugins communicating via stdin/stdout JSON-RPC with explicit permission verification and cryptographic signature validation (`starforge plugin verify`).

### Positive Consequences

* Plugin crashes do not crash the host CLI process.
* Secrets and encrypted key vaults remain inaccessible to plugins without explicit permission grants.
