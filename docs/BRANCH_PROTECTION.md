# Branch Protection & Merge Gates

The default branch (`master`) is protected. A pull request can merge **only**
when both of the following are true:

1. **Every required CI status check is green on the latest commit of the PR.**
   A check that is pending, failing, cancelled, or was run against an older
   commit blocks the merge. Pushing a new commit re-runs every check.
2. **The branch merges cleanly into `master` with no conflicts.** If GitHub
   reports "This branch has conflicts that must be resolved", rebase onto the
   latest `master`, resolve the conflicts locally, re-run the preflight script,
   and force-push with `--force-with-lease`.

Review feedback must also be addressed and all review threads resolved.

Run [`scripts/preflight-pr.sh`](../scripts/preflight-pr.sh) before opening or
updating a PR. It runs the same gates locally and exits non-zero if any fail.

## Required status checks

These are the job names from [`.github/workflows/ci.yml`](../.github/workflows/ci.yml)
as they appear in the PR checks list. Configure exactly these names as required
checks in branch protection.

| Required check | What it enforces | Local equivalent (preflight gate) |
|---|---|---|
| `Rustfmt` | Code is formatted | `cargo fmt --all --check` |
| `MSRV (Rust 1.80)` | Workspace builds on the minimum supported Rust | `cargo check --locked --workspace` (run with a 1.80 toolchain for full parity) |
| `Cargo Deny` | Advisories, licenses, banned and duplicate crates | `cargo deny check --all-features` |
| `Secure Defaults Audit` | Security-sensitive defaults stay safe | `cargo test --test secure_defaults_audit --locked` |
| `Documentation Tests` | Doc examples compile and pass | `cargo test --doc --locked` (`--all`) |
| `Feature Matrix` | Build, JSON contract stability, full test suite across feature combinations (default, no-default, ai, hardware) | `cargo build`, `cargo test` with various `--features` combinations |
| `Docs Cheat Sheet (anti-drift)` | `docs/COMMAND_CHEATSHEET.md` matches the clap metadata | `cargo build --locked` then `git diff --exit-code -- docs/COMMAND_CHEATSHEET.md` |
| `Clippy Lint` | No lint warnings with every feature enabled | `cargo clippy --all-features --locked -- -D warnings` |
| `CLI Smoke Tests (Linux)` | End-to-end CLI behaviour | `cargo test --test cli_cross_platform --locked`, `cargo test --test cli_smoke --locked`, `bash scripts/e2e-smoke.sh` |
| `macOS CLI Tests` | Cross-platform CLI behaviour on macOS | Covered by CI only |
| `Windows CLI Tests` | Cross-platform CLI behaviour on Windows | Covered by CI only |
| `Reproducible WASM Build` | Builds a sample contract twice and checks hash equality | Covered by CI only |

## Conflict-free requirement

GitHub computes mergeability against the current tip of `master`, which moves
as other PRs land. Keep your branch current:

```bash norun
git fetch origin
git rebase origin/master
# resolve conflicts, then
./scripts/preflight-pr.sh
git push --force-with-lease
```

The preflight script's first gate fails when a merge or rebase is in progress,
when the index has unmerged paths, when modified files still contain conflict
markers, or when a trial merge of `HEAD` into `origin/master` would conflict.
The trial merge uses `git merge-tree` (Git 2.38 or newer) and never touches your
working tree.

## Running the preflight script

```bash norun
./scripts/preflight-pr.sh            # standard merge gates
./scripts/preflight-pr.sh --quick    # fmt, clippy, JSON contract, unit tests
./scripts/preflight-pr.sh --all      # everything, including doctests and the full suite
./scripts/preflight-pr.sh --fix      # run cargo fmt first, then verify
./scripts/preflight-pr.sh --base main
```

It exits `0` only when every gate passes and `1` if any gate fails, and it
lists the failed gates in its summary.

## Configuring protection (maintainers)

In **Settings → Branches → Branch protection rules** for `master`, enable:

- *Require a pull request before merging*, with at least one approval.
- *Require status checks to pass before merging*, and add every check in the
  table above.
- *Require branches to be up to date before merging*, so checks run against the
  latest `master`.
- *Require conversation resolution before merging*.
- *Do not allow bypassing the above settings*.

The same configuration can be applied with the GitHub CLI:

```bash norun
gh api -X PUT repos/Nanle-code/StarForge/branches/master/protection \
  --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "Rustfmt", "MSRV (Rust 1.80)", "Cargo Deny", "Secure Defaults Audit",
      "Documentation Tests", "Build and Test", "Docs Cheat Sheet (anti-drift)",
      "Hardware Wallet (optional backends)", "Clippy Lint",
      "CLI Smoke Tests (Linux)", "macOS CLI Tests", "Windows CLI Tests",
      "Reproducible WASM Build"
    ]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": { "required_approving_review_count": 1 },
  "required_conversation_resolution": true,
  "restrictions": null
}
JSON
```

When a job is added to or renamed in `ci.yml`, update this table, the
`contexts` list, and the matching gate in `scripts/preflight-pr.sh` in the same
PR.
