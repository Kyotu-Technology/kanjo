# Contributing to Kanjo

Thank you for helping. Kanjo is an offline cost and limit monitor for Claude Code, published by Kyotu under the Apache License 2.0. By submitting a contribution you agree that it is licensed under the same terms (section 5 of [LICENSE](LICENSE)). Security problems are not handled here; please follow [SECURITY.md](SECURITY.md) instead.

## Building and testing

You need a stable Rust toolchain (the workspace declares its minimum version in `Cargo.toml`), a C compiler and Perl, because the database layer builds SQLCipher and a vendored OpenSSL from source. On Windows that means the MSVC build tools and a native Perl such as Strawberry Perl; the MSYS perl that comes with Git does not work for the OpenSSL build. The first build compiles the C code and takes a couple of minutes; later builds are incremental. The web UI needs Node.js and npm.

The server embeds the built UI from `web/dist`, so build it once before building the binary:

```sh
(cd web && npm ci && npm run build)
cargo build -p kanjo
cargo run -p kanjo -- demo
```

`kanjo demo` serves synthetic data and does not read your real Claude Code directory, which makes it the right way to try a change by hand. If you only work on Rust code, `scripts/ci-web-stub.sh` puts a placeholder page in `web/dist` instead of a full UI build.

Before opening a pull request, run what CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
(cd web && npm run check && npm test && npm run build)
cargo deny check
scripts/cargo-audit.sh
scripts/egress-test.sh target/debug/kanjo
sh scripts/test-install.sh
sh scripts/test-packaging.sh
```

`cargo deny` needs [cargo-deny](https://github.com/EmbarkStudios/cargo-deny), and `scripts/cargo-audit.sh` needs [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit) and git; it audits against a fresh clone of the advisory database instead of `~/.cargo/advisory-db`. `scripts/test-install.sh` tests `install/install.sh` with stub tools in a temporary directory; on Windows, `scripts/test-install.ps1` does the same for `install.ps1`. `scripts/test-packaging.sh` runs the Homebrew formula and WinGet manifest generators and the packing step of the release workflow against a synthetic `SHA256SUMS`; it skips the Ruby syntax check without `ruby` and the packing check without `zip` and `unzip`. The egress test needs Linux, `strace`, `unshare` and `ip`; it skips with a message when they are missing, and CI runs it in strict mode. `scripts/cross-check.sh` needs Docker and builds what a Linux machine cannot run natively: a static musl binary run in Alpine, a Windows build run under Wine together with the test suite, and macOS binaries that are compiled and linked but cannot be executed there. `scripts/windows-vm-test.sh` goes one step further for Windows: it needs KVM, installs a real Windows 11 in a throwaway virtual machine and runs the installer, `kanjo up`, the dashboard and `kanjo uninstall` there (about half an hour).

## Working with Claude Code and other coding agents

Kanjo is developed with coding agents, and the repository carries the configuration for them so that an agent starts with the same rules as a person. Nothing here is required: every check can be run by hand, and a contribution written with an agent is reviewed exactly like any other. Whoever opens the pull request is its author and answers for all of it.

[CLAUDE.md](CLAUDE.md) holds the project instructions and is loaded in every session; [AGENTS.md](AGENTS.md) points other tools to it. `.claude/rules/` holds short rules for one part of the tree each (the web UI, the store, the API contract, the parser, the server, dependencies, workflows, scripts, documentation), loaded only when a matching file is opened. `.claude/skills/` holds procedures that Claude Code offers as slash commands: `/preflight` runs the checks CI runs, `/api-change`, `/add-migration` and `/parser-fixture` walk through the three changes that are easiest to get half right, `/run-demo` starts the app on synthetic data, and `/update-pricing` and `/release-prep` are maintainer procedures that only run when typed. `.claude/agents/invariant-reviewer.md` is a reviewer that edits nothing and checks a diff against the rules in the next section. Claude Code's built-in `/code-review` and `/security-review` need no installation and are the recommended baseline before a pull request.

`.claude/settings.json` is deliberately small. It lets the agent run the build, test and lint commands without asking, makes it ask before pushing, tagging, releasing, adding a dependency or reading a real Claude Code transcript under `~/.claude/projects`, and denies reading `.env` files. It commits no hooks and no MCP servers. Hooks in a repository run on a contributor's machine as soon as the folder is opened, and an MCP server is one more program with network access; neither fits a project whose promise is that nothing leaves the machine. Personal preferences go into `.claude/settings.local.json`, which is ignored by git. A pull request that changes anything under `.claude/` is reviewed like code.

The settings enable two plugins from Anthropic's own marketplace, the language servers for Rust and TypeScript. A committed entry does not download anything; install them once per clone, with the server binaries on your `PATH`:

```sh
rustup component add rust-analyzer
npm install -g typescript typescript-language-server
claude plugin install rust-analyzer-lsp@claude-plugins-official --scope project
claude plugin install typescript-lsp@claude-plugins-official --scope project
```

Other plugins are useful here but are not enabled by the repository, because a plugin runs with your privileges and a marketplace follows its upstream branch. Install the ones you want at user scope and review what they contain first.

| Plugin | Source | What it adds | Worth knowing |
| --- | --- | --- | --- |
| `svelte@svelte` | `sveltejs/ai-tools` (official, MIT) | Svelte 5 skills, a Svelte file editor agent and the `svelteserver` language server | Also starts an MCP server with `npx -y @sveltejs/mcp`, an unpinned package; disable it if you only want the skills |
| `rust-review@trailofbits` | `trailofbits/skills` (CC-BY-SA-4.0) | A security review of Rust code: panics that can be triggered from outside, concurrency, the unsafe boundary | Runs bundled Python scripts to plan the review and write its report, and uses several agents, so it costs more than a plain review |
| `differential-review@trailofbits`, `insecure-defaults@trailofbits`, `sharp-edges@trailofbits` | `trailofbits/skills` | A security-focused review of a diff, of configuration defaults and of error-prone APIs | Good before a change to `kanjo-server` or `kanjo-setup` |
| `security-audit` skill | `cloudflare/security-audit-skill` (MIT) | A full, multi-phase security audit with a written report | Not a plugin: copy `skills/security-audit` into `~/.claude/skills/`. Meant for an occasional deep audit, not for every change |
| `frontend-design@claude-plugins-official` | Anthropic | Guidance for visual design work | The design system in `web/README.md` wins where the two disagree |

Add a third-party marketplace with `/plugin marketplace add <owner>/<repo>` and install from it with `/plugin install <plugin>@<marketplace>`.

Two rules apply to agents more than to people. Do not give an agent a real transcript, not even your own, to explain a parser bug or to build a fixture; describe the record's keys and value types instead. And do not let an agent run `kanjo up` or `kanjo serve` to try a change; `kanjo demo` exists for that.

## Rules every change must follow

These come from the project's design and are not open to case-by-case exceptions; a pull request that breaks one will not be merged however useful it is otherwise.

Kanjo never stores, logs or exports prompt or response text. Parsers extract usage numbers and metadata only, and anything that cannot be parsed is recorded by its type and key signature, never by its content. The transcript viewer reads files on demand, is off by default and audits every view.

Kanjo makes no outbound network connections: no telemetry, no update checks, no price downloads, and notifications stay on the local machine. No HTTP client crate may enter the dependency tree. `deny.toml` bans the known client stacks and CI runs the egress test, but please do not rely on the tooling alone; if a crate you want to add can dial out, it is the wrong crate.

Filesystem locations come from `kanjo_core::paths::Paths`. Tests and demos use temporary directories, and nothing in a test may read or write the real `~/.claude` or the real data directory.

The request, session and hourly usage tables are written only through `kanjo_core::store::write`. Other tables are written by the crate that owns the feature. The schema changes only by adding a migration to `MIGRATIONS` in `crates/kanjo-core/src/store/mod.rs`; existing migrations are never edited.

JSON shapes live in `kanjo_core::api` and nowhere else. When you change one, regenerate the TypeScript bindings and `web/src/lib/api/contract.ts` with `scripts/gen-bindings.sh` and update the UI in the same pull request.

Timestamps are epoch milliseconds in UTC. Code that needs the current time or the local time zone takes a `&Ctx` instead of reading the clock, so tests stay deterministic.

A model without a price is unpriced: its cost stays empty and it is counted as unpriced. It is never treated as free and its price is never guessed.

Library code returns `Result`. There is no `unwrap`, `expect` or `panic!` outside tests and no `unsafe` without a written justification. Blocking database work runs in `spawn_blocking`, not on the async runtime threads.

Fixtures and examples are synthetic. Do not commit real project names, paths, account ids, e-mail addresses or transcript text, not even your own.

## Style

Rust code is formatted with `cargo fmt` (the width is set in `rustfmt.toml`) and must be clippy-clean with warnings denied. Comments explain what the code cannot say; public items that form a contract get doc comments. Errors use `kanjo_core::Error` or a crate-local `thiserror` enum that converts into it, and libraries log through `tracing`, never `println!`. User-facing text exists in English and Polish, so a new message needs both. Code, comments and documentation are written in English.

## Adding or updating the price table

Prices are compiled into the binary from `assets/pricing.json`, which is generated from a local copy of LiteLLM's `model_prices_and_context_window.json` by `scripts/gen-pricing.sh`. The script needs `jq` and keeps only Anthropic models whose name starts with `claude-`:

```sh
scripts/gen-pricing.sh path/to/model_prices_and_context_window.json 2026-11-25
cargo test -p kanjo-core
```

The second argument becomes the table's version; use the date of the source file. Download the source file yourself; Kanjo and its build never fetch it. In the pull request, say where the source came from and which models changed, and check the diff of `assets/pricing.json` by hand, because a wrong price silently changes every report. A model that is missing from the table stays unpriced, which is the intended behaviour, so do not add placeholder prices.

## Adding a parser fixture

Parser fixtures are small JSONL files that imitate Claude Code transcript records. Write them by hand or derive them from the structure of a real record, but never copy one: leave every message text out or empty (Kanjo does not need it, and a fixture that contains text suggests otherwise), and invent the session ids, project names, paths, request ids and timestamps. Keep the token counts and model names that matter for the case you are testing. Put the fixture next to the ingest tests that load it, give it a name that says what it covers (for example a duplicated message, a sidechain or an unknown record type), and add a test that asserts the parsed usage numbers, not just that parsing succeeds. A fixture for a record type Kanjo does not understand should assert that the record lands in quarantine by type and key signature.

## Pull requests

Keep a pull request to one topic, describe what it changes and why, and say how you tested it. Add an entry under Unreleased in [CHANGELOG.md](CHANGELOG.md) for anything a user would notice. CI must be green before review; the workflow runs on forks without access to any secrets.
