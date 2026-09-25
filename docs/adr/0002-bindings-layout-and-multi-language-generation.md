# ADR 0002: Multi-Language Client Bindings Layout

* **Status**: Accepted
* **Deciders**: StarForge Core Maintainers
* **Date**: 2026-09-05
* **Technical Area**: Bindings & Code Generation

## Context and Problem Statement

Soroban smart contracts compiled to WebAssembly emit custom XDR environment specs. Developers interacting with these contracts require typed SDK client libraries in their host languages (Rust, TypeScript, Python, and Go). Generating disparate layouts across languages causes friction for full-stack Stellar teams.

## Decision Drivers

* Consistent contract invocation syntax across languages (`invoke`, `simulate`, `decode_events`).
* Idiomatic type mapping for Soroban types (Address, Symbol, Bytes, Vec, Map, Custom Enum/Struct).
* Independent compilation and packaging per target ecosystem (npm, crates.io, PyPI, Go modules).

## Considered Options

* **Option 1**: Monolithic multi-language code generator emitting single unified wrapper files.
* **Option 2**: Modular per-target generator outputting dedicated directories (`bindings/rust/`, `bindings/typescript/`, `bindings/python/`, `bindings/go/`) with standard packaging metadata (`package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`).

## Decision Outcome

Chosen option: **Option 2**, because it allows developers in any supported language to immediately import and use the generated bindings using their native package managers while preserving strict Soroban type safety.
