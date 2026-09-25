#!/usr/bin/env python3
"""Execute and lint shell snippets in StarForge's documentation.

Every shell code block in README.md and docs/ must say whether it is
runnable, via a word after the language in the fence info string:

    ```bash run          executed by this script; must exit 0
    ```bash run fails    executed; must exit non-zero (documents an error)
    ```bash run local    executed only with --local-network (needs a node)
    ```bash norun        not executed (needs secrets, mainnet, Docker, ...)

Any block (YAML, JSON, TOML, ...) can also be materialised for later
snippets in the same file:

    ```yaml file=ops.yaml   written to ops.yaml in the sandbox before the
                            next runnable block executes

An unannotated shell block is an error, as is an untagged block that
contains a `starforge` command. See CONTRIBUTING.md ("Documentation
snippets") for when to use which.

Each file gets its own temporary HOME and working directory; its runnable
blocks run in order in that sandbox, so later blocks can build on earlier
ones (create a wallet, then show it). Blocks run under
`bash -euo pipefail` with the freshly built `starforge` first on PATH.

Failures are reported as `path:line: message`, and as GitHub Actions error
annotations when GITHUB_ACTIONS is set.

Usage:
    python3 scripts/docs-snippets.py                  # lint + run
    python3 scripts/docs-snippets.py --lint-only      # annotations only
    python3 scripts/docs-snippets.py --bin target/debug/starforge docs/FOO.md
    python3 scripts/docs-snippets.py --local-network  # also run `run local`
    python3 scripts/docs-snippets.py --try-unannotated docs/NEW.md
                                   # execute unannotated blocks to decide
                                   # which ones can be marked `run`
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PATHS = ["README.md", "docs"]
SHELL_LANGS = {"bash", "sh", "shell", "console", "zsh", "fish", "powershell", "ps1", "pwsh"}
# Languages this runner can execute on the CI host.
EXECUTABLE_LANGS = {"bash", "sh", "shell", "console"}
FENCE = re.compile(r"^(?P<indent>\s*)(?P<fence>`{3,}|~{3,})(?P<info>.*)$")
COMMAND_LINE = re.compile(r"^\s*(\$\s+)?starforge(\s|$)")
TIMEOUT_SECONDS = int(os.environ.get("DOCS_SNIPPET_TIMEOUT", "120"))


@dataclass
class Snippet:
    path: Path
    line: int  # 1-based line of the opening fence
    lang: str
    attrs: list[str]
    body: str

    @property
    def location(self) -> str:
        return f"{self.path.relative_to(REPO_ROOT)}:{self.line}"

    @property
    def mode(self) -> str | None:
        if "norun" in self.attrs:
            return "norun"
        if "run" in self.attrs:
            return "run"
        return None

    @property
    def file_name(self) -> str | None:
        for a in self.attrs:
            if a.startswith("file="):
                return a.split("=", 1)[1]
        return None

    def runnable(self, try_unannotated: bool) -> bool:
        if self.lang not in EXECUTABLE_LANGS:
            return False
        return self.mode == "run" or (try_unannotated and self.mode is None)


@dataclass
class Report:
    errors: list[tuple[str, str]] = field(default_factory=list)
    ran: int = 0
    skipped: int = 0

    def error(self, location: str, message: str) -> None:
        self.errors.append((location, message))
        path, _, line = location.rpartition(":")
        print(f"{location}: {message.splitlines()[0]}")
        for extra in message.splitlines()[1:]:
            print(f"    {extra}")
        if os.environ.get("GITHUB_ACTIONS"):
            flat = message.replace("%", "%25").replace("\r", "").replace("\n", "%0A")
            print(f"::error file={path},line={line}::{flat}")


def parse_snippets(path: Path) -> list[Snippet]:
    snippets: list[Snippet] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    i = 0
    while i < len(lines):
        m = FENCE.match(lines[i])
        if not m:
            i += 1
            continue
        fence, indent = m.group("fence"), len(m.group("indent"))
        words = m.group("info").strip().split()
        start = i
        body: list[str] = []
        i += 1
        while i < len(lines):
            close = FENCE.match(lines[i])
            if close and close.group("fence")[0] == fence[0] and len(close.group("fence")) >= len(fence) \
                    and not close.group("info").strip():
                break
            # Strip the fence's indentation (blocks nested in list items).
            body.append(lines[i][indent:] if lines[i][:indent].isspace() else lines[i])
            i += 1
        lang = words[0].lower() if words else ""
        attrs = [w if w.startswith("file=") else w.lower() for w in words[1:]]
        snippets.append(Snippet(path, start + 1, lang, attrs, "\n".join(body)))
        i += 1
    return snippets


def lint(snippets: list[Snippet], report: Report) -> None:
    for s in snippets:
        name = s.file_name
        if name is not None and (not name or "/" in name or "\\" in name or name.startswith(".")):
            report.error(s.location, f"`file={name}` must be a plain file name")
        if s.lang in SHELL_LANGS:
            if s.mode is None:
                report.error(s.location, f"unannotated `{s.lang}` snippet: add `run` or `norun` after the language")
            elif s.mode == "run" and s.lang not in EXECUTABLE_LANGS:
                report.error(s.location, f"`{s.lang}` snippets cannot be executed here; mark them `norun`")
        elif not s.lang and any(COMMAND_LINE.match(l) for l in s.body.splitlines()):
            report.error(s.location, "untagged block contains a starforge command: tag it `bash run` or `bash norun`")


def script_for(snippet: Snippet) -> str:
    if snippet.lang != "console":
        return snippet.body
    # console blocks: only `$ ` lines are commands; the rest is sample output.
    return "\n".join(l.split("$ ", 1)[1] for l in snippet.body.splitlines() if l.lstrip().startswith("$ "))


def run_file(snippets: list[Snippet], bin_dir: Path, report: Report, local_network: bool,
             try_unannotated: bool = False) -> None:
    runnable = [s for s in snippets if s.runnable(try_unannotated) or s.file_name]
    if not any(s.runnable(try_unannotated) for s in runnable):
        return
    sandbox = Path(tempfile.mkdtemp(prefix="sf-docs-"))
    home, work = sandbox / "home", sandbox / "work"
    home.mkdir()
    work.mkdir()
    env = {
        "PATH": f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}",
        "HOME": str(home),
        "USERPROFILE": str(home),
        "TMPDIR": str(sandbox),
        "LANG": "C.UTF-8",
        "NO_COLOR": "1",
        "TERM": "dumb",
        "CI": "1",
        "STARFORGE_NON_INTERACTIVE": "1",
        "STARFORGE_TELEMETRY": "0",
        "DOCS_SNIPPET_REPO": str(REPO_ROOT),
    }
    # Keep the real toolchain reachable even though HOME points at the sandbox.
    real_home = Path.home()
    env["CARGO_HOME"] = os.environ.get("CARGO_HOME", str(real_home / ".cargo"))
    env["RUSTUP_HOME"] = os.environ.get("RUSTUP_HOME", str(real_home / ".rustup"))
    if "DOCKER_HOST" in os.environ:
        env["DOCKER_HOST"] = os.environ["DOCKER_HOST"]
    # Local-network settings are passed through for `run local` blocks.
    env.update({k: v for k, v in os.environ.items() if k.startswith("STARFORGE_LOCAL_")})
    try:
        for s in runnable:
            if s.file_name:
                (work / Path(s.file_name).name).write_text(s.body + "\n", encoding="utf-8")
                continue
            if "local" in s.attrs and not local_network:
                report.skipped += 1
                continue
            report.ran += 1
            expect_failure = "fails" in s.attrs
            try:
                proc = subprocess.run(
                    ["bash", "-euo", "pipefail", "-c", script_for(s)],
                    cwd=work, env=env, capture_output=True, text=True, timeout=TIMEOUT_SECONDS,
                )
            except subprocess.TimeoutExpired:
                report.error(s.location, f"snippet timed out after {TIMEOUT_SECONDS}s")
                continue
            failed = proc.returncode != 0
            if failed != expect_failure:
                what = "succeeded but is marked `fails`" if expect_failure else f"failed (exit {proc.returncode})"
                tail = "\n".join((proc.stderr or proc.stdout).strip().splitlines()[-15:])
                report.error(s.location, f"snippet {what}\n{tail}" if tail else f"snippet {what}")
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


def collect(paths: list[str]) -> list[Path]:
    files: list[Path] = []
    for p in paths:
        path = (REPO_ROOT / p).resolve()
        if path.is_dir():
            files.extend(sorted(path.rglob("*.md")))
        elif path.is_file():
            files.append(path)
        else:
            sys.exit(f"error: no such file or directory: {p}")
    return files


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("paths", nargs="*", default=DEFAULT_PATHS, help="files or directories (default: README.md docs)")
    parser.add_argument("--bin", default="target/debug/starforge", help="starforge binary to put on PATH")
    parser.add_argument("--lint-only", action="store_true", help="check annotations without executing")
    parser.add_argument("--local-network", action="store_true", help="also run `run local` snippets")
    parser.add_argument("--try-unannotated", action="store_true",
                        help="also execute unannotated shell blocks (triage helper; implies no lint)")
    args = parser.parse_args()

    report = Report()
    files = collect(args.paths)
    per_file = {f: parse_snippets(f) for f in files}
    if not args.try_unannotated:
        for snippets in per_file.values():
            lint(snippets, report)

    if not args.lint_only:
        binary = (REPO_ROOT / args.bin).resolve()
        if not binary.is_file():
            sys.exit(f"error: {args.bin} not found; run `cargo build` first or pass --bin")
        bin_dir = Path(tempfile.mkdtemp(prefix="sf-docs-bin-"))
        (bin_dir / "starforge").symlink_to(binary)
        try:
            for f, snippets in per_file.items():
                run_file(snippets, bin_dir, report, args.local_network, args.try_unannotated)
        finally:
            shutil.rmtree(bin_dir, ignore_errors=True)

    total = sum(len(s) for s in per_file.values())
    print(f"\n{len(files)} files, {total} code blocks, {report.ran} snippets run, "
          f"{report.skipped} skipped, {len(report.errors)} problem(s)")
    return 1 if report.errors else 0


if __name__ == "__main__":
    sys.exit(main())
