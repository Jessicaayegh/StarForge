# Fuzz Corpus Hygiene Guide

## Overview

StarForge fuzz corpora contain seed inputs for libfuzzer. Over time, corpora grow unbounded, slowing CI and hiding interesting inputs under noise. This guide explains corpus maintenance, hygiene tooling, and best practices.

**Current corpora:** 11 targets under `fuzz/corpus/`

**Hygiene scripts:** `scripts/corpus-hygiene.{sh,ps1}`

---

## Corpus Location & Structure

```
fuzz/
├── corpus/
│   ├── fuzz_contract_spec_parse/     # 5-10 seed inputs
│   ├── fuzz_validate_public_key/     # 3-8 seeds
│   ├── fuzz_wallet_backup_parse/     # 6-12 seeds
│   ├── fuzz_wasm_validation/         # 2-5 seeds
│   └── ... (11 total targets)
├── fuzz_targets/
├── dicts/                             # Fuzzer dictionaries
└── Cargo.toml
```

Each corpus directory contains seed inputs (small files) that libfuzzer starts with. Good seeds exercise important code paths and bootstrap the fuzzer quickly.

---

## Running Hygiene Tools

### Local Development

```bash
# Show statistics for all corpora
./scripts/corpus-hygiene.sh --stats

# Dry-run deduplication (see what would be deleted)
./scripts/corpus-hygiene.sh --dedupe --dry-run

# Actually deduplicate (removes duplicates)
./scripts/corpus-hygiene.sh --dedupe

# Run all checks (dedupe + stats)
./scripts/corpus-hygiene.sh --all
```

### Windows

```powershell
# Show statistics
.\scripts\corpus-hygiene.ps1 -Stats

# Dry-run deduplication
.\scripts\corpus-hygiene.ps1 -Dedupe -DryRun

# Actually deduplicate
.\scripts\corpus-hygiene.ps1 -Dedupe
```

### Output

```
[INFO] Fuzz Corpus Hygiene Report

📦 Corpus: fuzz_wallet_backup_parse
[INFO] Deduplicating corpus: fuzz/corpus/fuzz_wallet_backup_parse
[INFO]   Total files: 8
[INFO]   Duplicates found: 2
[INFO]   Duplicate: seed_1_truncated (hash: a1b2c3d4...)
[INFO]   Deleted: fuzz/corpus/fuzz_wallet_backup_parse/seed_1_truncated
  Count:     6
  Total:     2048 bytes
  Average:   341 bytes
  Min:       32 bytes
  Max:       1024 bytes

[INFO] Corpus hygiene check complete
```

---

## Contributing New Seeds

When you discover an interesting crash or edge case, save it as a regression fixture:

### 1. Find the crash file

After running `cargo fuzz run fuzz_my_target` and finding a crash:

```
libFuzzer found a crash: fuzz/artifacts/fuzz_my_target/crash-abc123def456
```

### 2. Minimize the input (optional but recommended)

```bash
# Minimize to smallest input that still triggers the crash
cargo fuzz tmin fuzz_my_target fuzz/artifacts/fuzz_my_target/crash-abc123def456
```

This produces a minimized version that's smaller and easier to debug.

### 3. Add to corpus

```bash
# Copy the crash (or minimized version) to the corpus
cp fuzz/artifacts/fuzz_my_target/crash-abc123def456 \
   fuzz/corpus/fuzz_my_target/seed_regression_xyz

# Or if you have a specific issue number:
cp fuzz/artifacts/fuzz_my_target/crash-abc123def456 \
   fuzz/corpus/fuzz_my_target/issue_1234_regression
```

### 4. Verify the seed triggers the test

```bash
# Run the target with the new seed
cargo fuzz run fuzz_my_target fuzz/corpus/fuzz_my_target/seed_regression_xyz

# It should reproduce the crash immediately
```

### 5. Fix the underlying bug

Update the code to handle the edge case, then verify the test passes:

```bash
# Re-run and verify the crash no longer occurs
cargo fuzz run fuzz_my_target fuzz/corpus/fuzz_my_target/seed_regression_xyz
```

### 6. Commit the seed

```bash
git add fuzz/corpus/fuzz_my_target/seed_regression_xyz
git commit -m "test: add regression seed for issue #1234"
```

---

## Scheduled Corpus Refresh

The `.github/workflows/corpus-refresh.yml` workflow:

- **Runs:** Weekly Monday at 2 AM UTC
- **Action:** Deduplicates all corpora, removes duplicate inputs
- **Commits:** Changes back to main branch if duplicates found
- **Artifacts:** Uploads corpus statistics for analysis

### Manual Trigger

Trigger a dry-run corpus check from GitHub Actions UI:

1. Go to **Actions** → **Fuzz Corpus Refresh**
2. Click **Run workflow**
3. Leave `dry_run=true` (default)
4. Check the PR comment with statistics

Or via `gh` CLI:

```bash
gh workflow run corpus-refresh.yml \
  -f dry_run=true
```

### Scheduled Workflow Permissions

The workflow requires these permissions (check `.github/workflows/corpus-refresh.yml`):

```yaml
permissions:
  contents: write    # To commit deduplicated corpus
  pull-requests: write  # To post PR comments
```

---

## Hygiene Best Practices

### ✓ Do

- Run `--stats` periodically to monitor corpus growth
- Minimize seeds before adding to corpus
- Use descriptive names: `seed_<issue|regression|feature>.bin`
- Commit seeds that expose real bugs
- Review corpus diffs before merging

### ✗ Don't

- Add large files (> 1 MB) without minimization
- Add non-minimal seeds (multiple crash cases in one file)
- Mix success and error cases in one seed
- Leave artifacts in `fuzz/artifacts/` (temporary)
- Commit unreviewed corpus changes

---

## Monitoring Corpus Size

Check corpus size regularly:

```bash
# Show stats for all corpora
./scripts/corpus-hygiene.sh --stats

# Or per-corpus
du -sh fuzz/corpus/fuzz_*/
```

**Target size:** < 50 KB per corpus

**Action threshold:** > 100 KB → Schedule manual deduplication

### Check in CI

The corpus-refresh workflow runs weekly and reports size growth. If the report shows:

```
Average: 512 bytes (OK)
Total: 8 KB (OK)
```

Corpus is healthy.

If it shows:

```
Average: 5120 bytes (⚠ HIGH)
Total: 256 KB (⚠ CRITICAL)
```

Consider deduplicating manually.

---

## Interpreting Hygiene Reports

### Duplicates Found

A duplicate means the same seed (byte-for-byte identical) exists in the corpus twice. This happens when:

1. The same crash is discovered independently
2. A seed is accidentally committed twice
3. libfuzzer generated identical inputs

**Action:** Safe to delete; hygiene script removes them automatically.

### High Average Size

If average seed size is > 1 KB:

1. Seeds may not be minimized
2. Corpus contains concatenated inputs
3. Fuzzer discovered complex failure modes

**Action:** Manually review the largest seeds:

```bash
# Find largest files in a corpus
find fuzz/corpus/fuzz_wallet_backup_parse -type f -exec ls -lh {} \; \
  | sort -k5 -h | tail -5
```

If a seed is much larger than others (> 10 KB), minimize it:

```bash
cargo fuzz tmin fuzz_wallet_backup_parse \
  fuzz/corpus/fuzz_wallet_backup_parse/large_seed
```

---

## Adding New Fuzz Targets

When adding a new fuzz target `fuzz_my_new_target`:

1. Create a new directory:
   ```bash
   mkdir fuzz/corpus/fuzz_my_new_target
   ```

2. Add at least one seed (even if trivial):
   ```bash
   echo "seed" > fuzz/corpus/fuzz_my_new_target/initial_seed
   ```

3. The corpus-refresh workflow automatically includes it

---

## Troubleshooting

### Script fails: "Permission denied"

```bash
# Make script executable
chmod +x scripts/corpus-hygiene.sh
./scripts/corpus-hygiene.sh --stats
```

### Script fails: "sha256sum not found" (macOS)

Use `shasum` instead:

```bash
shasum -a 256 fuzz/corpus/fuzz_wallet_backup_parse/*
```

Or install `sha256sum`:

```bash
brew install coreutils
```

### Corpus directory is empty

Expected for new targets. Add an initial seed:

```bash
# Create a minimal seed
echo "test" > fuzz/corpus/fuzz_my_target/seed_0
```

Libfuzzer will generate more seeds as it discovers mutations.

---

## See Also

- [FUZZING_GUIDE.md](FUZZING_GUIDE.md) — Fuzz targets, property tests, coverage
- [CI_ENFORCEMENT.md](CI_ENFORCEMENT.md) — CI pipeline requirements
- [cargo-fuzz docs](https://rust-fuzz.github.io/book/cargo-fuzz.html) — Official libfuzzer guide
- [Minimization tips](https://llvm.org/docs/LibFuzzer/#minimization) — libfuzzer docs
