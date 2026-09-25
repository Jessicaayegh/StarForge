#!/usr/bin/env bash
# scripts/check-canonical-urls.sh
#
# Fails if any tracked file points at a StarForge repository other than the
# canonical one (Nanle-code/StarForge). Links to forks in install commands,
# release URLs or package metadata are a supply-chain risk: users following
# them could install binaries built by someone else.
#
# Detected references (case-insensitive):
#   github.com/<owner>/StarForge            (web, clone, release URLs)
#   raw.githubusercontent.com/<owner>/StarForge
#   api.github.com/repos/<owner>/StarForge
#   brew install <owner>/starforge/...      (Homebrew taps)
#   REPO="<owner>/StarForge"                (installer scripts)
#   github.com/YOUR_USERNAME/...            (unfilled placeholders)
#
# Files that deliberately contain placeholder URLs as test fixtures are listed
# in ALLOWLIST below; only the placeholder rule is skipped for them.
#
# Usage:
#   bash scripts/check-canonical-urls.sh

set -euo pipefail

CANONICAL_OWNER="Nanle-code"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Paths (relative to repo root) whose placeholder URLs are intentional test
# fixtures for src/utils/cargo_metadata.rs placeholder detection.
ALLOWLIST=(
    "tests/cargo_metadata.rs"
    "docs/CARGO_METADATA.md"
    "scripts/check-canonical-urls.sh"
)

owner='[A-Za-z0-9_.-]+'
# The repository name must end at a non-name character so that plugin repos
# such as Nanle-code/starforge-defi are not mistaken for the main repo.
repo_end='([^A-Za-z0-9_-]|$)'
patterns=(
    "(github\.com|raw\.githubusercontent\.com|api\.github\.com/repos)/${owner}/starforge${repo_end}"
    "brew (install|tap) ${owner}/starforge"
    "REPO=[\"']?${owner}/starforge${repo_end}"
)
placeholder='github\.com/(YOUR_USERNAME|your_username)/'

is_allowlisted() {
    local f="$1" a
    for a in "${ALLOWLIST[@]}"; do
        [ "$f" = "$a" ] && return 0
    done
    return 1
}

violations=0
report() {
    echo "$1"
    violations=$((violations + 1))
}

files=()
while IFS= read -r -d '' f; do
    files+=("$f")
done < <(git ls-files -z)

for pattern in "${patterns[@]}"; do
    while IFS= read -r hit; do
        [ -z "$hit" ] && continue
        file="${hit%%:*}"; rest="${hit#*:}"; lineno="${rest%%:*}"; text="${rest#*:}"
        # A line can hold several references; judge each match on its own.
        while IFS= read -r m; do
            # Owner is the path segment right before "/starforge".
            match_owner="$(sed -E "s#.*[/ =\"']([A-Za-z0-9_.-]+)/[Ss][Tt][Aa][Rr][Ff][Oo][Rr][Gg][Ee].*#\1#" <<<"$m")"
            # Placeholder owners are handled (and allowlisted) by the rule below.
            [ "${match_owner,,}" = "your_username" ] && continue
            if [ "${match_owner,,}" != "${CANONICAL_OWNER,,}" ]; then
                report "$file:$lineno: non-canonical repository reference: $m"
            fi
        done < <(grep -oiE "$pattern" <<<"$text")
    done < <(grep -nHiE "$pattern" -- "${files[@]}" 2>/dev/null || true)
done

while IFS= read -r hit; do
    [ -z "$hit" ] && continue
    file="${hit%%:*}"; rest="${hit#*:}"; lineno="${rest%%:*}"; text="${rest#*:}"
    is_allowlisted "$file" && continue
    report "$file:$lineno: placeholder repository URL: $(grep -oE "${placeholder}[^ )\"'>]*" <<<"$text" | head -1)"
done < <(grep -nHE "$placeholder" -- "${files[@]}" 2>/dev/null || true)

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "error: found $violations non-canonical repository reference(s)."
    echo "       Point them at https://github.com/${CANONICAL_OWNER}/StarForge."
    exit 1
fi

echo "ok: all repository references point at ${CANONICAL_OWNER}/StarForge"
