# Changelog

All notable changes to this project are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/). The release workflow copies the section of the tagged version into the GitHub release notes, so every release needs a `## [x.y.z] - YYYY-MM-DD` heading here before it is tagged.

## [Unreleased]

## [0.2.0] - 2026-11-26

### Added

- `kanjo token rotate` replaces the access token with a new random one and records the rotation in the audit log. A running server keeps accepting the old token, and the browsers signed in with it, until it restarts; the command says whether a server is running and how to restart it.

### Changed

- Plans follow Anthropic's current line-up: Max 20x is now Ultra, the $100 plan is Max, and the Team plan's seats are Team Standard and Team Premium with their current list prices of $25 and $125 a month per seat, billed monthly. In the settings and plan-fit JSON the plan values `max5x` and `max20x` became `max` and `ultra`; settings saved by 0.1.0 are read as before.
- A verified Ultra account has no 5-hour limit, so with Ultra chosen in Settings the plan advice can suggest Max on the weekly peak alone, and Kanjo no longer estimates a 5-hour limit in the dashboard, the Limits page, `kanjo status` or `limits.estimate` of the API.
- The plan advice moves Team seats between Standard and Premium, as it moves individual plans between Pro, Max and Ultra.
- Under Homebrew, `kanjo up`, `kanjo service install` and the dashboard's connect step record the `<prefix>/bin/kanjo` link instead of the versioned Cellar path for the statusline, the hooks and the background service, so the recorded path does not point into a keg that `brew upgrade` and `brew cleanup` remove.
- Each release archive now holds its files in `kanjo-<target>/` instead of `kanjo-<version>-<target>/`, so the binary's path inside an unpacked archive stays the same from one release to the next. The archive file names do not change, and both installers still install archives of either layout.

### Removed

- Releases are no longer prepared for a minisign signature. No release was ever signed with minisign; a release is verified with its cosign signature and build provenance attestations. The release workflow has no minisign step, the installers' verification summary has no minisign line, and the installers ignore `KANJO_MINISIGN_PUBKEY`.

## [0.1.0] - 2026-11-26

### Added

- Kanjo, an offline cost and limit monitor for Claude Code, shipped as one `kanjo` binary that contains the collector, a local HTTP API and the web dashboard. It reads Claude Code's files on the same machine and sends nothing over the network: no telemetry, no update checks, no price downloads.
- Collection: on first start Kanjo imports the existing transcript history, then follows new requests live. The statusline and session hooks add the official 5-hour and 7-day limit readings, cost and session state. Records in a format Kanjo does not know are set aside by type, never with their content, and `kanjo doctor` lists them.
- Usage reports by day, week, month and session, in the dashboard and as `kanjo daily`, `weekly`, `monthly` and `session` (also with `--json`), filtered by project, model, agent and client. They include the token breakdown with both cache-write lifetimes, the API-equivalent cost next to the subscription price, attribution to subagents, skills, MCP servers and tools, a split by request source, the part of Claude Code that sent each request (the main conversation and compaction are confirmed values; others are shown as recorded), an activity heatmap and a comparison of periods. Local days, weeks and months follow the system time zone, daylight saving changes included; hourly totals are stored per UTC hour, so in a zone with a fractional offset such as UTC+5:30 a day edge built from them is approximate to the hour. Every figure is labelled as official, an estimate or a lower bound.
- Limits: 5-hour blocks and the weekly limit with a forecast of when each runs out, the cost of one percent of the weekly limit, estimates when no statusline data exists, the spend limit, and a check whether the current plan fits the usage.
- Alerts for limit thresholds, forecast exhaustion, budgets (for all usage, a project or a client, per day, week or month), cost spikes, runaway sessions, agent loops and a cold cache, shown as desktop notifications and in the dashboard, and as a terminal bell when the server runs in a terminal.
- Insights: cache hit ratio with the causes of misses and the configuration changes that broke the cache, the most expensive sessions and turns, model mix suggestions and long-context warnings.
- A native statusline for Claude Code (model, cost, limits, reset times, cache) that can be chained with an existing one.
- `kanjo up` sets everything up in one step: it finds the Claude Code directory, asks before it changes `settings.json` and backs it up first, registers a background service (a systemd user unit, a LaunchAgent, or a per-user start entry on Windows) and opens the dashboard. `kanjo doctor` checks the installation.
- Dashboard with live updates, a "Demo data" marker in `kanjo demo`, dark and light themes, a mobile layout, Ctrl/Cmd+K search, English and Polish, CSV and JSON export, and a transcript viewer that is off by default and audits every use.
- Privacy and security: prompt and response text is never stored. The database is encrypted with SQLCipher, with its key in the OS keyring (Secret Service on Linux, the Keychain on macOS, Credential Manager on Windows) or in a key file. The server listens on loopback only and needs an access token; the browser is signed in with a one-time login code that reaches it through an owner-only local page, never on the command line of the program that opens the browser. A hash-chained audit log, retention settings and purge, and redaction of project names and paths in reports and exports.
- Operations: a built-in price table with signed offline price imports, schema migrations that take a backup first, `kanjo backup` and `kanjo restore`, and `kanjo demo` with synthetic data.
- A release workflow that builds binaries for Linux (static musl builds, x86_64 and aarch64), macOS (Intel and Apple silicon) and Windows (x86_64, with the C runtime linked in, so no Visual C++ redistributable is needed). Each release has a `SHA256SUMS` file signed with Sigstore cosign, build provenance attestations, CycloneDX SBOMs, and the dependency list embedded in every binary.
- Installers for Linux and macOS (`install.sh`) and Windows (`install.ps1`). They check the archive against `SHA256SUMS` and, when cosign is installed, check that `SHA256SUMS` was signed by this repository's release workflow for that version. A failed check stops the installation, and a skipped check is reported with its reason. `--skip-signature` (`-SkipSignature`) turns the signature checks off explicitly. When the install directory is not on `PATH`, the installer prints the full command to run and how to add the directory for bash, zsh, fish and PowerShell.
- One-line installs from the latest release, `curl -fsSL .../install.sh | sh` and `irm .../install.ps1 | iex`, which run safely from a pipe and leave nothing behind in the PowerShell session.
