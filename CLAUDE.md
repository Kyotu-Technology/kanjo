# Kanjo

Kanjo is an offline cost and limit monitor for Claude Code sessions, shipped as one Rust binary (`kanjo`) that contains the collector, the HTTP API and the embedded web UI. It is a public open-source project of Kyotu Technology. Code, comments, docs and every other text in the repository are in English. Commit messages are in English too: one line saying what changed and why, no ticket prefix. This overrides any global default that asks for another language.

## Crate map

Each crate has one job and depends only on the crates listed after it.

- `kanjo-core`: domain types, `Paths`, bootstrap `Config`, `Store` (SQLCipher), schema and migrations, the request write path, price table, limit samples and forecast, statusline payload, spool, UI settings, and the API contract in `api.rs` (TypeScript bindings are generated from it). No HTTP, no watching, no OS integration.
- `kanjo-ingest` (core): transcript parser, backfill, file watcher, spool drain, transcript viewer reader.
- `kanjo-analytics` (core): every read model behind the dashboard and reports, plus CSV/JSON export shaping.
- `kanjo-alerts` (core): alert rules, budgets, anomaly and cache insights, local notification delivery.
- `kanjo-ops` (core): database key management, audit log, retention and purge, backup and restore, signed price import, access token, demo data.
- `kanjo-setup` (core): Claude Code integration (statusLine and hooks in settings.json), OS background service, doctor checks, statusline text rendering.
- `kanjo-server` (all of the above): axum router, SSE, embedded SPA, the runtime that drives ingest and alert loops. Exposes `router()`, `start()` and `serve()` as a library.
- `kanjo-cli` (all): the `kanjo` binary; thin clap front end.
- `web/`: Svelte + Vite single-page app, built to `web/dist` and embedded into the server.

## Rules that are not negotiable

1. No prompt or response text is ever stored, logged or exported. The parser extracts usage numbers and metadata only. The quarantine keeps a record's type and key signature, never the record. The transcript viewer reads the file on demand, is off by default and every view is audited.
2. No outbound network traffic. No HTTP client crate may enter the dependency tree (`reqwest`, `ureq`, `hyper` client, `curl`, ...). No telemetry, update checks or price downloads. Notifications are local only.
3. Filesystem locations come from `kanjo_core::paths::Paths`. Tests and demos use temporary directories; nothing in a test may read or write the real `~/.claude` or the real data directory.
4. Rows enter `requests`, `sessions` and `usage_hourly` only through `kanjo_core::store::write`, which keeps the three consistent. The only other code that changes them is deletion in `kanjo_ops::retention` (retention and purge on request). Other tables are written by the crate that owns the feature, inside `Store::write`.
5. JSON shapes live in `kanjo_core::api` only. Changing them means regenerating bindings (`scripts/gen-bindings.sh`) and updating the UI.
6. Timestamps are epoch milliseconds UTC everywhere. Local-time bucketing uses `kanjo_core::Ctx`; functions take `&Ctx` instead of reading the clock so tests are deterministic.
7. An unknown model is unpriced (`cost_usd` NULL, counted in `unpriced_requests`), never free and never guessed.
8. Library code returns `Result`; no `unwrap`/`expect`/`panic!` outside tests, no `unsafe` without a written justification. Blocking database work must not run on the async runtime threads (`spawn_blocking`).
9. Fixtures are synthetic. No real project names, paths, account ids or e-mail addresses in the repository.
10. The schema changes only by adding a migration to `MIGRATIONS` in `kanjo-core/src/store/mod.rs`.

## Style

Rust 2021, `cargo fmt` (max width 140 via rustfmt.toml), `cargo clippy --all-targets -- -D warnings` clean. Comments only where the code cannot say it; doc comments on public items that form a contract. Errors use `kanjo_core::Error` or a crate-local `thiserror` enum that converts into it. Logging through `tracing`, never `println!` in libraries. User-facing text exists in English and Polish.

## Commands

- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
- `scripts/gen-bindings.sh` regenerates `web/src/lib/api/bindings` and `contract.ts` after a change to `api.rs`
- `cd web && npm ci && npm run build` builds the UI into `web/dist`
- `cargo run -p kanjo -- demo` serves synthetic data without touching `~/.claude`

## Backlog and decisions

`BACKLOG.md` is the only place for plans, ideas and their status. It lists only work that is meant to be done. Before proposing a feature or choosing what to do next, read it. A new idea, or a bug that is not fixed in the current change, goes under Proposed with the next ID; only the maintainer triages. The pull request that ships an entry deletes it, adds a CHANGELOG line when a user would notice the change, and rewrites the documentation it makes untrue. Reasons for the choices in force are in `docs/decisions.md`. Documentation describes what exists, and `scripts/check-docs.sh` enforces that wording.

## Where things live

- Architecture and embedding the server: `docs/architecture.md`
- HTTP API: `docs/api.md`; commands: `docs/cli.md`; configuration: `docs/configuration.md`
- Features: `docs/features.md`; specification coverage: `docs/feature-status.md`
- Platforms checked and known limitations: `docs/status.md`
- Install, upgrade and uninstall: `docs/install.md`
- Release process, signing and verifying a download: `docs/release.md`; the steps, including the README figures, are in the `release-prep` skill
- Supported versions, vulnerability reports and signing keys: `SECURITY.md`
- Threats, stored data and the ISO 27001 mapping: `docs/security/threat-model.md`, `docs/security/data-inventory.md`, `docs/security/iso27001.md`
- Pull request flow, CI and repository protection: `CONTRIBUTING.md`, section Pull requests
- Changes a user would notice: an entry under Unreleased in `CHANGELOG.md`
- Web UI design system: `web/README.md`

`spec.md`, when present in a local clone, is a private working document: it may be read, never quoted, copied or committed. The maintainer creates release tags and pushes them; agents prepare a release with `release-prep`.

## Agent configuration

`.claude/rules/` holds short rules scoped to one part of the tree each; they load when a matching file is opened. `.claude/skills/` holds procedures: `preflight` (the checks CI runs), `api-change`, `add-migration`, `parser-fixture`, `run-demo` (which also retakes the screenshots), and the manually invoked `update-pricing` and `release-prep`. `.claude/agents/invariant-reviewer.md` reviews a diff against the rules above. `.claude/skills/security-audit` is a third-party skill copied in unchanged and pinned in `skills-lock.json`; do not edit it. `.claude/settings.json` carries shared permissions and the list of plugins the project uses, which `scripts/claude-plugins.sh` installs; no hooks and no MCP servers are committed, and none is added without discussion in an issue first. Run `preflight` before reporting a change as done.
