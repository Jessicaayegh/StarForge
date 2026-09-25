# Incident Response Runbook: Compromised Key Material

This runbook provides emergency procedures and containment checklists for operators, developers, and administrators when secret keys (`S...`), recovery phrases, or signer credentials managed by or used with StarForge are suspected of being leaked or compromised.

---

## 🚨 Emergency Response Checklist

```
[ ] 1. IDENTIFY      — Determine leaked public address and scope of exposure
[ ] 2. CONTAIN       — Revoke permissions / increase multisig threshold immediately
[ ] 3. ROTATE        — Provision new keypair and replace compromised signer
[ ] 4. SWEEP         — Migrate remaining funds / contract admin ownership
[ ] 5. AUDIT         — Inspect transaction logs and audit trail for unauthorized calls
[ ] 6. POST-MORTEM   — Document root cause and deploy preventive security controls
```

---

## Phase 1: Detection & Confirmation

### Common Compromise Signals
- Secret key or mnemonic phrase accidentally committed to a public git repository.
- Secret key surfaced in unredacted CI build logs or error stack traces.
- Unexpected contract invocations, unauthorized admin calls, or unknown balance drains on Stellar Expert / Horizon.

### StarForge Investigation Commands

```bash
# 1. Inspect recent wallet transactions and signatures
starforge tx list --account <PUBLIC_KEY> --limit 50

# 2. Inspect local audit trail for anomalous commands or exports
starforge audit logs --limit 100

# 3. Check wallet configuration and registered signers
starforge wallet show <WALLET_NAME>
```

---

## Phase 2: Immediate Containment

1. **Do NOT delete the key locally yet**: You may need it to sign the signer rotation or migration transaction before the attacker drains the account.
2. **If account is protected by multi-signature (recommended)**:
   - Immediately increase the threshold or remove the compromised signer using the remaining threshold keys.
   ```bash
   starforge multisig remove-signer --account <ACCOUNT_ID> --signer <COMPROMISED_PUBLIC_KEY>
   ```

3. **If single-signer account**:
   - Proceed immediately to Phase 3 to rotate signers or transfer balance.

---

## Phase 3: Signer Rotation & Fund Migration

### 1. Generate a Fresh Secure Keypair

Generate a new keypair in an isolated, secure environment:

```bash
starforge wallet create recovery-vault --network mainnet
```

### 2. Add New Signer to Account

Add the new keypair with high weight:

```bash
starforge multisig add-signer \
  --account <COMPROMISED_ACCOUNT> \
  --signer <NEW_PUBLIC_KEY> \
  --weight 20
```

### 3. Remove Compromised Signer

Set the compromised key's weight to `0`:

```bash
starforge multisig set-weight \
  --account <COMPROMISED_ACCOUNT> \
  --signer <COMPROMISED_PUBLIC_KEY> \
  --weight 0
```

### 4. Transfer Remaining Native & Token Balances

```bash
starforge tx payment \
  --source <COMPROMISED_ACCOUNT> \
  --destination <NEW_RECOVERY_ADDRESS> \
  --amount MAX \
  --asset native
```

---

## Phase 4: Post-Incident Review & Prevention

1. **Revoke Exposed Tokens**: If git credentials or third-party RPC tokens were also exposed in the same environment, revoke and regenerate them immediately.
2. **Review Local Vault Encryption**:
   ```bash
   starforge wallet verify-encryption
   ```
3. **Audit History & Compliance**:
   ```bash
   starforge compliance audit --output incident-report.json
   ```
4. **Submit Incident Summary**: Document root cause, timeline, affected assets, and remediation steps for compliance recordkeeping.
