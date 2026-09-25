# Installing StarForge

StarForge is released only from the canonical repository,
[github.com/Nanle-code/StarForge](https://github.com/Nanle-code/StarForge).
Don't install binaries or scripts from any other fork or mirror. CI rejects
links to other owners ([`scripts/check-canonical-urls.sh`](https://github.com/Nanle-code/StarForge/blob/master/scripts/check-canonical-urls.sh)).

## Quick install (macOS / Linux)

```bash norun
curl -fsSL https://raw.githubusercontent.com/Nanle-code/StarForge/master/install.sh | bash
```

The script:

1. Detects your OS and CPU architecture.
2. Looks up the latest release of `Nanle-code/StarForge`. The repository is
   hard-coded and can't be overridden from the environment.
3. Downloads `starforge-<os>-<arch>.tar.gz` and the release's `SHA256SUMS.txt`.
4. **Verifies the SHA-256 checksum** and stops if it doesn't match.
5. Installs the binary to `/usr/local/bin` (or `INSTALL_DIR`) and cleans up.

> **Security note:** read [`install.sh`](https://github.com/Nanle-code/StarForge/blob/master/install.sh) before you pipe it to
> `bash`. You can also download the archive from the
> [Releases page](https://github.com/Nanle-code/StarForge/releases) and check it
> yourself:
>
> ```bash norun
> sha256sum -c SHA256SUMS.txt --ignore-missing
> ```

### Custom install directory

`INSTALL_DIR` has to be set for `bash`, the process that runs the script, not
for `curl`:

```bash norun
curl -fsSL https://raw.githubusercontent.com/Nanle-code/StarForge/master/install.sh \
  | INSTALL_DIR="$HOME/.local/bin" bash
```

### Uninstall

```bash norun
rm -f /usr/local/bin/starforge          # or "$INSTALL_DIR/starforge"
rm -rf ~/.starforge                     # optional: wallets, config and history
```

## Platform support

| OS | Architecture | Supported |
|---|---|---|
| Linux | x86\_64 | ✅ |
| Linux | aarch64 | ✅ |
| macOS | x86\_64 | ✅ |
| macOS | aarch64 (Apple Silicon) | ✅ |
| Windows | x86\_64 | ✅ `.zip` from [Releases](https://github.com/Nanle-code/StarForge/releases) |
| FreeBSD / other | — | ❌ |

On every push, CI smoke-tests the Windows binary: it checks that
`starforge.exe` starts and that `--help` and `config doctor` work
([`tests/installer/windows_smoke.ps1`](https://github.com/Nanle-code/StarForge/blob/master/tests/installer/windows_smoke.ps1)).
The release pipeline won't publish a Windows binary that fails those checks.

## Homebrew

A draft formula lives in [`packaging/homebrew/starforge.rb`](https://github.com/Nanle-code/StarForge/blob/master/packaging/homebrew/starforge.rb).
No Homebrew tap has been published yet, so use the install script or build
from source for now.

## Build from source

You need Rust 1.80 or newer ([rustup](https://rustup.rs)). On Linux, also install
`libudev-dev` for hardware-wallet support.

```bash norun
git clone https://github.com/Nanle-code/StarForge.git
cd StarForge
cargo install --path . --locked
```

## Verify the installation

```bash run
starforge --version
starforge info
```

## Companion tools

StarForge builds on the standard Soroban toolchain rather than replacing it:

- **Rust with the `wasm32v1-none` target**, to compile contracts
  (`rustup target add wasm32v1-none`).
- **[stellar-cli](https://developers.stellar.org/docs/tools/cli)**. `starforge
  deploy --execute` passes the final on-chain submission to `stellar contract
  deploy`. See [Migrating from stellar-cli](MIGRATING_FROM_STELLAR_CLI.md).
- **Docker** (optional), for `starforge node start` (a local quickstart network).
