# Contract Performance Regression Testing

`starforge perf regression` catches performance degradation in contract updates
before it merges. It tracks named baselines, compares fresh measurements against
them statistically, raises alerts, and produces reports for terminals, PR
comments, and tooling.

## Workflow

```bash norun
# 1. On master (or a release tag), record the baseline.
starforge perf regression baseline --name main --input target/perf.json

# 2. On a PR branch, measure again and compare. Exits 1 on regression.
starforge perf regression check --baseline main --input target/perf.json

# 3. Review how the baseline evolved.
starforge perf regression history --name main
```

Baselines live in `.starforge/perf-baselines/` by default (override with
`--dir`). Commit them so CI and contributors compare against the same numbers:

- `<name>.json` is the current baseline: per-metric sample count, mean, median,
  standard deviation, min, max, and p95, plus the git commit it was recorded at.
- `<name>.history.jsonl` has one line per recorded version, which is where the
  trend in `history` comes from.

## Measurements

Any benchmark harness can produce the input file. Two layouts are accepted:

```json
{ "metrics": { "transfer.cpu_insns": [10231, 10240, 10229], "transfer.mem_bytes": [4096, 4096] } }
```

```json
{
  "metrics": [
    { "name": "transfer.cpu_insns", "unit": "insns", "samples": [10231, 10240, 10229] },
    { "name": "swap.tps", "unit": "tx/s", "higher_is_better": true, "samples": [118, 121] }
  ]
}
```

Metrics are costs by default (lower is better). Set `higher_is_better` for
throughput-style metrics. Every metric needs at least one finite sample.

To measure without a harness, time a command instead:

```bash norun
starforge perf regression check --baseline main \
  --run "cargo test --release --test token_bench" --label token_bench \
  --iterations 10 --warmup 2
```

This records `token_bench.wall_time_ms`. The command must succeed on every run,
because a failing benchmark is a test failure, not a sample. `--run` and
`--input` can be combined.

## Regression detection

Each metric shared by the baseline and the run is classified by the change in
its mean:

| Status | Condition |
|---|---|
| `regressed` | Worse by more than `--fail-pct` (default 10%) or its `--metric-threshold` override |
| `warning` | Worse by more than `--warn-pct` (default 5%) |
| `improved` | Better by more than `--warn-pct` |
| `unchanged` | Anything else, or a change inside the noise band |
| `missing` | In the baseline but not measured, which is treated as a warning because a dropped benchmark hides regressions |
| `new` | Measured but not in the baseline |

**Noise band.** A change smaller than `--noise-sigma` (default 2.0) baseline
standard deviations is reported as `unchanged (noise)`. Noisy metrics such as
wall time therefore don't fail builds on jitter, while deterministic metrics such
as CPU instructions, whose standard deviation is ~0, are held to the percentage
thresholds exactly.

Per-metric thresholds let you be strict where it matters:

```bash norun
starforge perf regression check --baseline main --input perf.json \
  --fail-pct 10 --metric-threshold transfer.cpu_insns=2 --metric-threshold swap.wall_time_ms=25
```

## Verdicts, alerts, and exit codes

The verdict is `FAIL` when any metric regressed, `WARN` when there are only
warnings or missing metrics, and `PASS` otherwise. `--fail-on` decides when the
command exits non-zero:

- `regression` (default) exits non-zero on `FAIL`.
- `warning` exits non-zero on `WARN` or `FAIL`.
- `never` always exits 0, for report-only runs.

Every regression produces a `critical` alert, and every warning or missing
metric a `warning` alert. Alerts are listed in the report. With `--notify` they
are also sent to the notification channels configured with
`starforge contract-monitor notify add --channel <slack|discord|webhook|email> --destination <URL>`.

`--update-baseline` replaces the baseline with the new measurements when the
check does not fail. Use it on `master` so the baseline follows accepted changes.

## Reports

`--format text` (default), `markdown`, or `json`, written to stdout or
`--output <FILE>`. Markdown gives a status table plus alerts, ready for a PR
comment or `$GITHUB_STEP_SUMMARY`. JSON contains the full comparison, policy,
summary, and alerts for dashboards.

## CI example

```yaml
perf-regression:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo install --path . --locked
    - name: Measure
      run: ./scripts/bench-to-json.sh > target/perf.json   # your harness
    - name: Check against baseline
      run: |
        starforge perf regression check --baseline main --input target/perf.json \
          --format markdown --output perf-report.md
    - name: Publish report
      if: always()
      run: cat perf-report.md >> "$GITHUB_STEP_SUMMARY"
```
