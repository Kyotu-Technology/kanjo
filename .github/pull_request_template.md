## What and why

<!-- What this change does and the reason for it. Link the issue it closes, if any. -->

## How it was tested

<!-- Commands you ran and what they showed. -->

## Checklist

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass
- [ ] UI changes: `npm run check`, `npm test` and `npm run build` pass in `web/`
- [ ] No prompt or response text is stored, logged or exported, and no new dependency can make outbound connections
- [ ] Fixtures and examples are synthetic (no real project names, paths, account ids or e-mail addresses)
- [ ] JSON shape changes are made in `kanjo_core::api` and the TypeScript bindings are regenerated
- [ ] Schema changes are a new migration, not an edit of an existing one
- [ ] CHANGELOG.md has an entry under Unreleased for user-visible changes
