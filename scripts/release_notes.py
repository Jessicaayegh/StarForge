#!/usr/bin/env python3
"""Generate Markdown release notes from conventional git history.

The generator reads the commit stream between two refs, classifies each commit
by its conventional-commit type (feat, fix, perf, refactor, docs, test, build,
ci, chore) and emits a governance/publication-ready Markdown release notes file.

Usage:
    python scripts/release_notes.py --from v0.1.0 --to v0.2.0 --out BODY.md
    python scripts/release_notes.py --version 0.3.0 --out BODY.md

If `--from` is omitted the script uses the most recent reachable tag as the
start of the range; if no tag exists the notes cover the full history.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

# Conventional-commit type -> Markdown section heading.
TYPE_SECTIONS = {
    "feat": "Features",
    "feature": "Features",
    "features": "Features",
    "fix": "Bug Fixes",
    "bugfix": "Bug Fixes",
    "hotfix": "Bug Fixes",
    "perf": "Performance",
    "performance": "Performance",
    "refactor": "Refactoring",
    "docs": "Documentation",
    "doc": "Documentation",
    "test": "Testing",
    "tests": "Testing",
    "build": "Build & CI",
    "ci": "Build & CI",
    "chore": "Chores",
    "style": "Chores",
    "revert": "Reverts",
}

# Stable output ordering for sections.
SECTION_ORDER = [
    "Breaking Changes",
    "Features",
    "Bug Fixes",
    "Performance",
    "Refactoring",
    "Documentation",
    "Testing",
    "Build & CI",
    "Chores",
    "Reverts",
    "Other",
]

# `type(scope)!: subject` with an optional scope and breaking-change marker.
SUBJECT_RE = re.compile(
    r"^(?P<type>[a-z][a-z0-9-]*)(?:\((?P<scope>[^)]+)\))?(?P<break>!)?:\s+(?P<subject>.+)$"
)

# Trailers/issues referenced from the subject or body: `Closes #12`, `Fixes #7`.
ISSUE_RE = re.compile(r"(?:closes|fixes|resolves)\s+#(\d+)", re.IGNORECASE)


@dataclass
class Commit:
    """A single commit with a parsed conventional-commit subject."""

    short_sha: str
    subject: str
    body: str

    def parse(self) -> tuple[str, str, bool]:
        """Return (section, subject_text, is_breaking) for this commit."""
        match = SUBJECT_RE.match(self.subject)
        if not match:
            return "Other", self.subject, False
        commit_type = match.group("type").lower()
        scope = match.group("scope")
        breaking = bool(match.group("break"))
        subject_text = match.group("subject")
        if scope:
            subject_text = f"**{scope}:** {subject_text}"
        section = TYPE_SECTIONS.get(commit_type, "Other")
        if breaking:
            section = "Breaking Changes"
        return section, subject_text, breaking

    def issue_refs(self) -> list[str]:
        """Extract `#<n>` issue references so PRs can be closed from the notes."""
        haystack = f"{self.subject}\n{self.body}"
        return [f"#{n}" for n in dict.fromkeys(ISSUE_RE.findall(haystack))]


def git(repo: str | Path, args: list[str]) -> str:
    """Run a git command in `repo` and return trimmed stdout."""
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def previous_tag(repo: str | Path) -> str | None:
    """Return the most recent tag that is an ancestor of the current work.

    Tries `HEAD^` first so that a tag freshly placed on the current commit does
    not collapse the range to an empty diff.
    """
    for rev in ("HEAD^", "HEAD"):
        try:
            return git(repo, ["describe", "--tags", "--abbrev=0", rev])
        except subprocess.CalledProcessError:
            continue
    return None


def collect_commits(repo: str | Path, revision_range: str) -> list[Commit]:
    """Return commits in `revision_range` (merge commits excluded)."""
    fmt = "--format=%h%x00%s%x00%b%x00"
    out = git(repo, ["log", "--no-merges", fmt, revision_range])
    parts = out.split("\x00")
    commits: list[Commit] = []
    for index in range(0, len(parts) - 2, 3):
        short_sha, subject, body = parts[index : index + 3]
        commits.append(Commit(short_sha=short_sha, subject=subject, body=body or ""))
    return commits


def generate_notes(
    repo: str | Path,
    from_ref: str | None = None,
    to_ref: str = "HEAD",
    version: str | None = None,
) -> str:
    """Build the Markdown release notes for the `from_ref..to_ref` range."""
    if from_ref is None:
        from_ref = previous_tag(repo)
    revision_range = f"{from_ref}..{to_ref}" if from_ref else to_ref
    commits = collect_commits(repo, revision_range)

    sections: dict[str, list[tuple[str, str]]] = {name: [] for name in SECTION_ORDER}
    for commit in commits:
        section, subject_text, _breaking = commit.parse()
        refs = commit.issue_refs()
        suffix = f" ({', '.join(refs)})" if refs else ""
        sections.setdefault(section, []).append((commit.short_sha, f"{subject_text}{suffix}"))

    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    heading = f"# StarForge v{version}" if version else "# StarForge Release Notes"

    lines = [
        heading,
        "",
        f"_Generated from `{from_ref or 'history start'}..{to_ref}` at {now}_",
        "",
        f"**{len(commits)} commit(s), {sum(len(items) for items in sections.values())} classified changes.**",
        "",
    ]

    for section in SECTION_ORDER:
        items = sections.get(section, [])
        if not items:
            continue
        lines.append(f"## {section}")
        lines.append("")
        for short_sha, text in items:
            lines.append(f"- `{short_sha}` {text}")
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate Markdown release notes from conventional git history."
    )
    parser.add_argument("--repo", default=".", help="Path to the git repository (default: current dir)")
    parser.add_argument("--from", dest="from_ref", default=None,
                        help="Start ref (default: most recent reachable tag)")
    parser.add_argument("--to", dest="to_ref", default="HEAD", help="End ref (default: HEAD)")
    parser.add_argument("--version", default=None, help="Release version, added to the title")
    parser.add_argument("--out", default=None, help="Write the notes to a file (default: stdout)")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)
    notes = generate_notes(
        repo=args.repo,
        from_ref=args.from_ref,
        to_ref=args.to_ref,
        version=args.version,
    )
    if args.out:
        Path(args.out).write_text(notes, encoding="utf-8")
    else:
        sys.stdout.write(notes)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())