# Software Bill of Materials (SBOM)

StarForge publishes a comprehensive Software Bill of Materials (SBOM) with every release to provide full transparency into dependencies, licensing, and supply chain integrity.

## Formats Available

For every GitHub release tag (`v*`), StarForge produces and publishes:

| File | Standard | Specification Version | Purpose |
|---|---|---|---|
| `starforge-sbom.cdx.json` | [CycloneDX](https://cyclonedx.org/) | 1.5 JSON | Standard for vulnerability analysis and automated security tooling. |
| `starforge-sbom.spdx.json` | [SPDX](https://spdx.dev/) | 2.3 JSON | Standard for license compliance, open-source governance, and asset inventories. |

Both files are published directly to the GitHub Release alongside release archives and recorded in `SHA256SUMS.txt`.

---

## Consumer Verification Steps

### 1. Verify Checksums

Ensure the downloaded SBOM matches the published checksum manifest:

```bash
# Verify all release files and SBOMs against SHA256SUMS.txt
sha256sum --check SHA256SUMS.txt --ignore-missing
```

### 2. Inspect with CycloneDX Tools

You can validate, query, and inspect components using `cyclonedx-cli`:

```bash
# Validate format compliance
cyclonedx-cli validate --input-file starforge-sbom.cdx.json --input-format json --input-version v1_5

# List all component purls
jq '.components[].purl' starforge-sbom.cdx.json
```

### 3. Vulnerability Scanning with Grype or Trivy

Feed the release SBOM into vulnerability management tools:

```bash
# Scan CycloneDX SBOM with Grype
grype sbom:starforge-sbom.cdx.json

# Scan with Trivy
trivy sbom starforge-sbom.cdx.json
```

### 4. Inspect with Syft or SPDX Tools

```bash
# Inspect SPDX document packages
jq '.packages[].name' starforge-sbom.spdx.json
```

---

## Generation & CI/CD Pipeline

SBOMs are generated deterministically in `.github/workflows/release.yml` using `scripts/generate_sbom.py`:

```bash
python scripts/generate_sbom.py \
  --lock-file Cargo.lock \
  --version <VERSION> \
  --out-dir dist \
  --format both
```

All generated SBOMs are attested with GitHub Artifact Attestations (`actions/attest-build-provenance`) for cryptographic supply chain verification.
