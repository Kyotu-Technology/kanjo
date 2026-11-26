---
paths:
  - "crates/kanjo-core/src/store/**"
  - "crates/kanjo-ops/src/retention.rs"
---

# Store, write path and schema

- `store/write.rs` is the only code that inserts or updates `requests`, `sessions` and `usage_hourly`, and it keeps the three in step. The only other code that changes them is deletion in `kanjo_ops::retention`. A new way to add usage rows is a new function in `write.rs`, not SQL in another crate.
- The schema changes only by appending a `(version, sql)` entry to `MIGRATIONS` in `store/mod.rs`. Existing entries, `schema.sql` included, are never edited. `SCHEMA_VERSION` follows from the last entry; `kanjo doctor` and backup restore compare against it.
- Each migration runs in one immediate transaction, after a `pre-migration-v<n>` copy of the database. Write migrations so that they succeed on a database of any size and can be retried after a crash.
- A new table or column that holds anything about the user goes into `docs/security/data-inventory.md` in the same change. No column may hold prompt or response text.
- Timestamps are epoch milliseconds UTC. Functions take the time through `&Ctx` or an explicit argument; they do not read the clock.
- Store calls are blocking. Async callers wrap them in `spawn_blocking`.
- The `add-migration` skill has the full procedure.
