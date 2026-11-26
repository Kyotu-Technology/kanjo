---
paths:
  - ".github/**"
---

# GitHub workflows and repository files

- Every action is pinned to a full commit SHA with the version in a trailing comment (`uses: owner/action@<sha> # vX.Y.Z`). The repository refuses unpinned actions. Dependabot keeps the pins current.
- The workflow default is `permissions: contents: read` (or `{}`), and each job declares its own. Only the release `sign` job gets an OIDC token and attestation rights, and only `publish` can write repository contents.
- Checkouts use `persist-credentials: false`. Every job has `timeout-minutes`.
- CI runs on `pull_request`, never `pull_request_target`, and references no secrets, so fork pull requests run with a read-only token.
- Never interpolate `${{ github.event.* }}` or another attacker-controlled value directly into a `run:` script; pass it through `env:`.
- The release workflow uses no caches, so nothing written by a pull request run can reach a release. Tools are installed at exact versions.
- Workflows call the scripts in `scripts/` instead of repeating their logic, so the same check runs locally and in CI.
