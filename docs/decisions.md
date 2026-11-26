# Decisions

This page records the decisions that are in force and the reasons for them, one short entry each, so that a settled question is not reopened without a new reason. Plans and ideas are in [BACKLOG.md](../BACKLOG.md).

## D-001 Notifications stay local (2026-11-26)

Context: alerts could be sent by webhook or e-mail, but rule 2 of CLAUDE.md forbids outbound network traffic.
Decision: alerts are delivered only locally, as a desktop notification, a dashboard toast and the terminal bell (`crates/kanjo-alerts/src/notify.rs`). There is no webhook or SMTP delivery.
Consequences: `deny.toml` bans the `lettre` SMTP client together with every HTTP client crate, and `scripts/egress-test.sh` checks that the binary connects to nothing but loopback.

## D-002 Releases are signed with cosign keyless and carry attestations and SBOMs (2026-11-26)

Context: a downloaded binary has to be traceable to this repository's release workflow without a long-lived signing key.
Decision: the `sign` job of `.github/workflows/release.yml` signs `SHA256SUMS` with Sigstore cosign in keyless mode, and a GitHub build provenance attestation covers every archive, CycloneDX SBOM and installer; each target has its own SBOM. No release is signed with minisign or any other long-lived key.
Consequences: the installers check the archive against `SHA256SUMS` and, when cosign is installed, check the signature against the release workflow's identity; [release.md](release.md) shows how to verify a download by hand.

## D-003 No package-manager machinery in the release workflow or this repository (2026-11-26)

Context: the release workflow once generated a Homebrew formula and WinGet manifests and attested them as release assets.
Decision: commit f195feb removed the generators, their packing script, their tests, the CI step and the tap workflow template, so a release needs nothing beyond the GitHub release and the two installers. A release job does not write to a tap or to `microsoft/winget-pkgs`, because that would put a credential able to write to another repository into the release path.
Consequences: a package-manager channel is maintained outside this repository, in its own tap or through submissions to `microsoft/winget-pkgs`, and the release workflow stays unchanged by it.

## D-004 TypeScript stays on 6 (2026-11-26)

Context: `svelte-check`, which runs in `npm run check`, declares TypeScript `^5.0.0 || ^6.0.0` as its peer range.
Decision: `web/package.json` keeps TypeScript on `~6.0` until `svelte-check` supports a newer major.
Consequences: `.github/dependabot.yml` ignores TypeScript major updates, so no update is proposed that the web checks would reject.

## D-005 @types/node stays on 24 (2026-11-26)

Context: CI runs the web UI on Node 24 (`NODE_VERSION` in `.github/workflows/ci.yml`).
Decision: `@types/node` stays on the major that matches that Node version, so the types describe the runtime that runs the code.
Consequences: `.github/dependabot.yml` ignores `@types/node` major updates; the major moves together with the Node version in CI.

## D-006 BSL-1.0 is allowed for xxhash-rust (2026-11-26)

Context: `notify` 9, the file watcher of `kanjo-ingest`, depends on `xxhash-rust`, which is under the Boost Software License 1.0.
Decision: `deny.toml` allows BSL-1.0 for `xxhash-rust` only. The licence is permissive, approved by the OSI, and does not require the notice in binary distributions.
Consequences: any other crate under BSL-1.0 still fails `cargo deny check` until it gets its own exception with a reason.
