# CLI output accessibility

StarForge's terminal output uses color (via the `colored` crate) and
decorative Unicode symbols (`✓` success, `✗` error, `⚠` warning, `→`
info/hint) to distinguish message kinds at a glance. Neither is ever the
*only* signal: every one of these carries a distinct symbol and a distinct
message prefix regardless of color support, so a monochrome terminal, a
color-blind user, or a grep over piped output can already tell them apart
without relying on color.

What color and Unicode symbols do not serve well is a screen reader or a
braille display: ANSI escape codes are noise to announce, and the Unicode
glyphs above are announced inconsistently (or not at all, or as an unrelated
character description) across screen readers. Plain mode exists for that
case, and for anyone else who wants ASCII-only output — a log file, a CI
runner, a terminal multiplexer with limited Unicode support.

## Enabling plain mode

Any of the following enables it, checked in this order:

1. `--plain` (a global flag, available on every subcommand).
2. `$NO_COLOR` set to any non-empty value — the cross-tool convention
   described at <https://no-color.org>, honored here even though it is
   named for color specifically: a caller who sets it gets both no color and
   no decorative Unicode, since both exist for the same "glanceable in a
   normal terminal" purpose that a screen-reader/log-file context doesn't
   share.
3. `$STARFORGE_NO_COLOR` set to a truthy value (`1`, `true`, `yes`, `on`),
   the project-namespaced equivalent of `$NO_COLOR`.

```bash
starforge --plain wallet list
NO_COLOR=1 starforge wallet list
STARFORGE_NO_COLOR=1 starforge wallet list
```

## What changes in plain mode

- `colored::control::set_override(false)` is set process-wide at startup,
  which disables ANSI color for every `colored` call in the codebase, not
  only the ones described below.
- `utils::print`'s message-kind helpers (`success`, `error`, `warn`, `info`,
  and the top-level `cli_error` handler `main.rs` uses to report a failed
  command) swap their Unicode symbol for an ASCII label:

  | Kind | Normal | Plain |
  | --- | --- | --- |
  | Success | `✓` | `[OK]` |
  | Error | `✗` | `[ERROR]` |
  | Warning | `⚠` | `[WARN]` |
  | Info / hint | `→` | `[INFO]` (or `-` for a `cli_error` hint bullet) |

## What does not change

- `--json` output (see `docs/CLI_JSON_STABILITY.md`) is already
  machine-readable and carries no color or symbols; `--plain` and `--json`
  are independent and can be combined, though a command that supports
  `--json` should generally be driven that way by anything parsing its
  output programmatically.
- Table layout (`utils::print::table`), progress bars, and spinners are
  unaffected beyond losing color; a progress bar or spinner intended for a
  screen-reader-friendly workflow should be paired with `--quiet` (suppresses
  the banner) or redirected output, not treated as accessible on its own.

## Extending this to a new call site

Most of the CLI already goes through `utils::print`'s helpers, so a new
command gets plain-mode support for free by using `p::success`/`p::error`/
`p::warn`/`p::info`/`p::cli_error` rather than a direct `println!` with an
inline `colored` call. A call site that must print outside those helpers
should check `crate::utils::output::is_plain_mode_enabled()` itself and
follow the same ASCII-label convention above, so a future consistency pass
recognizes it as one more site to migrate rather than a special case.
