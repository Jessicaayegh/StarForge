# ADR 0004: Privacy-Preserving Telemetry & Strict Opt-In

* **Status**: Accepted
* **Deciders**: StarForge Core Maintainers
* **Date**: 2026-09-15
* **Technical Area**: Telemetry & Privacy

## Context and Problem Statement

CLI tools in the Web3/Stellar ecosystem frequently touch sensitive information (private keys, seed phrases, unpublished contract WASM bytecode, and proprietary RPC endpoints). Any telemetry system must guarantee zero leakage of sensitive data.

## Decision Drivers

* Comply with strict developer privacy expectations (opt-in by default, prompt-based consent).
* Zero storage or transmission of keys, seeds, account hashes, or contract payloads.
* Complete local transparency (`starforge privacy audit`, `starforge telemetry status`).

## Decision Outcome

Chosen option: Telemetry is strictly disabled by default until explicit user consent is given. All payload payloads undergo local redaction and anonymization before transmission. Any presence of Ed25519 secret seeds (`S...`) immediately drops the event.
