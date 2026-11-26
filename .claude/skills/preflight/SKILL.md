---
name: preflight
description: Run the checks CI runs, scoped to what changed, before a commit or a pull request. Use when asked to verify a change, before reporting work as done, or when CI failed and the failure needs reproducing locally.
---

# Preflight

Run the same checks as `.github/workflows/ci.yml`, from the repository root. Look at `git status --short` and `git diff --name-only` first and pick the groups that apply; when unsure, run them all.

The server embeds `web/dist`. If that directory has no `index.html`, run `scripts/ci-web-stub.sh` before any cargo command, or do a real UI build.

**Rust code** (`crates/**`, `Cargo.toml`):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

**Web UI** (`web/**`, or after `scripts/gen-bindings.sh`), in `web/`; run `npm ci` first if `node_modules` is missing:

```sh
npm run check
npm test
npm run build
```

**Dependencies** (`Cargo.toml`, `Cargo.lock`, `deny.toml`):

```sh
cargo deny check advisories bans licenses sources
scripts/cargo-audit.sh
```

**API types** (`crates/kanjo-core/src/api.rs`, `settings.rs`): run `scripts/gen-bindings.sh`, then `git status --short web/src/lib/api` must show nothing that is not part of the change.

**Installers and scripts** (`install/**`, `scripts/**`):

```sh
shellcheck -s sh install/install.sh scripts/*.sh
sh scripts/test-install.sh
```

**Anything that could open a connection** (server, ingest, setup, a new dependency), Linux only, after `cargo build --locked -p kanjo`:

```sh
scripts/egress-test.sh target/debug/kanjo
```

The egress test skips with a message when `strace`, `unshare` or `ip` is missing; a skip is not a pass.

Report each command with its result. Name every check that was skipped or could not run and why (a missing tool, another platform). Do not describe the change as verified when a check that applies to it did not run.
