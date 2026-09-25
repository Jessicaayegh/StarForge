# Simulation Resource Fees

Soroban charges for what a transaction *actually does*: CPU instructions burned,
linear memory touched, and the ledger entries read and written. The network's
`simulateTransaction` RPC is the only place those numbers come from — it runs
the invocation against a real ledger snapshot and reports a **minimum resource
fee** the transaction must carry to be accepted.

StarForge surfaces those numbers in three places:

| Command | What it does |
|---|---|
| `starforge simulate resources` | Report CPU, memory, footprint, and minimum resource fee, and derive a submittable fee |
| `starforge cost resources` | The same report, then check the resulting fee against configured budgets |
| `starforge deploy --simulate` / `--dry-run` | Print the resource report inline before the deploy is confirmed |

---

## `starforge simulate resources`

Two input modes. Exactly one is required.

**Offline** — read a `simulateTransaction` response captured earlier (for
example by a CI job, or with `curl`):

```bash norun
starforge simulate resources --file simulation.json
```

**Live** — simulate against a Soroban RPC endpoint:

```bash norun
starforge simulate resources \
  --contract CCPYZ... \
  --function transfer \
  --arg GABC... --arg 1000 \
  --network testnet
```

| Flag | Default | Purpose |
|---|---|---|
| `--file <PATH>` | — | Saved `simulateTransaction` JSON response (conflicts with `--contract`) |
| `--contract <ID>` | — | Contract to simulate live |
| `--function <NAME>` | — | Function to simulate (required with `--contract`) |
| `--arg <VALUE>` | — | Function argument; repeat for multiple |
| `--arg-type <TYPE>` | inferred | Type for the matching `--arg`; must be supplied for all or none |
| `--network <NAME>` | `testnet` | Network for live simulation |
| `--profile <NAME>` | — | Deterministic simulation profile (`ci-smoke`, `ci-full`, `dev-fast`) to assert resource ceilings |
| `--margin <PERCENT>` | `20` | Safety margin over the minimum resource fee (`0`–`1000`) |
| `--inclusion-fee <STROOPS>` | `100` | Per-operation inclusion (base) fee |
| `--json` | off | Emit the report as machine-readable JSON |

### Deterministic Simulation Profiles

Profiles define reproducible resource ceilings and default margins shared between local development and CI pipelines:

- **`ci-smoke`**: Strict resource limits (2M CPU instructions, 2MB memory, 10 footprint entries, 100k stroops fee). Designed for pull request smoke tests and fast latency budget validation.
- **`ci-full`**: Standard production-grade limits (100M CPU instructions, 40MB memory, 100 footprint entries, 10M stroops fee).
- **`dev-fast`**: Relaxed iteration profile (50M CPU instructions, 20MB memory, 10% safety margin) for rapid local development loops.

When `--profile` is specified, StarForge asserts all resource metrics against the profile ceilings and exits non-zero if any metric breaches the ceiling. Profile definitions are version-controlled in `starforge-simulation-profiles.toml`.

Example output with profile assertion:

```
Simulated Transaction Resources
─────────────────────────────────
CPU instructions      1,274,180
Memory (bytes)        1,275,072
Footprint entries     3 total (2 read-only, 1 read-write)
Ledger read bytes     8,192
Ledger write bytes    1,024
Simulated at ledger   1234567
─────────────────────────────────
Min resource fee      58,181 stroops (0.0058181 XLM)
Safety margin (20%)   11,636 stroops
Inclusion fee         100 stroops
Recommended fee       69,917 stroops (0.0069917 XLM)

Simulation Profile: ci-smoke
─────────────────────────────────
Description           Strict tight resource limits for fast CI sanity checks
Max CPU               2,000,000
Max Memory            2,097,152 bytes
Max Read Bytes        32,768 bytes
Max Write Bytes       8,192 bytes
Max Footprint Entries 10
Max Fee               100,000 stroops
─────────────────────────────────
✔ All simulation resource metrics within profile 'ci-smoke' ceilings.
```

### Why the margin exists

Ledger state moves between simulation and submission. Submitting exactly
`minResourceFee` is a coin flip — a rent bump or a competing write in the same
ledger pushes the real cost above the simulated one and the transaction fails
with `txINSUFFICIENT_FEE`. The default 20% matches the Stellar CLI. Set
`--margin 0` only when you are replaying against a frozen ledger.

---

## `starforge cost resources`

Prices a saved simulation and checks it against the budgets configured with
`starforge cost budget set`:

```bash norun
starforge cost resources --file simulation.json --network mainnet --enforce
```

| Flag | Default | Purpose |
|---|---|---|
| `--file <PATH>` | required | Saved `simulateTransaction` JSON response |
| `--network <NAME>` | `testnet` | Network whose budgets to check against |
| `--margin <PERCENT>` | `20` | Safety margin over the minimum resource fee |
| `--inclusion-fee <STROOPS>` | `100` | Per-operation inclusion fee |
| `--enforce` | off | Exit non-zero if the fee would exceed a budget |

With `--enforce` this is a CI gate: the command exits non-zero when the
projected period spend crosses the configured limit, so a pipeline can refuse
to deploy without a human decision.

---

## `starforge deploy`

`--simulate` (and `--dry-run`, which implies it) now prints the resource
accounting alongside the fee:

```
Minimum Resource Fee   58181 stroops
CPU instructions       1274180
Memory (bytes)         1275072
Footprint              2 read-only, 1 read-write, 8192 B read, 1024 B written
Recommended fee        69917 stroops (0.0069917 XLM, includes a 20% margin)
```

If the RPC server returns no resource accounting, the extra lines are omitted
rather than filled with invented numbers, and the reason is reported as a
simulation warning.

---

## Compatibility

| RPC field | Protocol 20 | Protocol 21 / 22 | Behaviour |
|---|---|---|---|
| `minResourceFee` | yes | yes | Required. Without it the response is rejected. |
| `cost.cpuInsns` | yes | deprecated | Falls back to the instruction count in `transactionData`. |
| `cost.memBytes` | yes | deprecated | Reported as `not reported` with a warning. |
| `transactionData` | yes | yes | Source of the footprint. Absent → footprint omitted, fee still reported. |
| `restorePreamble` | no | yes | Its `minResourceFee` is added to the plan and a restore warning is printed. |

Numeric fields are accepted as JSON numbers **or** JSON strings, because
stellar-rpc serialises 64-bit counters as strings. Values that are negative,
fractional, or non-numeric are rejected rather than coerced to zero.

### Unsupported environments

- **Not a Soroban RPC server** — a response with neither `minResourceFee` nor
  `transactionData` is rejected with a message saying so, instead of reporting a
  fabricated fee.
- **Host failure** (contract panic, bad auth, budget exceeded) — the `error`
  from the response is surfaced and no fee is planned.
- **Transport failure** — a JSON-RPC `error` member is reported as an RPC error.

---

## Migration note

`SimulationResult::fee` previously reported `cost.cpuInsns`, an **instruction
count**, not a fee. Any script parsing that field as stroops was reading the
wrong number by roughly an order of magnitude. It now reports the RPC's
`minResourceFee`, falling back to `100000` stroops only when the server reports
no resource accounting at all.

The struct gained an optional `resources` field. It is `#[serde(default)]`, so
previously serialised `SimulationResult` JSON still deserialises.

---

## Security

- Simulation responses are read from disk and from the network, so
  `--file` inputs are capped at 8 MiB before the JSON parser runs.
- `transactionData` is decoded with an explicit XDR depth limit, so a nested
  payload cannot drive unbounded recursion.
- Fee arithmetic is overflow-checked: a hostile or corrupted `minResourceFee`
  produces an error rather than a wrapped total that looks affordable.

---

## See also

- [GAS_OPTIMIZATION_GUIDE.md](../GAS_OPTIMIZATION_GUIDE.md) — reducing the resources in the first place
- [docs/COMMAND_REFERENCE.md](COMMAND_REFERENCE.md) — every command and flag
