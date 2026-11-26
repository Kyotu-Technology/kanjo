---
name: release-prep
description: Prepare a Kanjo release commit (version, changelog, checks). Stops before tagging; the maintainer tags and pushes.
argument-hint: <x.y.z>
disable-model-invocation: true
---

# Preparing a release

`docs/release.md` is the reference. This skill prepares the release commit for version `$ARGUMENTS` and stops there. It never creates a tag, pushes or starts a workflow: pushing the `vx.y.z` tag is what publishes, and that belongs to the maintainer.

1. Check the starting point: the working tree is clean, the branch is up to date with `main`, and the last CI run on `main` is green (`gh run list --branch main --limit 5`). Stop and report if not.
2. Set `version` in `[workspace.package]` of the root `Cargo.toml`, then run `cargo check --workspace` once so `Cargo.lock` records the new version of the workspace crates and nothing else (the release build uses `--locked`). The crates refer to each other with `version = "..."`; update those as well when the version in them no longer matches.
3. In `CHANGELOG.md`, move the Unreleased entries under a `## [x.y.z] - YYYY-MM-DD` heading and leave an empty Unreleased section. The release workflow copies that section into the release notes and fails without it.
4. Remove status notes that stop being true with this release from `README.md`, `README.pl.md` and `SECURITY.md` ("not released yet" and the like), and update `docs/feature-status.md`.
5. Run the `preflight` skill with every group.
6. Show the diff and propose a one-line commit message. Then tell the maintainer what remains for them: merge to `main`, wait for CI, then `git tag vx.y.z` on that commit and `git push origin vx.y.z`. The first job of the release workflow refuses a tag that differs from the workspace version.
