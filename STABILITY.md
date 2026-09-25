# StarForge Stability Policy

This document defines the stable public surface of StarForge, the semver
versioning policy that governs it, and the rules for introducing breaking
changes.  Integrators and shell-script authors should rely only on commands
and flags marked **Stable** below.

---

## Stability tiers

| Tier | Meaning |
|------|---------|
| **Stable** | Covered by semver. Breaking changes require a major-version bump and at least one deprecation cycle. |
| **Experimental** | May change in any release. Opt in with `--experimental` where required. Not covered by semver. |
| **Internal** | Not intended for external use. No compatibility guarantee. |

---

## Stable surface (as of 0.1.0)

### Commands

| Command | Notes |
|---------|-------|
| `starforge wallet <sub>` | All wallet sub-commands (`create`, `list`, `show`, `fund`, `remove`) |
| `starforge new <sub>` | `contract` and `dapp` scaffolding |
| `starforge deploy` | Deploy a compiled `.wasm` contract |
| `starforge invoke` | Invoke a deployed contract |
| `starforge inspect` | Inspect contract storage |
| `starforge contract <sub>` | Core contract operations |
| `starforge network <sub>` | `use`, `list`, `show` |
| `starforge config <sub>` | `get`, `set`, `reset`, `doctor` |
| `starforge info` | Environment summary |
| `starforge bug-report` | Collect diagnostics for a bug report |
| `starforge completions <shell>` | Shell completion generation |
| `starforge tx` | Fetch and display a transaction |

### Global flags

All stable commands honour these global flags:

- `--json` — machine-readable JSON output
- `--quiet` / `-q` — suppress decorative output
- `--plain` — ASCII-only output (no Unicode symbols or colour)
- `--non-interactive` — fail instead of prompting

### Config keys

The following keys in `~/.config/starforge/config.toml` are stable:

- `network` — active network name (`testnet` | `mainnet`)
- `wallets` — named keypair store (array of `{ name, public_key }`)

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error (config, network, validation) |
| `2` | Usage error (bad flags, missing required argument) |

---

## Experimental surface

The following features are under active development.  Their CLI shape,
config keys, and JSON output may change at any time:

- All `ai` sub-commands (`starforge ai`, `starforge ai-debug`, `starforge ai-profile`, …)
- `starforge nl` (natural language interface)
- `starforge monitor` (contract event monitoring)
- `starforge simulate` / `starforge shell` (local devnet)
- `starforge registry` (template marketplace)
- `starforge diagnostics` (hardware-wallet bridge)
- `starforge multisig` (multi-signature builder)
- `starforge governance` / `starforge upgrade` (on-chain governance)
- `starforge collab` / `starforge social` (community features)
- All plugin (`starforge plugin`) and scheduling (`starforge schedule`) commands

---

## Semver policy

StarForge follows [Semantic Versioning 2.0.0](https://semver.org).

```
MAJOR.MINOR.PATCH

MAJOR — incompatible changes to the stable surface
MINOR — new stable commands or flags; backwards-compatible
PATCH — bug fixes and non-breaking improvements
```

### Pre-1.0 (current)

While the project is at `0.x.y`, **minor** version bumps may contain
breaking changes to the experimental surface without a deprecation cycle.
The stable surface listed above is already held to full semver discipline
so that early adopters can pin safely.

### Breaking-change checklist

A PR that makes a breaking change to the stable surface must:

1. Bump `MAJOR` in `Cargo.toml` (or target the next major milestone).
2. Add a `## [Unreleased]` entry to `CHANGELOG.md` with a `### Breaking`
   section describing the change and migration path.
3. Print a deprecation warning at runtime for at least one minor release
   before the breaking change ships.
4. Update this document.

---

## Deprecation process

1. Mark the command/flag `deprecated` in its `#[arg]` / `#[command]` doc
   comment and print a runtime warning.
2. Wait at least one minor release.
3. Remove in the next major release.

---

## 1.0 scope

The 1.0 milestone (tracked in [#973](https://github.com/Nanle-code/StarForge/issues/973))
will freeze this stable surface and publish this policy.  All items listed
as *Stable* above must have:

- Full `--json` output coverage
- Documented exit codes
- At least one integration test
- An entry in `CHANGELOG.md`
