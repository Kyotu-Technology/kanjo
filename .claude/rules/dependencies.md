---
paths:
  - "**/Cargo.toml"
  - "Cargo.lock"
  - "deny.toml"
  - "web/package.json"
  - "web/package-lock.json"
---

# Dependencies

- No crate that can make an outbound connection may enter the dependency tree: no HTTP, SMTP or DNS client, no updater, no client TLS stack, no client features of hyper. `deny.toml` bans the known ones, but the rule is wider than the list. Check a new crate's own dependencies and default features before adding it (`cargo tree -p <crate> -e features`).
- Prefer no new dependency. When one is needed, turn off default features and enable only what is used, and say in the pull request why the crate is needed and what else was considered.
- After any change run `cargo deny check` and `scripts/cargo-audit.sh`. Builds and tests use `--locked`, so commit `Cargo.lock` with the change.
- A licence exception or an ignored advisory in `deny.toml` is scoped to one crate and carries a comment that states the reason.
- The MSRV is `rust-version` in the root `Cargo.toml`; CI checks the workspace with exactly that toolchain. Do not add a `rust-toolchain.toml`: it would override the toolchain that job selects.
- The web UI has `devDependencies` only. Nothing may be loaded from a CDN at run time.
