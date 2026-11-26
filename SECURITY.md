# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub security advisories: open the Security tab of [the repository][repo], choose "Advisories" and then "Report a vulnerability". The report reaches the maintainers only. Do not open a public issue, pull request or discussion for a suspected vulnerability.

A useful report says which version you ran (`kanjo --version`), on which operating system, what you did and what happened, and whether you believe the issue is exploitable by someone other than the local user. Do not attach real Claude Code transcripts, project names, account identifiers or database files; a synthetic reproduction is enough and keeps your own data out of the report.

We aim to acknowledge a report within five working days and to agree on a disclosure date with the reporter. Fixes are released as a new patch version, and the advisory is published together with the release that contains the fix. Reporters are credited unless they ask not to be.

## Supported versions

Kanjo is before 1.0. Only the most recent release receives security fixes; there are no backports to older release lines. Once 1.0 ships, this section will say which release lines are supported and for how long.

## What the project promises

Kanjo sends no telemetry and makes no outbound network connections. There are no update checks, crash reports or price downloads; the price table is compiled into the binary. This is enforced in two places. `deny.toml` makes cargo-deny reject HTTP, SMTP, DNS and update client crates and the client features of hyper, so they cannot enter the dependency tree. `scripts/egress-test.sh` runs the built binary in demo mode inside a network namespace that has only loopback and under `strace`, and fails if any `connect()` targets something other than loopback or a local socket. Both are jobs in the CI workflow, which runs on every pull request; it has not run yet, because the repository has not been published. If you observe Kanjo opening a non-loopback connection, that is a vulnerability under this policy.

Releases are built by the public GitHub Actions workflow in `.github/workflows/release.yml`, from the tagged commit, with `--locked` dependencies. Every release publishes a `SHA256SUMS` file signed with Sigstore cosign (keyless, bound to that workflow) and, when the project's minisign key is configured, a minisign signature. Every archive has a GitHub build provenance attestation and a CycloneDX SBOM, and each binary embeds its own dependency list through cargo-auditable. `install.sh` and `install.ps1` check the cosign signature when cosign is installed and stop when it does not match this repository's release workflow and the version being installed; without cosign they say that the signature was not checked and rely on the checksum. The minisign public key has not been published yet, so the installers report the minisign check as skipped. The release workflow has not been executed yet, so no release, signature, attestation or SBOM exists so far. `docs/release.md` explains how to verify all of this by hand.

[repo]: https://github.com/kyotu/kanjo
