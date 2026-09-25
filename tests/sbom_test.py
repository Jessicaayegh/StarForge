import json
import tempfile
import unittest
from pathlib import Path

from scripts.generate_sbom import generate_cyclonedx, generate_spdx, parse_cargo_lock
from scripts.release_artifacts import REQUIRED_ARCHIVES, SBOM_ARTIFACTS, prepare_release


class SbomGenerationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.dummy_packages = [
            {
                "name": "starforge",
                "version": "0.1.0",
                "source": "registry+https://github.com/rust-lang/crates.io-index",
                "checksum": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            },
            {
                "name": "soroban-sdk",
                "version": "21.0.0",
                "source": "registry+https://github.com/rust-lang/crates.io-index",
                "checksum": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            },
            {
                "name": "stellar-strkey",
                "version": "0.0.8",
                "source": "registry+https://github.com/rust-lang/crates.io-index",
            },
        ]

    def test_generate_cyclonedx_structure(self) -> None:
        cdx = generate_cyclonedx(self.dummy_packages, "0.1.0")
        
        self.assertEqual(cdx["bomFormat"], "CycloneDX")
        self.assertEqual(cdx["specVersion"], "1.5")
        self.assertEqual(cdx["metadata"]["component"]["name"], "starforge")
        self.assertEqual(cdx["metadata"]["component"]["version"], "0.1.0")
        
        components = cdx["components"]
        self.assertEqual(len(components), 3)
        
        # Check purl and type
        names = {c["name"]: c for c in components}
        self.assertIn("starforge", names)
        self.assertEqual(names["starforge"]["type"], "application")
        self.assertEqual(names["soroban-sdk"]["type"], "library")
        self.assertEqual(names["soroban-sdk"]["purl"], "pkg:cargo/soroban-sdk@21.0.0")
        self.assertEqual(names["soroban-sdk"]["hashes"][0]["content"], "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890")

    def test_generate_spdx_structure(self) -> None:
        spdx = generate_spdx(self.dummy_packages, "0.1.0")
        
        self.assertEqual(spdx["spdxVersion"], "SPDX-2.3")
        self.assertEqual(spdx["dataLicense"], "CC0-1.0")
        self.assertEqual(spdx["name"], "starforge-0.1.0")
        
        packages = spdx["packages"]
        self.assertEqual(len(packages), 3)
        
        pkg_names = {p["name"]: p for p in packages}
        self.assertIn("starforge", pkg_names)
        self.assertIn("soroban-sdk", pkg_names)
        self.assertEqual(pkg_names["soroban-sdk"]["versionInfo"], "21.0.0")
        
        relationships = spdx["relationships"]
        self.assertTrue(any(r["relationshipType"] == "DESCRIBES" for r in relationships))
        self.assertTrue(any(r["relationshipType"] == "DEPENDS_ON" for r in relationships))

    def test_prepare_release_with_sbom_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            source = root / "artifacts"
            destination = root / "release"
            source.mkdir()

            # Create required binary archives
            for name in REQUIRED_ARCHIVES:
                (source / name).write_bytes(name.encode("ascii"))
            
            # Create SBOM artifacts
            for sbom in SBOM_ARTIFACTS:
                (source / sbom).write_bytes(b'{"bomFormat": "CycloneDX"}')

            published = prepare_release(source, destination, require_sbom=True)
            published_names = {p.name for p in published}

            self.assertEqual(published_names, REQUIRED_ARCHIVES | SBOM_ARTIFACTS)
            manifest = (destination / "SHA256SUMS.txt").read_text(encoding="ascii")
            for item in REQUIRED_ARCHIVES | SBOM_ARTIFACTS:
                self.assertIn(f"  {item}\n", manifest)


if __name__ == "__main__":
    unittest.main()
