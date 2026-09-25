# Contract Storage Layout Introspection and Migration Guide

StarForge provides automated storage layout introspection and hazard analysis to ensure safe upgrades and state migrations between Soroban contract versions.

---

## 1. Storage Layout Introspection

Introspect the storage layout of any Soroban smart contract source (`.rs`) or serialized layout definition (`.json`):

```bash
starforge migrate introspect contracts/token/src/lib.rs
```

### JSON Output for Automation

```bash
starforge migrate introspect contracts/token/src/lib.rs --json
```

Output format:
```json
{
  "contract_name": "TokenContract",
  "version": "1.0.0",
  "keys": [
    {
      "name": "Admin",
      "storage_tier": "instance",
      "key_type": "DataKey::Admin",
      "value_type": "Address",
      "discriminant": 0,
      "is_optional": false
    },
    {
      "name": "TotalSupply",
      "storage_tier": "instance",
      "key_type": "DataKey::TotalSupply",
      "value_type": "i128",
      "discriminant": 1,
      "is_optional": false
    }
  ],
  "variants": [
    {
      "name": "Admin",
      "fields": [],
      "discriminant": 0
    },
    {
      "name": "TotalSupply",
      "fields": [],
      "discriminant": 1
    }
  ]
}
```

---

## 2. Migration Hazard Analysis

When upgrading a contract from V1 to V2, run hazard analysis to identify breaking incompatibilities before deploying to testnet or mainnet:

```bash
starforge migrate hazards \
  --old contracts/v1/src/lib.rs \
  --new contracts/v2/src/lib.rs
```

### Hazard Classifications

| Severity | Description | Action Required |
|---|---|---|
| **BREAKING** | Removed storage keys, incompatible value types, storage tier shifts (instance ↔ persistent), or enum discriminant collisions. | Migration script required; direct upgrade will fail or corrupt state. |
| **HIGH RISK** | Type narrowing (e.g., `i128` to `u32`) or large schema restructurings. | Manual verification of value ranges. |
| **WARNING** | Uninitialized newly added mandatory storage keys. | Ensure `init` or migration sets default values. |
| **INFO** | Added optional keys or documentation updates. | No action required. |

### Generating Starter Migration Rules

Pass `--output-rules <path>` to automatically produce a starter JSON migration rules file:

```bash
starforge migrate hazards \
  --old contracts/v1/src/lib.rs \
  --new contracts/v2/src/lib.rs \
  --output-rules migration_rules.json
```

---

## 3. End-to-End Migration Workflow

1. **Introspect & Detect Hazards**:
   ```bash
   starforge migrate hazards --old v1.rs --new v2.rs --output-rules rules.json
   ```
2. **Capture Storage Snapshot**:
   ```bash
   starforge migrate snapshot --contract-id <CONTRACT_ID> --output snapshot-v1.json
   ```
3. **Dry-Run Rules Against Snapshot**:
   ```bash
   starforge migrate test --sample snapshot-v1.json --rules rules.json
   ```
4. **Execute Migration**:
   ```bash
   starforge migrate run --contract-id <CONTRACT_ID> --snapshot snapshot-v1.json --rules rules.json --output snapshot-v2.json
   ```
5. **Validate Migrated State**:
   ```bash
   starforge migrate validate --snapshot snapshot-v2.json --rules rules.json
   ```
