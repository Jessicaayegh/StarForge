# ADR 0006: Versioned Configuration Schema Migrations

* **Status**: Accepted
* **Deciders**: StarForge Core Maintainers
* **Date**: 2026-09-22
* **Technical Area**: Configuration & Storage

## Context and Problem Statement

As StarForge evolves, new configuration keys (telemetry preferences, audit locations, simulation defaults, network RPC endpoints) are added to `~/.starforge/config.json`. Directly modifying schema formats without automated migration paths breaks existing user installations or silently resets configurations.

## Decision Drivers

* Seamless backward compatibility with older configuration file layouts (v1 -> v2 -> v3).
* Automatic non-destructive schema migration upon CLI startup.
* Safe backup creation prior to rewriting configuration files.

## Decision Outcome

Chosen option: Store an integer `schema_version` in the root configuration file. A sequential migration engine checks the version on startup, backs up the previous configuration, and applies sequential up-migrations incrementally.
