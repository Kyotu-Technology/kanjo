---
name: add-migration
description: Add a database schema migration to Kanjo (a new table, column or index). Use for any change to the SQLite schema; the existing schema.sql and earlier migrations are never edited.
---

# Adding a schema migration

The schema is defined by `MIGRATIONS` in `crates/kanjo-core/src/store/mod.rs`, a list of `(version, sql)` pairs applied in order. `SCHEMA_VERSION` is the version of the last entry.

1. Read `crates/kanjo-core/src/store/schema.sql` and the later migration files to see the current schema. Do not edit any of them.
2. Write the migration as a new file next to `schema.sql`, named `migration_<n>.sql`, where `<n>` is the next version. It runs with `execute_batch` inside one immediate transaction, so it must not contain `BEGIN`, `COMMIT` or `VACUUM`. Prefer `ALTER TABLE ... ADD COLUMN` with a default over rebuilding a table; a rebuild of `requests` can take minutes on a large database.
3. Append `(<n>, include_str!("migration_<n>.sql"))` to `MIGRATIONS`.
4. If the change touches `requests`, `sessions` or `usage_hourly`, update `store/write.rs` so the three stay consistent, and nothing else writes them.
5. Add a test in `store/mod.rs` that creates a database at the previous version, migrates it and checks both the new structure and that existing rows survive. The helpers in `kanjo_core::testkit` create stores in temporary directories.
6. Update what depends on the version: `kanjo doctor` and backup restore compare against `SCHEMA_VERSION` and need no change, but their tests may pin a number. Search for it:

   ```sh
   grep -rn "SCHEMA_VERSION\|user_version" crates --include=*.rs
   ```

7. If the new table or column holds anything about the user, describe it in `docs/security/data-inventory.md`. No column may hold prompt or response text.
8. Verify with `cargo test --workspace --locked`, then start `cargo run -p kanjo -- demo --no-open` once to see a fresh database come up on the new schema.
