# ADR 0005: Deterministic Soroban Simulation Profiles for CI

* **Status**: Accepted
* **Deciders**: StarForge Core Maintainers
* **Date**: 2026-09-20
* **Technical Area**: Simulation & CI Automation

## Context and Problem Statement

Soroban transaction fee and resource calculations (CPU instructions, memory allocations, read/write footprints) are subject to dynamic network state. In CI pipelines and local developer testing, unpinned simulation settings lead to flaky test suites and unpredictable gas latency budgets.

## Decision Drivers

* Reproducible resource assertions across developer laptops and CI runners.
* Named profiles with defined resource limits:
  - `ci-smoke`: Fast, tight sanity checks.
  - `ci-full`: Comprehensive production-grade limits.
  - `dev-fast`: Relaxed developer iteration ceilings.
* Clear machine-readable and human-readable output indicating ceiling assertions and breaches.

## Decision Outcome

Chosen option: Implement version-controlled deterministic simulation profiles via `starforge-simulation-profiles.toml` and CLI `--profile` flag in `starforge simulate resources`. The command asserts execution metrics against profile ceilings and exits non-zero if ceilings are breached.
