#!/usr/bin/env python3
"""Record the README demo: scaffold -> build -> import wallet -> deploy -> invoke.

Runs the real commands in a throwaway HOME and writes an asciinema v2
recording (docs/assets/demo.cast). If `agg` (https://github.com/asciinema/agg)
is installed, it also renders docs/assets/demo.gif for the README.

Nothing is faked: every line in the recording is the output of the command
shown (long outputs are trimmed to their last lines). The run deploys to
testnet from a fresh Friendbot-funded account, so it needs network access
plus the Soroban toolchain:

    rustup target add wasm32v1-none
    stellar-cli (`stellar`) on PATH   # builds, signs the deploy, invokes

Usage:
    cargo build --release
    python3 scripts/record-demo.py [--bin target/release/starforge] [--no-gif]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ASSETS = REPO_ROOT / "docs" / "assets"
COLS, ROWS = 100, 30
TYPE_DELAY = 0.035
PROMPT = "\x1b[1;36m$\x1b[0m "
ANSI = re.compile(r"\x1b\[[0-9;]*m")


class Cast:
    def __init__(self) -> None:
        self.t = 0.0
        self.events: list[list] = []

    def out(self, text: str, delay: float = 0.0) -> None:
        self.t += delay
        self.events.append([round(self.t, 3), "o", text])

    def type(self, command: str) -> None:
        self.out(PROMPT, 0.6)
        for ch in command:
            self.out(ch, TYPE_DELAY)
        self.out("\r\n", 0.3)

    def comment(self, text: str) -> None:
        self.out(f"\x1b[2m# {text}\x1b[0m\r\n", 0.8)

    def write(self, path: Path) -> None:
        header = {"version": 2, "width": COLS, "height": ROWS,
                  "env": {"SHELL": "/bin/bash", "TERM": "xterm-256color"},
                  "title": "StarForge: scaffold, deploy and call a Soroban contract"}
        with path.open("w", encoding="utf-8") as f:
            f.write(json.dumps(header) + "\n")
            for e in self.events:
                f.write(json.dumps(e) + "\n")


def run(cast: Cast, shown: str, env: dict, cwd: Path, actual: str | None = None,
        keep: int | None = None, pick: str | None = None) -> str:
    """Type `shown`, run `actual` (default: `shown`), replay its output.

    `pick` keeps only lines matching the regex (first occurrence of each), and
    `keep` keeps only the last N lines, so long reports fit on one screen.
    """
    cast.type(shown)
    proc = subprocess.run(["bash", "-euo", "pipefail", "-c", actual or shown], cwd=cwd, env=env,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    lines = [l for l in proc.stdout.splitlines() if l.strip()]
    if pick is not None:
        seen: set[str] = set()
        lines = [l for l in lines if re.search(pick, ANSI.sub("", l))
                 and not (ANSI.sub("", l) in seen or seen.add(ANSI.sub("", l)))]
    if keep is not None and len(lines) > keep:
        lines = lines[-keep:]
    for line in lines:
        cast.out(line + "\r\n", 0.05)
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.exit(f"error: `{shown}` failed with exit {proc.returncode}")
    return ANSI.sub("", proc.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin", default="target/release/starforge")
    parser.add_argument("--no-gif", action="store_true")
    args = parser.parse_args()

    binary = (REPO_ROOT / args.bin).resolve()
    for tool, hint in [(str(binary), "cargo build --release"), ("stellar", "install stellar-cli")]:
        if not shutil.which(tool) and not Path(tool).is_file():
            sys.exit(f"error: {tool} not found ({hint})")

    sandbox = Path(tempfile.mkdtemp(prefix="sf-demo-"))
    home, work, bindir = sandbox / "home", sandbox / "work", sandbox / "bin"
    for d in (home, work, bindir):
        d.mkdir()
    (bindir / "starforge").symlink_to(binary)
    real_home = Path.home()
    env = {
        "PATH": f"{bindir}{os.pathsep}{os.environ['PATH']}",
        "HOME": str(home),
        "CARGO_HOME": os.environ.get("CARGO_HOME", str(real_home / ".cargo")),
        "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(real_home / ".rustup")),
        "CLICOLOR_FORCE": "1",
        "TERM": "xterm-256color",
        "COLUMNS": str(COLS),
        "STARFORGE_TELEMETRY": "0",
        "STARFORGE_NON_INTERACTIVE": "1",
    }

    cast = Cast()
    try:
        cast.comment("1. Scaffold a Soroban contract from a template")
        run(cast, "starforge -q new contract hello", env, work, keep=6)

        cast.comment("2. Build it to WebAssembly (stellar-cli)")
        project = work / "hello"
        run(cast, "cd hello && stellar contract build", env, work,
            pick=r"Wasm File|Exported Functions|• hello|Build Complete")

        cast.comment("3. Bring a stellar-cli identity into StarForge and fund it")
        run(cast, "stellar keys generate demo", env, project, keep=1)
        run(cast, "starforge -q wallet import --from-stellar-cli demo", env, project, keep=3)
        run(cast, "starforge -q wallet fund demo", env, project, keep=2)

        cast.comment("4. Validate, plan and deploy")
        wasm = "target/wasm32v1-none/release/hello.wasm"
        out = run(cast, f"starforge -q deploy --wasm {wasm} --wallet demo --yes --execute",
                  env, project,
                  pick=r"Risk Level|WASM size|Network |XLM Balance|Signer|Executing|Contract ID|executed successfully")
        ids = re.findall(r"\bC[A-Z2-7]{55}\b", out)
        if not ids:
            sys.exit("error: no contract ID in deploy output")
        contract_id = ids[-1]

        cast.comment("5. Call it")
        run(cast, f"stellar contract invoke --id {contract_id} --source demo --network testnet -- hello --to Stellar",
            env, project, keep=2)
        cast.out("", 2.5)
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)

    ASSETS.mkdir(parents=True, exist_ok=True)
    cast_path = ASSETS / "demo.cast"
    cast.write(cast_path)
    print(f"wrote {cast_path.relative_to(REPO_ROOT)}")

    if not args.no_gif:
        agg = shutil.which("agg")
        if not agg:
            print("note: `agg` not found; skipping GIF (cargo install --locked --git https://github.com/asciinema/agg)")
        else:
            gif = ASSETS / "demo.gif"
            subprocess.run([agg, "--font-size", "16", "--idle-time-limit", "2", str(cast_path), str(gif)], check=True)
            print(f"wrote {gif.relative_to(REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
