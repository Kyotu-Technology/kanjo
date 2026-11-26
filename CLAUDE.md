# Kanjo

Kanjo: offline cost and limit monitor for Claude Code sessions. One Rust binary (`kanjo`) holds collector, HTTP API, embedded web UI. Public open-source project of Kyotu Technology. Code, comments, docs, all repository text in English. Commit messages in English: one line, what changed and why, no ticket prefix. Overrides any global default asking for another language.

## Crate map

Each crate has one job, depends only on crates listed after it.

- `kanjo-core`: domain types, `Paths`, bootstrap `Config`, `Store` (SQLCipher), schema and migrations, request write path, price table, limit samples and forecast, statusline payload, spool, UI settings, API contract in `api.rs` (TypeScript bindings generated from it). No HTTP, no watching, no OS integration.
- `kanjo-ingest` (core): transcript parser, backfill, file watcher, spool drain, transcript viewer reader.
- `kanjo-analytics` (core): every read model behind dashboard and reports, CSV/JSON export shaping.
- `kanjo-alerts` (core): alert rules, budgets, anomaly and cache insights, local notification delivery.
- `kanjo-ops` (core): database key management, audit log, retention and purge, backup and restore, signed price import, access token, demo data.
- `kanjo-setup` (core): Claude Code integration (statusLine and hooks in settings.json), OS background service, doctor checks, statusline text rendering.
- `kanjo-server` (all of the above): axum router, SSE, embedded SPA, runtime driving ingest and alert loops. Exposes `router()`, `start()`, `serve()` as library.
- `kanjo-cli` (all): the `kanjo` binary; thin clap front end.
- `web/`: Svelte + Vite single-page app, built to `web/dist`, embedded into server.

## Rules that are not negotiable

1. Prompt or response text never stored, logged or exported. Parser extracts usage numbers and metadata only. Quarantine keeps record's type and key signature, never the record. Transcript viewer reads file on demand, off by default, every view audited.
2. No outbound network traffic. No HTTP client crate in dependency tree (`reqwest`, `ureq`, `hyper` client, `curl`, ...). No telemetry, update checks or price downloads. Notifications local only.
3. Filesystem locations come from `kanjo_core::paths::Paths`. Tests and demos use temporary directories; no test reads or writes real `~/.claude` or real data directory.
4. Rows enter `requests`, `sessions`, `usage_hourly` only through `kanjo_core::store::write`, which keeps the three consistent. Only other code changing them: deletion in `kanjo_ops::retention` (retention and purge on request). Other tables written by the crate owning the feature, inside `Store::write`.
5. JSON shapes live in `kanjo_core::api` only. Changing them: regenerate bindings (`scripts/gen-bindings.sh`), update UI.
6. Timestamps epoch milliseconds UTC everywhere. Local-time bucketing uses `kanjo_core::Ctx`; functions take `&Ctx`, never read the clock, so tests deterministic.
7. Unknown model is unpriced (`cost_usd` NULL, counted in `unpriced_requests`), never free, never guessed.
8. Library code returns `Result`; no `unwrap`/`expect`/`panic!` outside tests, no `unsafe` without written justification. Blocking database work never on async runtime threads (`spawn_blocking`).
9. Fixtures synthetic. No real project names, paths, account ids or e-mail addresses in repository.
10. Schema changes only by adding migration to `MIGRATIONS` in `kanjo-core/src/store/mod.rs`.

## Style

Rust 2021, `cargo fmt` (max width 140 via rustfmt.toml), `cargo clippy --all-targets -- -D warnings` clean. Comments only where code cannot say it; doc comments on public items forming a contract. Errors use `kanjo_core::Error` or crate-local `thiserror` enum converting into it. Logging through `tracing`, never `println!` in libraries. User-facing text in English and Polish.

## Commands

- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
- `scripts/gen-bindings.sh` regenerates `web/src/lib/api/bindings` and `contract.ts` after change to `api.rs`
- `cd web && npm ci && npm run build` builds UI into `web/dist`
- `cargo run -p kanjo -- demo` serves synthetic data without touching `~/.claude`

## How a request becomes a change

Defaults with reasons, not a script. Task size and risk decide how much applies; judgment is yours.

**Read the request for its goal.** Request often a list mixing decisions, small edits, whole features. Item readable two ways: state your reading in one sentence, proceed on it. Before building anything with lasting cost (new repository, third-party submission, secret, release step, dependency): name cost and lightest shape meeting the goal, build that one. Machinery built here from literal reading of a list was removed, then rebuilt smaller. Collect maintainer-only decisions into one numbered list, recommendation for each; keep working on everything independent of them.

**Scale to the task.** Change in one area: inline. Parallel agents or workflow pay off when parts independent: each part own files or own worktree, shared context in one brief file, diff read by reviewer that did not write it. Review depth follows risk: parser, store, server, installers, release workflow, dependencies get `invariant-reviewer` or security review; wording fix does not. Single agent's research finding is unverified until you checked it; say so when passing it on unchecked.

**Branch, verify, land.** Work on branch cut from current `origin/main`. Branch in current working tree is default; separate worktree only when something else writes to same tree at same time (another session, or your own parallel agents). In doubt: check `git status` and `git worktree list`, decide. Uncommitted changes not yours: leave untouched, report. One commit per topic or backlog entry. Run `preflight` for what changed, push branch, open pull request early: CI is only place change runs on macOS and Windows. Watch CI until green, fix what it finds. In maintainer's session agent pushes without asking; where maintainer said session lands its own work, move `main` to green branch with fast-forward push (`git push origin <branch>:main`), keeping signed commits and linear history. Rewriting published history, tags, releases, publishing stay with maintainer: prepare everything, hand over exact command.

**Clean up after yourself.** Before reporting, remove what task created and no longer needs: worktrees, local and remote branches whose commits are in `main`, background processes (demo and dev servers, watchers), temporary directories and scratch files outside repository, stashes, superseded pull requests. Notes the change made stale (backlog entry, documentation sentence, memory note) are same cleanup. Only what the task itself created: other sessions work in this repository, so branch, directory or process you did not create is named in report and left alone.

**Report.** Short, in maintainer's language: what changed, what verified with what result, what not verified and why, what done without being asked, open decisions. Figures measured, not estimated. Step you are able and permitted to do is done, not handed back.

## Backlog and decisions

`BACKLOG.md` is only place for plans, ideas and their status. Lists only work meant to be done. Before proposing a feature or choosing next work, read it. New idea, or bug not fixed in current change: under Proposed with next ID; only maintainer triages. Pull request shipping an entry deletes it, adds CHANGELOG line when user would notice the change, rewrites documentation it makes untrue. Reasons for choices in force: `docs/decisions.md`. Documentation describes what exists; `scripts/check-docs.sh` enforces that wording.

## Where things live

- Architecture and embedding the server: `docs/architecture.md`
- HTTP API: `docs/api.md`; commands: `docs/cli.md`; configuration: `docs/configuration.md`
- Features: `docs/features.md`; specification coverage: `docs/feature-status.md`
- Platforms checked and known limitations: `docs/status.md`
- Install, upgrade and uninstall: `docs/install.md`
- Release process, signing and verifying a download: `docs/release.md`; steps, including README figures, in `release-prep` skill
- Supported versions, vulnerability reports and signing keys: `SECURITY.md`
- Threats, stored data and ISO 27001 mapping: `docs/security/threat-model.md`, `docs/security/data-inventory.md`, `docs/security/iso27001.md`
- Pull request flow, CI and repository protection: `CONTRIBUTING.md`, section Pull requests
- Changes a user would notice: entry under Unreleased in `CHANGELOG.md`
- Web UI design system: `web/README.md`

`spec.md`, when present in local clone, is private working document: may be read, never quoted, copied or committed. Maintainer creates release tags and pushes them; agents prepare release with `release-prep`.

## Agent configuration

`.claude/rules/`: short rules, each scoped to one part of tree; load when matching file is opened. `.claude/skills/`: procedures: `preflight` (checks CI runs), `api-change`, `add-migration`, `parser-fixture`, `run-demo` (also retakes screenshots), `backlog-wave` (several backlog entries shipped in one pass), manually invoked `update-pricing` and `release-prep`. `.claude/agents/invariant-reviewer.md` reviews diff against rules above. `.claude/skills/security-audit`: third-party skill copied in unchanged, pinned in `skills-lock.json`; do not edit. `.claude/settings.json`: shared permissions and list of plugins project uses, installed by `scripts/claude-plugins.sh`; no hooks and no MCP servers committed, none added without issue discussion first. Run `preflight` before reporting change as done.
