# SQLite database schema migrations

StarForge persists wallets, networks, config key/value pairs, and indexed
events in a SQLite database at `~/.starforge/starforge.db` (see
`crate::utils::database::db_path`). This is a separate schema from the
`~/.starforge/config.toml` schema covered by
[`CONFIGURATION_MIGRATIONS.md`](CONFIGURATION_MIGRATIONS.md): the TOML file
holds user-editable configuration, while the SQLite database holds larger,
structured, and frequently-written state.

Its schema version is tracked independently, in the database's own `meta`
table (`schema_version` key), separately from `CURRENT_CONFIG_VERSION`.

## Authoring a migration

1. Increment `CURRENT_SCHEMA_VERSION` in `src/utils/database.rs`.
2. Implement the `Migration` trait for the new version (`up` applies the
   change, `down` reverses it) beside `MigrationV1`.
3. Register the migration in `Database::get_migration`.
4. Add tests: at minimum, one exercising a real upgrade on a file-backed
   database that starts at the previous version and ends at the new one
   (see `migration_upgrades_a_reopened_database_from_n_minus_1_to_n` for the
   pattern — `open_in_memory()` cannot be closed and reopened, so this kind
   of test needs a real temp file), plus a rollback test.

`Database::run_migrations` applies every migration between the database's
current version and `CURRENT_SCHEMA_VERSION` in order, each inside its own
transaction: a failing migration rolls back cleanly rather than leaving the
schema half-upgraded. Each applied migration is recorded in
`schema_migrations` with a checksum of its version and description, so
`Database::get_applied_migrations` gives a full audit trail of what ran and
when.

## Corruption detection

`Database::open()` runs `PRAGMA integrity_check` and `PRAGMA foreign_key_check`
against any *pre-existing* database file (a brand-new file has nothing to be
corrupt yet, so this is skipped for one) before returning. A problem in
either — including the file not being a valid SQLite database at all — fails
with `MigrationError::DatabaseCorrupted` naming the path and the specific
issue(s), rather than surfacing as a confusing, unrelated SQL error the first
time some later query happens to touch the damaged page.

## Backup-before-migrate

`Database::initialize_with_backup(backup_dir)` is the same as
`Database::initialize()`, except that when it is about to apply an actual
upgrade (the database was already initialized, and its schema version trails
`CURRENT_SCHEMA_VERSION`), it first copies the database file into
`backup_dir` as `starforge-pre-migrate-<UTC timestamp>.db` and returns that
path. A fresh database and one that is already current have nothing to
protect, so no backup is taken in either case (`Ok(None)`).

This is opt-in, not the default `initialize()`'s behavior, because most of
the 15+ existing `Database::open()` / `Database::initialize()` call sites run
on every CLI invocation and a schema that is already current (the overwhelming
majority of calls) should stay a fast no-op rather than pay for a backup
check on every command. Call sites that specifically want the safety net
around an upgrade — for example a future `starforge config db migrate`
enhancement — should call `initialize_with_backup` instead.

The backup itself is `Database::backup`, a plain file copy. Because
`open()`/`open_in_memory()` both run in `journal_mode=WAL`, it first issues
`PRAGMA wal_checkpoint(TRUNCATE)` to fold the WAL file's contents back into
the main database file, so the copy does not miss data that has been
committed but not yet checkpointed.

## Recovery

- **Corrupted database, `open()` now fails**: restore a known-good copy with
  `starforge config db restore <backup-file>`, or move the corrupted file
  aside (StarForge will create a fresh one on the next `open()`) if no
  acceptable backup exists.
- **A migration made things worse**: `Database::rollback_migration` reverses
  the single most-recently-applied migration (the CLI does not yet expose
  this as its own subcommand; `starforge config db restore` against a
  pre-migration backup is the supported recovery path today).
- **Manual backup**: `starforge config db backup <dest>` at any time,
  independent of a migration.
- **Verify integrity without opening for normal use**: `starforge config db
  check` runs the same integrity check `open()` runs automatically.

## Review checklist

- The migration is deterministic and idempotent after its version is applied.
- `up` and `down` are both implemented and tested against a real
  previous-version database, not only an in-memory one initialized straight
  at the new version.
- A failing migration leaves the schema at its prior, consistent version
  (verify the transaction actually rolls back).
- `README.md` and release notes describe user-visible migration behavior.
