# Changelog

All notable changes to this project are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/). The release workflow copies the section of the tagged version into the GitHub release notes, so every release needs a `## [x.y.z] - YYYY-MM-DD` heading here before it is tagged.

## [Unreleased]

### Added

- Continuous integration: formatting, clippy, tests on Linux, macOS and Windows, web UI checks, cargo-deny, cargo-audit and a zero-egress test.
- Release pipeline producing static Linux (musl), macOS and Windows archives with SHA256SUMS, cosign and minisign signatures, build provenance attestations and CycloneDX SBOMs.
- Installers for Linux and macOS (`install/install.sh`) and Windows (`install/install.ps1`).
