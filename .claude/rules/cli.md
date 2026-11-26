---
paths:
  - "crates/kanjo-cli/**"
  - "docs/cli.md"
---

# Command line

- A command or flag is declared in `src/cli.rs` (clap) and dispatched in `src/main.rs`. Shell completions are generated from that declaration by `kanjo completions`, so there is nothing to update by hand.
- Text a command prints exists in English and Polish, through `Lang::t` or `tr!` from `src/i18n.rs`. Clap help text is English only.
- `--json` output uses types from `kanjo_core::api`; changing it follows the `api-change` skill.
- A new or changed command is tested in `tests/cli.rs`, whose `Env` runs the binary against a temporary home and Claude directory. Commands that touch Claude Code's settings or the background service are never run against the real home.
- The same change updates the row in `docs/cli.md`, `README.md` and `README.pl.md` where they name the command, and the Unreleased section of `CHANGELOG.md`.
