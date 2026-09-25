# Multi-Signature Transaction Builder

Interactive CLI tool for building and managing multi-signature transactions with visual progress tracking.

## Features

✅ **Interactive multi-sig builder** - Step-by-step workflow
✅ **Visual progress tracking** - See signature collection status
✅ **Transaction export/import** - Share proposals as JSON
✅ **Signature verification** - Validate signatures
✅ **Pre-built templates** - Common scenarios (escrow, DAO, vault)
✅ **Notification system** - Alert signers via email/Slack/Discord

## Quick Start

### Create Proposal

```bash norun
starforge multisig create \
  --threshold 2 \
  --signers "alice,bob,charlie" \
  --network testnet
```

### Sign Proposal

```bash norun
starforge multisig sign proposal_123.json --wallet alice
```

### Check Status

```bash norun
starforge multisig status proposal_123.json
```

Output:

```
═══ SIGNATURE STATUS ═══
Progress: 1/2
[████████░░] 50%

⏳ Waiting for: bob
```

### Submit When Ready

```bash norun
starforge multisig submit proposal_123.json --network testnet
```

## Commands

### `create` - New Proposal

```bash norun
starforge multisig create \
  --threshold 2 \
  --signers "pubkey1,pubkey2,pubkey3" \
  --network testnet
```

Creates JSON proposal file with:

- Unique proposal ID
- Threshold & signer list
- Empty signatures array
- Metadata fields

### `add-signer` - Add Signer

```bash norun
starforge multisig add-signer proposal.json pubkey4
```

Adds new signer to pending approval list.

### `sign` - Sign Proposal

```bash norun
starforge multisig sign proposal.json --wallet alice
```

Signs with wallet, adds signature to proposal, updates progress.

### `view` - View Details

```bash norun
starforge multisig view proposal.json
```

Shows:

```
═══ PROPOSAL ═══
ID:        abc-123-def
Network:   testnet
Threshold: 2/3
Status:    pending (1/2)
Created:   2024-01-15T10:30:00Z

═══ SIGNERS ═══
✓ 1. alice
✗ 2. bob
✗ 3. charlie

═══ SIGNATURES ═══
✓ alice: abc123def456...
```

### `status` - Check Progress

```bash norun
starforge multisig status proposal.json
```

Shows visual progress bar + pending signers.

### `submit` - Submit to Network

```bash norun
starforge multisig submit proposal.json --network testnet
```

Validates all signatures and submits transaction.

### `export` - Export as JSON

```bash norun
starforge multisig export proposal.json --output proposal_backup.json
```

Exports proposal for sharing or archival.

### `import` - Import from JSON

```bash norun
starforge multisig import proposal_backup.json --output proposal_restored.json
```

Imports exported proposal.

### `templates` - List Templates

```bash norun
starforge multisig templates
```

Shows available pre-built templates:

```
═══ MULTI-SIG TEMPLATES ═══

escrow   - 2-of-3 Escrow (buyer, seller, arbiter)
company  - 3-of-5 Company Signers
dao      - 5-of-9 DAO Treasury
vault    - 2-of-2 Cold Storage Vault
payment  - 1-of-2 Payment Authorization
```

### `from-template` - Create from Template

```bash norun
starforge multisig from-template escrow --output escrow_proposal.json
```

Creates proposal pre-configured with template signers/threshold.

## Workflows

### Escrow Transaction (2-of-3)

```bash norun
# 1. Create from template
starforge multisig from-template escrow -o escrow.json

# 2. Buyer signs
starforge multisig sign escrow.json --wallet buyer

# 3. Check progress
starforge multisig status escrow.json

# 4. Arbiter signs
starforge multisig sign escrow.json --wallet arbiter

# 5. Submit
starforge multisig submit escrow.json
```

### Company Payment (3-of-5)

```bash norun
# Create with company signers
starforge multisig create \
  --threshold 3 \
  --signers "ceo,cfo,board1,board2,board3"

# CEO signs
starforge multisig sign proposal.json --wallet ceo

# CFO signs
starforge multisig sign proposal.json --wallet cfo

# Board member signs
starforge multisig sign proposal.json --wallet board1

# Submit
starforge multisig submit proposal.json
```

### DAO Treasury (5-of-9)

```bash norun
starforge multisig from-template dao -o dao_proposal.json

# Each DAO member signs
for member in member1 member2 member3 member4 member5; do
  starforge multisig sign dao_proposal.json --wallet $member
done

# Check final status
starforge multisig status dao_proposal.json

# Submit
starforge multisig submit dao_proposal.json
```

## Proposal JSON Format

```json
{
  "id": "abc-123-def",
  "threshold": 2,
  "signers": ["alice", "bob", "charlie"],
  "signatures": [
    {
      "signer": "alice",
      "signature": "abc123def456...",
      "signed_at": "2024-01-15T10:30:00Z"
    }
  ],
  "network": "testnet",
  "created_at": "2024-01-15T10:00:00Z",
  "metadata": {
    "title": "Escrow Payment",
    "description": "Payment for service",
    "transaction_type": "payment",
    "amount": 100.0,
    "recipient": "GDEF456..."
  }
}
```

## Notifications

### Send Signature Request

```bash norun
# Email
starforge multisig notify proposal.json --channel email

# Slack
starforge multisig notify proposal.json --channel slack \
  --webhook https://hooks.slack.com/...

# Discord
starforge multisig notify proposal.json --channel discord \
  --webhook https://discord.com/api/webhooks/...
```

## API

### Rust API

```rust
use starforge::utils::multisig_builder::{Proposal, generate_signature};

let mut proposal = Proposal::new(2, vec!["alice".into(), "bob".into()], "testnet".into());

let sig = generate_signature("alice")?;
proposal.add_signature("alice".into(), sig);

let progress = format!("{}/{}", proposal.signatures.len(), proposal.threshold);
```

## Templates Reference

### Escrow (2-of-3)

- Buyer, Seller, Arbiter
- Use: Service/product payment protection
- Release: Any 2 signers approve

### Company (3-of-5)

- CEO, CFO, 3 Board Members
- Use: Corporate treasury access
- Release: Any 3 signers approve

### DAO (5-of-9)

- 9 DAO members
- Use: Treasury proposals
- Release: Minimum 5 members approve

### Vault (2-of-2)

- Cold storage key holder 1, 2
- Use: Maximum security
- Release: Both signers required

### Payment (1-of-2)

- Approver 1, Approver 2
- Use: Flexible payment authorization
- Release: Any 1 approver needed

## Status Display

```
Pending:    ⏳ Waiting for signatures
Partial:    🔄 Some signatures collected
Ready:      ✓ All signatures collected
Submitted:  ✅ On-chain
Failed:     ❌ Submission failed
Expired:    ⏰ Signature window closed
```

## Keyboard Shortcuts (Interactive Mode)

- `s` - Sign with selected wallet
- `v` - View full proposal
- `p` - Check progress
- `n` - Send notifications
- `e` - Export proposal
- `c` - Copy proposal ID
- `q` - Quit

## Performance

| Operation       | Time   |
| --------------- | ------ |
| Create proposal | <10ms  |
| Add signature   | <50ms  |
| Export JSON     | <100ms |
| Status check    | <5ms   |

## Security

- ✅ Ed25519 signature verification
- ✅ Threshold validation
- ✅ Signer authentication
- ✅ Tamper detection
- ✅ Expiration support

## Troubleshooting

**"Not enough signatures"**

- Check status: `starforge multisig status proposal.json`
- Ensure all required signers have signed

**"Invalid signature"**

- Verify signer credentials
- Re-sign proposal

**"File not found"**

- Check proposal file exists
- Use full path if needed

## Examples

### GitHub Actions Automation

```yaml
- name: Sign Multi-Sig Proposal
  run: |
    starforge multisig sign proposal.json --wallet github_signer
    starforge multisig status proposal.json
```

### CI/CD Integration

```bash norun
#!/bin/bash
starforge multisig create \
  --threshold 2 \
  --signers "$SIGNER1,$SIGNER2,$SIGNER3"

# Wait for signatures
while [ ! $(starforge multisig is-ready proposal.json) ]; do
  sleep 10
done

# Submit
starforge multisig submit proposal.json
```

---

## Support

- GitHub: https://github.com/Nanle-code/StarForge
- Issues: https://github.com/Nanle-code/StarForge/issues
