"""Unit tests for scripts/release_notes.py.

These tests create a throwaway git repository in a temporary directory, write a
handful of conventional commits, and assert on the generated Markdown.
"""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import release_notes  # noqa: E402


def git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


class ReleaseNotesTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name)
        git(self.repo, "init", "-q", "-b", "main")
        git(self.repo, "config", "user.email", "test@starforge.dev")
        git(self.repo, "config", "user.name", "StarForge Test")
        git(self.repo, "config", "commit.gpgsign", "false")
        os.environ.setdefault("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+0000")
        os.environ.setdefault("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+0000")

    def _commit(self, message: str) -> None:
        (self.repo / "data.txt").write_text(f"{message}\n", encoding="utf-8")
        git(self.repo, "add", ".")
        git(self.repo, "commit", "-q", "-m", message)

    def _tag(self, name: str) -> None:
        git(self.repo, "tag", name)

    def test_full_range_from_first_commit(self) -> None:
        self._commit("feat: add interface diff tool")
        self._commit("fix: correct exit-code classification")
        self._commit("docs: document degrade mode")

        notes = release_notes.generate_notes(self.repo)

        self.assertIn("## Features", notes)
        self.assertIn("add interface diff tool", notes)
        self.assertIn("## Bug Fixes", notes)
        self.assertIn("correct exit-code classification", notes)
        self.assertIn("## Documentation", notes)

    def test_breaking_change_section_and_scoped_commits(self) -> None:
        self._commit("feat(upgrade): add diff command")
        self._commit("feat!: remove deprecated bind endpoint")
        self._commit("fix(wallet): validate keys before signing")

        notes = release_notes.generate_notes(self.repo)

        self.assertIn("## Breaking Changes", notes)
        self.assertIn("remove deprecated bind endpoint", notes)
        self.assertIn("Breaking Changes", notes.split("## Features")[0])
        self.assertIn("**upgrade:**", notes)
        self.assertIn("**wallet:**", notes)

    def test_issue_references_are_rendered(self) -> None:
        self._commit("feat: add interface diff\n\nCloses #802")

        notes = release_notes.generate_notes(self.repo)

        self.assertIn("#802", notes)
        self.assertIn("(#802)", notes)

    def test_range_between_tags(self) -> None:
        self._commit("feat: baseline")
        self._tag("v0.1.0")
        self._commit("fix: patch after first release")

        notes = release_notes.generate_notes(self.repo, from_ref="v0.1.0")

        self.assertNotIn("baseline", notes)
        self.assertIn("patch after first release", notes)

    def test_from_defaults_to_previous_tag(self) -> None:
        self._commit("chore: init")
        self._tag("v0.1.0")
        self._commit("perf: reduce deploy latency")

        notes = release_notes.generate_notes(self.repo)

        self.assertNotIn("init", notes)
        self.assertIn("## Performance", notes)
        self.assertIn("reduce deploy latency", notes)

    def test_unclassified_commit_lands_in_other(self) -> None:
        self._commit("random noise commit")

        notes = release_notes.generate_notes(self.repo)

        self.assertIn("## Other", notes)
        self.assertIn("random noise commit", notes)

    def test_version_heading(self) -> None:
        self._commit("feat: something")

        notes = release_notes.generate_notes(self.repo, version="0.4.0")

        self.assertTrue(notes.startswith("# StarForge v0.4.0"))


if __name__ == "__main__":
    unittest.main()