# Time-Locked Multi-Signature Policies in StarForge

StarForge supports time-locked multi-signature policies for governance, treasury management, and sensitive contract upgrades. Time-locks enforce a mandatory delay between when a proposal reaches its signature threshold and when it can be executed on-chain, providing a critical grace window for community inspection or automated circuit breakers.

---

## 1. Time-Lock Policy Parameters

| Parameter | CLI Flag | Description |
|---|---|---|
| **Min Delay** | `--timelock-delay <seconds>` | Mandatory waiting period in seconds after threshold is met before submission is allowed. |
| **Execution Window** | `--execution-window <seconds>` | Optional validity window in seconds after unlock before the proposal expires. |

---

## 2. Proposal Lifecycle & Status

A timelocked proposal moves through four sequential states:

```
[ Collecting Signatures ]
          │ (Threshold reached)
          ▼
      [ Locked ] ──────── (Delay elapsed) ──────► [ Ready to Execute ]
                                                          │ (Window passed)
                                                          ▼
                                                     [ Expired ]
```

1. **Collecting Signatures**: Signatures are being collected; threshold not yet met.
2. **Locked**: Threshold reached, but locked under mandatory timelock delay. `can_execute()` returns error with remaining delay seconds and unlock timestamp.
3. **Ready to Execute**: Timelock delay has satisfied; proposal is eligible for network submission.
4. **Expired**: The execution window has elapsed without submission; proposal must be re-proposed.

---

## 3. CLI Usage

### Creating a Time-Locked Proposal

Create a 2-of-3 multisig proposal with a 24-hour (86,400s) delay and a 7-day (604,800s) execution window:

```bash
starforge multisig create \
  --threshold 2 \
  --signers GAAA...,GBBB...,GCCC... \
  --timelock-delay 86400 \
  --execution-window 604800 \
  --network testnet
```

### Inspecting Timelock Status

Check progress and remaining delay:

```bash
starforge multisig status proposal.json
starforge multisig view proposal.json
```

### Automated Readiness Check (`is-ready`)

CI and automated relayers can query proposal execution eligibility:

```bash
starforge multisig is-ready proposal.json
```
- Exits `0` and outputs `ready` only if threshold is satisfied **and** timelock delay has elapsed.
- Exits `1` if still locked, incomplete, or expired.

### Submitting the Proposal

```bash
starforge multisig submit proposal.json --network testnet
```
If attempted while locked, submission aborts with:
`Error: Proposal is timelocked until 2026-09-26T08:00:00Z. Remaining delay: 84200s`
