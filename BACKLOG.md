# Backlog

This file is the only place that holds plans and ideas; the documentation describes what exists.
It lists work that is meant to be done: Now is in the order it should be done, and Later waits for the trigger named in each entry. An idea that is dropped is deleted, not kept.
To add an entry, put it under Proposed with the ID after "Last ID used" and raise that line by one. IDs are never reused.
Only the maintainer triages: a Proposed entry moves to Now or Later, or is deleted. The section an entry is in is its status.
The pull request that ships an entry deletes it, adds a line under Unreleased in CHANGELOG.md when a user would notice the change, and rewrites the documentation sentence it makes untrue.
Reasons for choices in force are in docs/decisions.md.
Sizes: S up to half a day, M one to two days, L one to two weeks, XL a month or more.

Last ID used: K-070

## Now

Accepted work, in the order it should be done.

### K-032 Windows session with the published release
Size: M.
Why: the Windows virtual machine run used the installer before its rework and the GNU build, not the published MSVC zip, and WinGet has not been tried; the same session shows whether the path that kanjo up records survives an upgrade (K-069).
Done when: scripts/windows-vm-test.sh has run with the published MSVC zip and the reworked install.ps1, a winget install from a local manifest (written with komac or by hand) has run non-elevated and elevated with the recorded path noted, and docs/status.md states the results.

### K-069 Stable recorded path on Windows if the archive layout is not enough
Size: S. Depends: K-032.
Why: if the Windows session shows that the path kanjo up records still changes on winget upgrade, the unversioned archive layout alone does not fix it.
Done when: the recorded path is mapped to a stable one in Rust as kanjo_setup does for the Homebrew prefix link, with a test, or the session has shown it is not needed and the entry is deleted.

### K-006 Create the tap repository
Size: S.
Why: the Homebrew channel needs its own public tap repository, outside this repository (D-003).
Done when: Kyotu-Technology/homebrew-tap is public and holds the formula generator, a README and the update workflow (which verifies each archive with gh attestation verify, runs brew style, audit, install and test, and needs no secret from this repository), Actions may write its contents, a ruleset blocks force pushes and deletion, the workflow has run dry against v0.1.0, and after v0.2.0 (K-007) a real run has published the formula.

### K-007 Release v0.2.0 through a release candidate
Size: S.
Why: v0.1.0 lacks the path fixes, so a channel built on it breaks on the first upgrade; v0.1.0 is not an immutable release because it was published before immutable releases were switched on.
Done when: v0.2.0-rc.1 and v0.2.0 are prepared with release-prep and published, the candidate has exercised the new archive layout end to end, and the v0.2.0 release reports itself immutable.

### K-008 First WinGet submission
Size: S. Depends: K-007, K-032.
Why: the first version of a WinGet package needs the Microsoft Contributor License Agreement and a new-package review, so it cannot be automated.
Done when: a pull request made with komac for KyotuTechnology.Kanjo is open in microsoft/winget-pkgs, the zip has passed gh attestation verify and its InstallerSha256 matches SHA256SUMS. If validation rejects the unsigned zip, K-033 moves to triage.

### K-009 Documentation for each live install channel
Size: S. Depends: K-006, K-008.
Why: the documentation describes a channel only once it exists.
Done when: for each live channel, docs/install.md (upgrade, then kanjo service install; the order of removal), docs/release.md (the sentence that Kanjo is not published to a package manager, and an "After a release" checklist), both READMEs, CHANGELOG.md, docs/security/iso27001.md and the trust points of the threat model describe it.

### K-026 Close the CodeQL alerts in test and mock code
Size: S.
Why: the five alerts were all in test or mock code; the tag-stripping expressions in web/src/csp.test.ts and the error body of the mock were changed, and the alerts close only after a CodeQL run on main.
Done when: after that run, alerts 1-3 and 5 are closed or dismissed with a reason, and alert 4 (random latency in the mock) is dismissed as used in tests.

### K-029 Finish verifying the published release by hand on Linux
Size: S.
Why: the cosign signature has been verified by hand and by install.sh, but the provenance attestations have not, and the systemd user service and the sign-in from a sandboxed browser have not run for real.
Done when: gh attestation verify has run against the published release, the systemd user service and the sign-in from a Snap or Flatpak browser have run in a virtual machine with a desktop, and docs/status.md states the results.

### K-031 macOS session
Size: M. Depends: K-006.
Why: the published release has not been installed by hand on macOS, and the session shows whether the mapping to the Homebrew prefix link is enough there.
Done when: install.sh, kanjo up, the LaunchAgent, the keychain and uninstall have run on macOS, a brew install from the tap has run with the recorded path noted, after v0.2.0 brew upgrade has kept that path, and docs/status.md states the results.

## Proposed

Waiting for triage; same entry form. There are no proposed entries.

## Later

Same entry form plus a Trigger line that says what would make the entry worth doing.

### K-011 Automated WinGet updates from CI
Size: S. Depends: K-008.
Why: each WinGet update after the first is a manual komac run.
Done when: a new release reaches microsoft/winget-pkgs without a manual step.
Trigger: the manual update has proven a burden over three releases.

### K-023 Rule file for alert changes
Size: S.
Why: no file in .claude/rules applies to the alerts crate, and the files a new alert kind touches (rules.rs, api.rs, settings, UI, both dictionaries, docs/api.md) are found only by search.
Done when: .claude/rules/alerts.md lists them for a new alert kind.
Trigger: the first alert change (K-038 or K-049).

### K-033 Windows Authenticode signing
Size: S.
Why: the Windows binary is not Authenticode-signed; WinGet is not known to require it, and the first submission (K-008) shows whether validation accepts an unsigned zip.
Done when: the release signs kanjo.exe and docs/release.md says so.
Trigger: WinGet validation or SmartScreen blocking users, or a company asking for signed binaries.

### K-034 Run the aarch64 Linux binary on real hardware
Size: S.
Why: the aarch64 Linux binary has been started only once, with --version, on the release runner.
Done when: the binary has run kanjo up and the dashboard on aarch64 hardware and docs/status.md says so.
Trigger: a bug report or a request from an aarch64 Linux user.

### K-035 Apple Developer ID signing and notarization
Size: S.
Why: the macOS binaries are not signed or notarized; a formula install is not quarantined, so nothing requires it today.
Done when: the release signs and notarizes the macOS binaries and docs/release.md says so.
Trigger: a cask, a .pkg, a desktop app, or a company requiring signed binaries.

### K-038 Estimated cost of cache misses in Insights
Size: M.
Why: Insights shows the cache hit ratio and the causes of misses but no dollar figure, which is the cheapest real waste insight.
Done when: Insights shows an estimated cost of cache misses, labelled as an estimate, with unpriced models excluded.
Trigger: the first feature slot after v0.2.0.

### K-039 Self-contained HTML report
Size: M.
Why: exports are CSV and JSON only; one HTML report with print CSS covers PDF output, anonymised snapshots and a per-client statement without a PDF engine.
Done when: a command writes a self-contained HTML report with a by-client month section and an option that forces redaction.
Trigger: a user asking to send a report to a client or a manager.

### K-040 Windows: warn about a data directory outside the user profile
Size: S.
Why: the permission helpers do nothing on Windows and rely on the profile's defaults.
Done when: kanjo doctor warns when the data directory is outside the user profile, and ACLs are set only if that turns out to be needed.
Trigger: a Windows user with a shared or roaming data directory.

### K-041 Confirm the guessed request source names
Size: S.
Why: of the request source names, only repl_main_thread and compact have been confirmed against real records.
Done when: each name is confirmed or corrected and docs/status.md no longer lists them as guessed.
Trigger: real transcript records with those values.

### K-042 Read-only terminal view
Size: M.
Why: kanjo status, the reports and the status line cover most needs, but there is no live view in the terminal; full parity with the web UI is not the aim.
Done when: kanjo top shows the live figures read-only.
Trigger: issues asking for it.

### K-043 Usage paid beyond the plan
Size: M.
Why: neither the statusline payload nor the transcripts carry usage paid beyond the plan, so Kanjo cannot show it.
Done when: Kanjo records and shows that usage from the source Claude Code provides.
Trigger: Claude Code exposes it.

### K-044 Automatic client assignment by path prefix
Size: M.
Why: the client field, the filter, per-client budgets and --by client exist, but each project is assigned by hand.
Done when: rules by path prefix, and optionally by git remote, assign projects to clients.
Trigger: a request from a freelancer or a team server.

### K-045 CLI and API export for the Limits, Insights and Projects pages
Size: M.
Why: those pages can be exported only as JSON from the browser.
Done when: the CLI and the API export them.
Trigger: a request.

### K-046 Prometheus metrics endpoint
Size: M.
Why: the only machine-readable endpoint for monitoring is /api/v1/health; an inbound scrape fits rule 2.
Done when: an authenticated metrics endpoint serves the main figures in the Prometheus format.
Trigger: a team server or a request.

### K-047 ISO 27001 evidence bundle command
Size: M.
Why: the pieces exist (audit verify and export, doctor, retention settings), but evidence from one laptop proves little.
Done when: one command writes them as a bundle.
Trigger: a company running Kanjo on many machines.

### K-048 Desktop edition as a Tauri shell
Size: XL. Depends: K-035, K-033.
Why: kanjo-server exposes router(), start() and serve() as a library, so a shell could host the same UI, but no shell crate exists.
Done when: a Tauri shell hosts the server and the UI and is built and signed by the release workflow.
Trigger: a decision to ship a desktop product.

### K-049 Alert command
Size: S.
Why: an alert could reach any channel the user chooses if Kanjo started a user-configured program with content-free alert JSON on stdin, as the statusline chain command already does.
Done when: the command runs on each alert, the threat model describes it, and the egress test runs with no command set.
Trigger: the maintainer writes down that rule 2 allows Kanjo to start a user-configured local program.

### K-058 OpenTelemetry receiver
Size: L. Depends: K-062.
Why: receiving OTLP/HTTP on the existing axum router fits rule 2; the records carry personal data such as user.email.
Done when: the receiver accepts OTLP/HTTP (prost and opentelemetry-proto, no tonic) and drops prompt fields before anything is stored.
Trigger: K-062 decided for a team mode.

### K-059 Docker image for the server
Size: M. Depends: K-061.
Why: the single-user tool reads ~/.claude locally, so an image is useful only for a team server.
Done when: the release publishes an image for the team server.
Trigger: a team server exists.

### K-060 Single sign-on with OIDC
Size: L. Depends: K-061.
Why: there is one access token or none; OIDC calls the identity provider, which is an outbound connection under rule 2.
Done when: a team server signs users in through OIDC.
Trigger: K-062 decided for a team mode and rule 2 amended for the identity provider call.

### K-061 Team server
Size: XL. Depends: K-062.
Why: the hourly aggregate (usage_hourly, per project, model and subagent type) and the read models in kanjo_core::api carry no conversation content and are the natural units to send and show, but an agent that pushes data to a server conflicts with rule 2 and the ban on hyper's client, and would need its own threat model.
Done when: a team server collects aggregates from each developer's Kanjo over mutual TLS, with central chargeback, team budgets and local roles, a write path that keeps rule 4, and a threat model.
Trigger: K-062 decided for a team mode and rule 2 amended for the push from each developer's machine.

### K-062 Decide whether Kanjo gets a team mode
Size: S.
Why: every team entry (K-058 to K-061) waits on this, and a tool that reports on developers' usage raises data-protection questions the decision has to answer.
Done when: the decision is recorded in docs/decisions.md.
Trigger: maintainer decision.

### K-065 Second maintainer with required release review
Size: L.
Why: main requires one approving review, which a sole maintainer can only bypass; the rule takes effect with a second maintainer.
Done when: a second maintainer reviews pull requests and releases.
Trigger: maintainer decision.

### K-066 Independent security review of the local dashboard
Size: L.
Why: an internal review and a threat model exist, but no outside review.
Done when: an outside reviewer has reviewed the dashboard and the findings are handled.
Trigger: maintainer decision.

### K-070 Bump @types/node when CI moves to a newer Node LTS
Size: S.
Why: @types/node is held on the Node major that CI uses (D-005).
Done when: @types/node matches the new major and the web checks pass.
Trigger: the Node version in ci.yml changes.
