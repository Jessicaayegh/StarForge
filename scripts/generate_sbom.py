#!/usr/bin/env python3
"""Generate CycloneDX and SPDX Software Bill of Materials (SBOM) for StarForge."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

try:
    import tomllib  # Python 3.11+
except ImportError:
    try:
        import tomli as tomllib  # type: ignore
    except ImportError:
        tomllib = None  # Handled with basic parser if needed


def parse_cargo_lock(lock_path: Path) -> List[Dict[str, Any]]:
    """Extract all resolved dependencies from Cargo.lock."""
    if not lock_path.is_file():
        raise FileNotFoundError(f"Cargo.lock not found at: {lock_path}")

    content = lock_path.read_text(encoding="utf-8")
    
    if tomllib is not None:
        data = tomllib.loads(content)
        return data.get("package", [])

    # Minimal fallback parser if tomllib is not installed
    packages = []
    current_pkg: Dict[str, Any] = {}
    for line in content.splitlines():
        line = line.strip()
        if line == "[[package]]":
            if current_pkg and "name" in current_pkg:
                packages.append(current_pkg)
            current_pkg = {}
        elif line.startswith("name = "):
            current_pkg["name"] = line.split("=", 1)[1].strip().strip('"')
        elif line.startswith("version = "):
            current_pkg["version"] = line.split("=", 1)[1].strip().strip('"')
        elif line.startswith("source = "):
            current_pkg["source"] = line.split("=", 1)[1].strip().strip('"')
        elif line.startswith("checksum = "):
            current_pkg["checksum"] = line.split("=", 1)[1].strip().strip('"')
    if current_pkg and "name" in current_pkg:
        packages.append(current_pkg)
    return packages


def generate_cyclonedx(packages: List[Dict[str, Any]], root_version: str = "0.1.0") -> Dict[str, Any]:
    """Build a CycloneDX 1.5 JSON SBOM."""
    components = []
    for pkg in packages:
        name = pkg.get("name", "")
        version = pkg.get("version", "0.0.0")
        checksum = pkg.get("checksum")
        
        comp: Dict[str, Any] = {
            "type": "application" if name == "starforge" else "library",
            "bom-ref": f"pkg:cargo/{name}@{version}",
            "name": name,
            "version": version,
            "purl": f"pkg:cargo/{name}@{version}",
            "scope": "required",
        }
        if checksum:
            comp["hashes"] = [
                {
                    "alg": "SHA-256",
                    "content": checksum,
                }
            ]
        components.append(comp)

    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:starforge-sbom-{root_version}",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "starforge",
                "version": root_version,
                "description": "StarForge - Developer tooling and CLI for the Stellar network and Soroban smart contracts",
                "purl": f"pkg:cargo/starforge@{root_version}",
            },
            "tools": [
                {
                    "vendor": "StarForge",
                    "name": "generate_sbom.py",
                    "version": "1.0.0",
                }
            ],
            "licenses": [
                {
                    "license": {
                        "id": "MIT",
                    }
                }
            ],
        },
        "components": components,
    }


def generate_spdx(packages: List[Dict[str, Any]], root_version: str = "0.1.0") -> Dict[str, Any]:
    """Build an SPDX 2.3 JSON SBOM."""
    spdx_packages = []
    relationships = []
    root_spdx_id = "SPDXRef-Package-starforge"

    # Root package
    spdx_packages.append({
        "SPDXID": root_spdx_id,
        "name": "starforge",
        "versionInfo": root_version,
        "downloadLocation": "https://github.com/Nanle-code/StarForge",
        "filesAnalyzed": False,
        "licenseConcluded": "MIT",
        "licenseDeclared": "MIT",
        "copyrightText": "NOASSERTION",
        "externalRefs": [
            {
                "referenceCategory": "PACKAGE-MANAGER",
                "referenceType": "purl",
                "referenceLocator": f"pkg:cargo/starforge@{root_version}",
            }
        ],
    })

    relationships.append({
        "spdxElementId": "SPDXRef-DOCUMENT",
        "relationshipType": "DESCRIBES",
        "relatedSpdxElement": root_spdx_id,
    })

    for pkg in packages:
        name = pkg.get("name", "")
        if name == "starforge":
            continue
        version = pkg.get("version", "0.0.0")
        checksum = pkg.get("checksum")
        safe_name = name.replace("-", "_")
        pkg_spdx_id = f"SPDXRef-Package-{safe_name}-{version.replace('.', '_')}"

        pkg_obj: Dict[str, Any] = {
            "SPDXID": pkg_spdx_id,
            "name": name,
            "versionInfo": version,
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "copyrightText": "NOASSERTION",
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": f"pkg:cargo/{name}@{version}",
                }
            ],
        }
        if checksum:
            pkg_obj["checksums"] = [
                {
                    "algorithm": "SHA256",
                    "checksumValue": checksum,
                }
            ]
        spdx_packages.append(pkg_obj)

        relationships.append({
            "spdxElementId": root_spdx_id,
            "relationshipType": "DEPENDS_ON",
            "relatedSpdxElement": pkg_spdx_id,
        })

    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"starforge-{root_version}",
        "documentNamespace": f"https://github.com/Nanle-code/StarForge/spdx/starforge-{root_version}",
        "creationInfo": {
            "creators": [
                "Tool: StarForge generate_sbom.py-1.0.0",
                "Organization: StarForge",
            ],
            "created": "2026-09-25T00:00:00Z",
        },
        "packages": spdx_packages,
        "relationships": relationships,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock-file", type=Path, default=Path("Cargo.lock"), help="Path to Cargo.lock")
    parser.add_argument("--version", default="0.1.0", help="Release version of starforge")
    parser.add_argument("--out-dir", type=Path, default=Path("dist"), help="Output directory for SBOM files")
    parser.add_argument("--format", choices=["cyclonedx", "spdx", "both"], default="both", help="SBOM format to generate")
    args = parser.parse_args()

    try:
        packages = parse_cargo_lock(args.lock_file)
        args.out_dir.mkdir(parents=True, exist_ok=True)

        if args.format in ("cyclonedx", "both"):
            cdx = generate_cyclonedx(packages, args.version)
            cdx_path = args.out_dir / "starforge-sbom.cdx.json"
            cdx_path.write_text(json.dumps(cdx, indent=2), encoding="utf-8")
            print(f"Generated CycloneDX SBOM: {cdx_path} ({len(cdx['components'])} components)")

        if args.format in ("spdx", "both"):
            spdx = generate_spdx(packages, args.version)
            spdx_path = args.out_dir / "starforge-sbom.spdx.json"
            spdx_path.write_text(json.dumps(spdx, indent=2), encoding="utf-8")
            print(f"Generated SPDX SBOM: {spdx_path} ({len(spdx['packages'])} packages)")

    except Exception as exc:
        print(f"Error generating SBOM: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
