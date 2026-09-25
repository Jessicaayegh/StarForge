#!/usr/bin/env bash
# install.sh — starforge installer
#
# Downloads, verifies, and installs a starforge release binary. Installs the
# latest release by default, or a specific pinned version (see below).
#
# Supported platforms:
#   OS:   linux, darwin (macOS)
#   Arch: x86_64, aarch64 / arm64
#
# Usage:
#   install.sh              # install the latest release
#   install.sh vX.Y.Z       # install a specific pinned version (checksum-verified same as latest)
#
# Environment variables (all optional):
#   INSTALL_DIR   Override the installation directory  (default: /usr/local/bin)
#
# Version pinning:
#   Pass a release tag (e.g. "v1.4.0") as the first argument to install that
#   exact version instead of whatever is currently "latest". This is useful
#   for reproducible environments (CI, Docker images) where an unannounced
#   new release should never silently change what gets installed.
#
#     ./install.sh v1.4.0
#
#   The pinned version is checksum-verified exactly the same way as "latest"
#   — see the security note below. If the tag doesn't exist, the download
#   step fails and nothing is installed.
#
# Rollback:
#   Before overwriting an existing install, the installer backs up the
#   current binary (if any) to "$INSTALL_DIR/starforge.bak". If a newly
#   installed version misbehaves, roll back with:
#
#     mv "$INSTALL_DIR/starforge.bak" "$INSTALL_DIR/starforge"
#
#   Or reinstall a known-good pinned version directly:
#
#     ./install.sh v1.3.0
#
#   See docs/INSTALL_HARDENING.md for the full, tested rollback walkthrough.
#
# Uninstall:
#   rm -f /usr/local/bin/starforge   # or wherever you installed it
#
# Security note:
#   The SHA-256 checksum of the downloaded archive is verified against the
#   release's SHA256SUMS.txt before the binary is extracted. The installer
#   aborts if the checksum does not match. See docs/INSTALL_HARDENING.md for
#   how to verify the checksum manually before running this script at all.
set -euo pipefail

# Canonical repository. This is intentionally not overridable from the
# environment: release archives and SHA256SUMS.txt must only ever come from
# the canonical project (see scripts/check-canonical-urls.sh).
readonly REPO="Nanle-code/StarForge"

# ── resolve requested version ─────────────────────────────────────────────────
# First positional argument pins an exact release tag; omit it for "latest".
PINNED_TAG="${1:-}"

# ── platform detection ────────────────────────────────────────────────────────
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
    x86_64)         ARCH="x86_64" ;;
    aarch64|arm64)  ARCH="aarch64" ;;
    *)
        echo "error: unsupported architecture: $ARCH" >&2
        echo "       starforge supports x86_64 and aarch64 / arm64." >&2
        exit 1
        ;;
esac

case "$OS" in
    linux|darwin) ;;
    *)
        echo "error: unsupported operating system: $OS" >&2
        echo "       starforge supports linux and darwin (macOS)." >&2
        echo "       On Windows, download the .zip release from GitHub directly." >&2
        exit 1
        ;;
esac

# ── resolve install directory ─────────────────────────────────────────────────
# Tests and callers can override via INSTALL_DIR.
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# ── working directory (auto-cleaned on exit) ──────────────────────────────────
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

TAR_FILE="starforge-${OS}-${ARCH}.tar.gz"

# ── resolve release tag ────────────────────────────────────────────────────────
if ! command -v curl >/dev/null 2>&1; then
    echo "error: curl is required to download starforge." >&2; exit 1
fi

if [ -n "$PINNED_TAG" ]; then
    echo "Installing pinned version $PINNED_TAG for starforge..."
    TAG="$PINNED_TAG"
else
    echo "Fetching latest release for starforge..."
    API_URL="https://api.github.com/repos/$REPO/releases/latest"
    TAG=$(curl -s "$API_URL" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

    if [ -z "$TAG" ]; then
        echo "error: failed to fetch the latest release version." >&2
        echo "       Check your internet connection or visit: https://github.com/$REPO/releases" >&2
        exit 1
    fi
fi

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$TAR_FILE"
CHECKSUM_URL="https://github.com/$REPO/releases/download/$TAG/SHA256SUMS.txt"

# ── download archive ──────────────────────────────────────────────────────────
echo "Downloading $DOWNLOAD_URL..."
curl -sL "$DOWNLOAD_URL" -o "$WORK_DIR/$TAR_FILE" || { echo "error: download failed." >&2; exit 1; }

# ── download checksums ────────────────────────────────────────────────────────
echo "Downloading checksum file..."
curl -sL "$CHECKSUM_URL" -o "$WORK_DIR/checksums.txt" || { echo "error: checksum download failed." >&2; exit 1; }

# ── verify checksum ───────────────────────────────────────────────────────────
echo "Verifying checksum..."
if command -v sha256sum >/dev/null 2>&1; then
    grep "$TAR_FILE" "$WORK_DIR/checksums.txt" | (cd "$WORK_DIR" && sha256sum -c -)
elif command -v shasum >/dev/null 2>&1; then
    grep "$TAR_FILE" "$WORK_DIR/checksums.txt" | (cd "$WORK_DIR" && shasum -a 256 -c -)
else
    echo "warning: no sha256 checksum tool found — skipping verification." >&2
fi

# ── extract and install ───────────────────────────────────────────────────────
echo "Extracting..."
tar -xzf "$WORK_DIR/$TAR_FILE" -C "$WORK_DIR"

echo "Installing to $INSTALL_DIR..."

# Back up any existing binary before overwriting it, so a bad upgrade can be
# rolled back without re-downloading anything. Only one prior version is
# kept (the backup is itself overwritten on each install) — pin an exact
# version with `install.sh vX.Y.Z` for anything more durable than "the one
# before this".
if [ -f "$INSTALL_DIR/starforge" ]; then
    if [ -w "$INSTALL_DIR" ]; then
        cp -f "$INSTALL_DIR/starforge" "$INSTALL_DIR/starforge.bak"
    else
        sudo cp -f "$INSTALL_DIR/starforge" "$INSTALL_DIR/starforge.bak"
    fi
fi

if [ -w "$INSTALL_DIR" ]; then
    mv -f "$WORK_DIR/starforge" "$INSTALL_DIR/"
else
    sudo mv -f "$WORK_DIR/starforge" "$INSTALL_DIR/"
fi
chmod +x "$INSTALL_DIR/starforge"

# WORK_DIR is removed by the EXIT trap.
echo "starforge $TAG installed successfully!"
echo "Run 'starforge --version' to verify."
echo ""
echo "To uninstall: rm -f $INSTALL_DIR/starforge"
if [ -f "$INSTALL_DIR/starforge.bak" ]; then
    echo "To roll back to the previous version: mv $INSTALL_DIR/starforge.bak $INSTALL_DIR/starforge"
fi
