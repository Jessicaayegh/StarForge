#!/usr/bin/env bash
# tests/installer/test_install_e2e.sh
#
# End-to-end test for install.sh. Unlike test_install.sh, the installer is run
# unmodified: a fake `curl` placed first on PATH plays the role of GitHub and
# serves a release (latest-tag JSON, archive, SHA256SUMS.txt) built from a
# stub binary. Every URL the installer requests is recorded, and the test
# fails if any of them is outside the canonical repository.
#
# Scenarios:
#   - installs from canonical release URLs and the binary runs
#   - requests exactly: releases/latest API, archive, SHA256SUMS.txt
#   - a tampered archive (checksum mismatch) is rejected
#
# Optional live mode downloads the real latest release from GitHub into a
# temp directory (requires network and a published release):
#   STARFORGE_INSTALL_LIVE=1 bash tests/installer/test_install_e2e.sh

set -euo pipefail

CANONICAL_REPO="Nanle-code/StarForge"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALL_SH="$REPO_ROOT/install.sh"

ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

FAILURES=0
pass() { echo "[PASS] $1"; }
fail() { echo "[FAIL] $1"; FAILURES=$((FAILURES + 1)); }

case "$(uname -s)" in
    Linux) OS=linux ;;
    Darwin) OS=darwin ;;
    *) echo "SKIP: unsupported host OS"; exit 0 ;;
esac
case "$(uname -m)" in
    x86_64) ARCH=x86_64 ;;
    aarch64|arm64) ARCH=aarch64 ;;
    *) echo "SKIP: unsupported host arch"; exit 0 ;;
esac
TAR_FILE="starforge-${OS}-${ARCH}.tar.gz"
TAG="v9.9.9"

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}

# Build a fake release: archive + SHA256SUMS.txt covering it and a sibling.
make_release() {
    local dir="$1" tamper="${2:-0}"
    mkdir -p "$dir/stage"
    printf '#!/bin/sh\necho "starforge 9.9.9 (e2e stub)"\n' > "$dir/stage/starforge"
    chmod +x "$dir/stage/starforge"
    tar -czf "$dir/$TAR_FILE" -C "$dir/stage" starforge
    : > "$dir/other.tar.gz"
    (cd "$dir" && sha256 "$TAR_FILE" other.tar.gz) > "$dir/SHA256SUMS.txt"
    if [ "$tamper" = "1" ]; then
        # Replace the archive after checksums were published.
        printf 'tampered' > "$dir/stage/starforge"
        tar -czf "$dir/$TAR_FILE" -C "$dir/stage" starforge
    fi
}

# Fake curl: supports the forms install.sh uses (`curl -s URL` and
# `curl -sL URL -o FILE`). Unknown or non-canonical URLs return exit 22 like
# `curl --fail` would, so a redirected download cannot silently succeed.
make_fake_curl() {
    local bin="$1" release="$2" log="$3"
    mkdir -p "$bin"
    cat > "$bin/curl" <<EOF
#!/usr/bin/env bash
out=""; url=""
while [ \$# -gt 0 ]; do
    case "\$1" in
        -o) out="\$2"; shift 2 ;;
        -*) shift ;;
        *) url="\$1"; shift ;;
    esac
done
echo "\$url" >> "$log"
base="https://github.com/$CANONICAL_REPO/releases/download/$TAG"
case "\$url" in
    "https://api.github.com/repos/$CANONICAL_REPO/releases/latest")
        # Pretty-printed like the real GitHub API response.
        body=\$'{\n  "url": "https://api.github.com/repos/$CANONICAL_REPO/releases/1",\n  "tag_name": "$TAG",\n  "name": "StarForge $TAG"\n}' ;;
    "\$base/$TAR_FILE") src="$release/$TAR_FILE" ;;
    "\$base/SHA256SUMS.txt") src="$release/SHA256SUMS.txt" ;;
    *) echo "curl: (22) unexpected URL: \$url" >&2; exit 22 ;;
esac
if [ -n "\$out" ]; then
    if [ -n "\${src:-}" ]; then cp "\$src" "\$out"; else printf '%s\n' "\$body" > "\$out"; fi
else
    if [ -n "\${src:-}" ]; then cat "\$src"; else printf '%s\n' "\$body"; fi
fi
EOF
    chmod +x "$bin/curl"
}

run_installer() {
    local case_dir="$1"
    PATH="$case_dir/bin:$PATH" INSTALL_DIR="$case_dir/prefix" bash "$INSTALL_SH"
}

# ── 1. install from canonical release ────────────────────────────────────────
test_installs_from_canonical_release() {
    local d="$ROOT/ok"; mkdir -p "$d/prefix"
    make_release "$d/release"
    make_fake_curl "$d/bin" "$d/release" "$d/urls.log"

    if ! run_installer "$d" > "$d/out.log" 2>&1; then
        cat "$d/out.log"; return 1
    fi
    [ "$("$d/prefix/starforge")" = "starforge 9.9.9 (e2e stub)" ] || { echo "installed binary did not run"; return 1; }

    local expected
    expected="$(printf '%s\n' \
        "https://api.github.com/repos/$CANONICAL_REPO/releases/latest" \
        "https://github.com/$CANONICAL_REPO/releases/download/$TAG/$TAR_FILE" \
        "https://github.com/$CANONICAL_REPO/releases/download/$TAG/SHA256SUMS.txt")"
    if [ "$(cat "$d/urls.log")" != "$expected" ]; then
        echo "unexpected URLs requested:"; cat "$d/urls.log"; return 1
    fi
}

# ── 2. tampered archive is rejected ──────────────────────────────────────────
test_rejects_tampered_archive() {
    local d="$ROOT/tampered"; mkdir -p "$d/prefix"
    make_release "$d/release" 1
    make_fake_curl "$d/bin" "$d/release" "$d/urls.log"

    if run_installer "$d" > "$d/out.log" 2>&1; then
        echo "installer succeeded on tampered archive"; cat "$d/out.log"; return 1
    fi
    [ ! -e "$d/prefix/starforge" ] || { echo "tampered binary was installed"; return 1; }
}

# ── 3. optional: real download from GitHub ───────────────────────────────────
test_live_install() {
    local d="$ROOT/live"; mkdir -p "$d/prefix"
    INSTALL_DIR="$d/prefix" bash "$INSTALL_SH"
    "$d/prefix/starforge" --version
}

for t in test_installs_from_canonical_release test_rejects_tampered_archive; do
    if (set -e; "$t"); then pass "$t"; else fail "$t"; fi
done

if [ "${STARFORGE_INSTALL_LIVE:-0}" = "1" ]; then
    if (set -e; test_live_install); then pass test_live_install; else fail test_live_install; fi
fi

echo "Results: $FAILURES failed"
[ "$FAILURES" -eq 0 ]
