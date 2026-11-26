---
paths:
  - "crates/kanjo-core/src/api.rs"
  - "crates/kanjo-core/src/settings.rs"
  - "web/src/lib/api/**"
  - "docs/api.md"
---

# API contract

- Every JSON shape exchanged between the engine, the HTTP server, the CLI (`--json`) and the web UI is a type in `crates/kanjo-core/src/api.rs` or `settings.rs`. Handlers, read models and the CLI use those types; none of them defines a response shape of its own.
- Conventions: timestamps are epoch milliseconds, money is USD (`*_usd`), ratios run 0..1 (`*_ratio`, `share`), percentages run 0..100 (`*_pct`). An unpriced cost is `null`, never `0`.
- After changing a type, run `scripts/gen-bindings.sh`. It rewrites `web/src/lib/api/bindings/` and `contract.ts`; both are generated and committed, never edited by hand.
- The same change updates the UI code that uses the type, the mock server in `web/src/lib/mock` and the endpoint description in `docs/api.md`.
- A changed or removed field is a change of the public interface and gets a CHANGELOG entry.
- The `api-change` skill has the full procedure.
