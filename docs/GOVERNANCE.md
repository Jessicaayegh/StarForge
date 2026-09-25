# Contract Upgrade Governance

StarForge provides an off-chain governance layer for Soroban contract upgrades. Proposals, voting, timelock delays, and audit trails are persisted locally under `~/.starforge/governance/`.

## Storage layout

```
~/.starforge/governance/
├── proposals.json   # Active and historical governance proposals
├── audit.json       # Governance-specific audit trail
└── config.json      # Default timelock, thresholds, emergency guardians
```

## Standard upgrade workflow

### 1. Configure governance defaults (optional)

```bash norun
starforge governance config set --timelock 86400 --threshold 2
starforge governance config set --guardian GALICE... --emergency-quorum 2
```

### 2. Create a proposal

```bash norun
starforge governance propose \
  --contract-id C... \
  --wasm target/wasm32v1-none/release/my_contract.wasm \
  --description "Fix transfer validation bug" \
  --threshold 2 \
  --timelock 86400 \
  --network testnet
```

### 3. Collect votes

```bash norun
starforge governance vote --proposal-id gov-abc123 --for --wallet alice
starforge governance vote --proposal-id gov-abc123 --for --wallet bob
```

When the approval threshold is met, the proposal enters a **timelock** period. Execution is blocked until the timelock expires.

### 4. Monitor status

```bash norun
starforge governance list --network testnet
starforge governance show --proposal-id gov-abc123
starforge governance dashboard
```

### 5. Execute after timelock

```bash norun
starforge governance execute --proposal-id gov-abc123 --wallet alice
```

StarForge validates the timelock and threshold, records the execution in the audit trail, and prints the on-chain `stellar contract` commands.

## Emergency upgrades

For critical security patches, authorized guardians can bypass the timelock:

```bash norun
# Register guardians first
starforge governance config set --guardian GALICE...
starforge governance config set --guardian GBOB... --emergency-quorum 2

# Initiate emergency upgrade
starforge governance emergency \
  --contract-id C... \
  --wasm target/wasm32v1-none/release/patch.wasm \
  --description "Critical reentrancy fix" \
  --wallet alice \
  --yes
```

Emergency proposals are flagged in the audit trail and skip the timelock when the emergency quorum is met.

## Audit trail

Every governance action (propose, vote, reject, execute, emergency) is recorded in `audit.json` and mirrored to the global StarForge audit log.

```bash run
# Full governance audit log
starforge governance audit

# Per-proposal audit
starforge governance audit --proposal-id gov-abc123

# JSON export
starforge governance audit --json
```

## Proposal lifecycle

```
Created (active)
    │
    ├── votes reach threshold ──► passed (timelock running)
    │                                  │
    │                                  └── timelock elapsed ──► timelock-ready
    │                                                              │
    │                                                              └── execute ──► executed
    │
    └── reject ──► rejected

Emergency path: emergency ──► emergency-executed (timelock bypassed)
```

## Relationship to `starforge upgrade`

| Feature | `upgrade` | `governance` |
|---------|-----------|--------------|
| Multi-approver workflow | Approvals | Votes (for/against) |
| Timelock | No | Yes (configurable) |
| Audit trail | History only | Full action audit |
| Emergency bypass | No | Yes (guardian quorum) |
| Dashboard | List/status | Dashboard + audit summary |

Use `starforge upgrade` for simple single-signer flows. Use `starforge governance` for production deployments requiring timelock, voting transparency, and audit compliance.

## Best practices

1. Set timelock to at least 24 hours on mainnet.
2. Register emergency guardians before deployment.
3. Require independent WASM hash verification by all voters.
4. Export audit logs regularly: `starforge governance audit --json > audit-backup.json`.
5. Test the full workflow on testnet before mainnet.
