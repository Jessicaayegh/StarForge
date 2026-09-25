# Deployment Scaling

Scaling deployments for AI-assisted / multi-contract rollouts: batch execution,
parallel waves, and safe concurrency.

## Parallel orchestration

`starforge orchestrate execute` deploys a manifest's contracts while honouring
the `depends_on` relationships. By default execution is sequential (worker
count = 1). Pass a worker count to deploy independent contracts concurrently:

```bash norun
# Sequential (default)
starforge orchestrate execute --file manifest.json --dry-run

# Up to 4 contracts in parallel, dependency-aware
starforge orchestrate execute --file manifest.json --concurrency 4

# Size the pool from the host CPU count (capped at 8)
starforge orchestrate execute --file manifest.json --concurrency auto
```

### Execution waves

Contracts are partitioned into *waves* by dependency depth:

- Steps in the same wave never depend on each other and may run concurrently.
- Waves are deployed in strict order (a contract is only deployed once every
  contract it `depends_on` is deployed).
- Within a wave, the worker pool is bounded by `--concurrency`, so a large
  manifest cannot oversubscribe an RPC endpoint's rate limit.

Checkpointing is unchanged: state is persisted after every step, so resume
(`--id`) and `orchestrate rollback` behave identically to sequential runs.

## AI concurrency recommendations

`src/utils/deployment_optimizer.rs` exposes
`BatchOptimizer::recommend_concurrency(contract_count, wasm_bytes, host_parallelism)`,
used by AI-generated deploy plans to propose a worker count that balances speed
against RPC limits (large WASM uploads are capped to 2 concurrent workers).

## Reference

| Component | Purpose |
|-----------|---------|
| `src/utils/deploy_orchestrator.rs` | Manifest plans, waves, sequential + parallel executors |
| `src/utils/deployment_optimizer.rs` | AI cost/speed/reliability + concurrency recommendations |
| `src/commands/orchestrate.rs` | `orchestrate` CLI (`--concurrency`, `--dry-run`, `--id`) |
| `src/utils/deployment_checkpoint.rs` | Per-deployment lock + resume/rollback state |