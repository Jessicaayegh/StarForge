# Strict Privacy Mode

End-to-end privacy mode is StarForge's kill-switch for outbound network activity.
When enabled, **no bytes leave the machine** for automatic operations: telemetry
export, AI cloud calls, and marketplace/template-registry auto-updates are all
blocked at the source.

## Enabling

| Mechanism | Command / value |
|-----------|-----------------|
| CLI | `starforge privacy mode on` |
| Config | `starforge config set privacy.mode true` |
| Environment | `STARFORGE_PRIVACY_MODE=1` (or `true/on/yes`) |

The environment variable overrides the persisted config. `STARFORGE_PRIVACY_MODE=0`
(or `false/off/no`) disables it. Unknown values fail **closed** (treated as on).

Check the effective status at any time:

```bash run
starforge privacy mode status
```

## What is blocked

| Channel | Implementation |
|---------|----------------|
| Telemetry | `src/utils/telemetry.rs` — `is_telemetry_enabled()` returns `false`, so even local capture is skipped. |
| AI cloud calls | `src/utils/ai_offline.rs` — `configured_mode()` is forced to offline, so cloud-only AI commands fail clearly and Ollama is never bypassed. |
| Marketplace / registry auto-update | `src/utils/templates.rs` — `load_registry()` serves the local cache or the bundled registry; the remote conditional fetch is never attempted. |

Explicit, user-requested network operations (e.g. `template install --url`,
`marketplace publish`) are unaffected by design — privacy mode targets automatic
data egress, not deliberate user actions.

## Design

- Precedence: `STARFORGE_PRIVACY_MODE` > `privacy.mode` config > default (`false`).
- Resolution helpers live in `src/utils/privacy.rs` (`is_privacy_mode_enabled()`,
  `require_online()`) and are unit-tested there.
- `config` subcommand shows the effective status under `privacy.mode`.