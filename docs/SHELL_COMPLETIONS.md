# Shell Completion for StarForge

StarForge ships pre-generated completion scripts for **bash**, **zsh**, and
**fish** in the `completions/` directory and can also generate them on-demand
via the `starforge completions` subcommand.

---

## Quick install

### Bash

```bash norun
# One-time — generate and install to the user completion directory
mkdir -p ~/.local/share/bash-completion/completions
starforge completions bash > ~/.local/share/bash-completion/completions/starforge

# Or source it directly from your shell profile
echo 'source <(starforge completions bash)' >> ~/.bashrc
source ~/.bashrc
```

### Zsh

```zsh norun
# Ensure a completions directory is on your $fpath, then install
mkdir -p ~/.zsh/completions
starforge completions zsh > ~/.zsh/completions/_starforge

# Add the directory to $fpath in ~/.zshrc (if not already present)
echo 'fpath=(~/.zsh/completions $fpath)' >> ~/.zshrc
echo 'autoload -Uz compinit && compinit' >> ~/.zshrc
source ~/.zshrc
```

### Fish

```fish norun
starforge completions fish > ~/.config/fish/completions/starforge.fish
```

Changes take effect in new shell sessions (or after `source`-ing the file).

---

## Using the pre-generated scripts

The `completions/` directory in the repository contains scripts generated from
the current CLI definition:

| File | Shell |
|---|---|
| `completions/starforge.bash` | bash |
| `completions/_starforge` | zsh |
| `completions/starforge.fish` | fish |

Copy the relevant file to the location shown above instead of running
`starforge completions <shell>`.

---

## Supported shells

| Shell | Status |
|---|---|
| bash | Fully supported |
| zsh | Fully supported |
| fish | Fully supported |

PowerShell completion is not currently supported.

---

## Verifying your installation

After installing, open a **new** terminal session and type:

```bash norun
starforge <TAB>
```

You should see a list of available subcommands. Press `<TAB>` again after
typing a subcommand prefix to narrow the choices.

### Bash

```bash norun
$ starforge w<TAB>
wallet
```

### Zsh

```zsh norun
$ starforge <TAB>
wallet     -- Manage test wallets (create, list, fund, show, remove)
new        -- Generate Soroban project boilerplate
deploy     -- Deploy a compiled Soroban contract (.wasm)
...
```

### Fish

```fish norun
$ starforge <TAB>
wallet   (Manage test wallets)
new      (Generate Soroban project boilerplate)
...
```

---

## Troubleshooting

**Completions not working after install**

1. Open a *new* terminal window — changes are not applied to running sessions.
2. Verify the file is in the correct location:
   - bash: `~/.local/share/bash-completion/completions/starforge`
   - zsh: a directory on your `$fpath`, e.g. `~/.zsh/completions/_starforge`
   - fish: `~/.config/fish/completions/starforge.fish`
3. Check that `bash-completion` (bash) or `compinit` (zsh) is active in your
   shell profile.

**Completions are stale after upgrading StarForge**

Re-run the install command to regenerate:

```bash norun
starforge completions bash > ~/.local/share/bash-completion/completions/starforge
```

**Zsh: `command not found: compdef`**

Add the following lines to `~/.zshrc` before the `fpath` entry:

```zsh norun
autoload -Uz compinit
compinit
```

---

## Keeping completions up to date

Completion scripts are generated from the live CLI definition, so they always
reflect the current set of commands and flags. Re-generate after any StarForge
upgrade to pick up new subcommands.

The `completions/` scripts in the repository are regenerated automatically as
part of the release process.
