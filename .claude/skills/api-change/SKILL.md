---
name: api-change
description: Change a JSON shape of the Kanjo HTTP API, the CLI --json output or the UI settings, and carry it through the generated TypeScript bindings, the web UI, the mock server and docs/api.md. Use for any edit to crates/kanjo-core/src/api.rs or settings.rs.
---

# Changing an API shape

JSON shapes live in `crates/kanjo-core/src/api.rs` (inside the `dto!` macro, which derives serde and `ts_rs::TS` with `#[ts(export)]`) and in `settings.rs`. Nothing else defines one.

1. Edit the type. Keep the conventions from the module comment: epoch milliseconds, `*_usd`, `*_ratio` 0..1, `*_pct` 0..100. An unpriced cost is an `Option` that serialises to `null`. Give a public field a doc comment when its meaning is not obvious from the name; the comment is carried into the binding.
2. Update the code that fills the type (the read model in `kanjo-analytics`, `kanjo-alerts` or `kanjo-ops`, and the handler in `crates/kanjo-server/src/handlers.rs`) and the CLI if it prints the type.
3. Regenerate the bindings:

   ```sh
   scripts/gen-bindings.sh
   ```

   It deletes `web/src/lib/api/bindings/`, runs `cargo test -p kanjo-core export_bindings` and rewrites `web/src/lib/api/contract.ts`. Commit the generated files; do not edit them.
4. Update the UI: the client in `web/src/lib/api/client.ts` and `queries.ts` for a new endpoint, the components that read the type, and the mock server in `web/src/lib/mock` so `?mock` still serves the same shape. New text needs entries in `en.ts` and `pl.ts`.
5. Update `docs/api.md` for the endpoint, and add a CHANGELOG entry under Unreleased when a field was added, changed or removed.
6. Verify:

   ```sh
   cargo test --workspace --locked
   (cd web && npm run check && npm test && npm run build)
   ```

   `npm run check` fails when the UI still uses the old shape, which is the point of the generated bindings.
