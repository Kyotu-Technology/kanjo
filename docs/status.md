# Status, limitations and roadmap

This page says which platforms have been checked and how, what a user may run into, and what is planned. How far the tree covers the functional specification, feature by feature, is in [feature-status.md](feature-status.md).

Kanjo is an independent open-source project published by Kyotu Technology, not a product of Anthropic. "Claude" and "Claude Code" are Anthropic's names for its products.

Kanjo is at version 0.1.0. This is what has been checked, and how:

| Platform | State | How it was verified |
| --- | --- | --- |
| Linux x86_64 | Built and tested | Test suite; manual runs in temporary directories; during development, an import of about 7 GB of real transcripts in under 10 seconds, with token totals cross-checked against a separate `jq` computation and the price table checked against the costs Claude Code records itself; a static musl build made and run in an Alpine container, packaged with `scripts/release-package.sh`, installed from the archive by `install.sh` and started with `kanjo up` in a scratch home (each step well under a second); the `strace` phase of the egress test; `scripts/test-install.sh` |
| Linux aarch64 | Defined in the release workflow | Not yet built |
| macOS (Intel and Apple silicon) | Tested in CI on Apple silicon | The test suite passes on GitHub's macOS 15 runner, and release binaries for both architectures compile and link (`scripts/cross-check.sh macos`). The install script, `kanjo up` and the background service have not been run on a Mac by hand |
| Windows x86_64 | Installed and run on Windows 11 in a virtual machine | `scripts/windows-vm-test.sh` installs a real Windows 11 (evaluation image, Windows PowerShell 5.1) and runs the whole path there: `install.ps1` from the packaged zip, `kanjo up` with the logon entry and the database key in the Windows credential store, the dashboard, the status line, live file watching, the reports and `kanjo uninstall`. `scripts/cross-check.sh windows` lints the workspace for Windows and runs the test suite under Wine. The binary in the virtual machine was the `x86_64-pc-windows-gnu` cross-build; the MSVC build is compiled, tested and checked for runtime dependencies by CI on Windows Server 2025 |

The background service and `kanjo uninstall` have been run for real only on Windows 11, in the virtual machine test, which also keeps the database key in the Windows credential store. That run used `install.ps1` as it was before its rework for piped use; the reworked installers have so far run only in the test scripts on Linux. On Linux and macOS that code is covered by tests that never call `systemctl` or `launchctl`, and the keyring backends there were tested only through a stand-in. CI runs the test suite on Ubuntu, on macOS 15 (Apple silicon) and on Windows Server 2025 (MSVC build), together with the zero-egress test and the dependency checks. The release workflow (signing, SBOMs, attestations) has not been executed yet, so there is no release to download.

## Known limitations

These are the limitations a user may run into:

- Kanjo is for one person on one machine. There is no team view yet.
- History reaches only as far back as the transcripts on disk. Claude Code deletes old transcripts (after 30 days unless `cleanupPeriodDays` says otherwise), so install Kanjo early; from then on it keeps its own copy.
- The transcript format is undocumented. After a Claude Code update, records Kanjo does not recognise are set aside rather than guessed at; `kanjo doctor` and the System page list them.
- Usage beyond the plan ("extra usage") is not tracked; only the spend limit that gateways report is shown.
- The command line and the API export seven views; the Limits, Insights and Projects pages export their data as JSON from the browser.
- Breakdowns by git branch, session, skill, MCP server, tool and request source are read from individual request rows, so they reach back only as far as request retention keeps them (400 days by default).
- Claude Code does not document its request sources. Only the main conversation (`repl_main_thread`) and compaction (`compact`) are confirmed from real records. The dashboard's names "Helper agent" (values starting with `agent`) and "Session title" (values containing `title`) are guesses; other values are shown as recorded, and requests without a source as "Not recorded".
- Hourly totals are stored per UTC hour. In a time zone with a fractional offset, such as UTC+5:30 or UTC+5:45, a day or bucket edge built from them is approximate to the hour. Cost-spike detection reads individual requests and stays exact.
- Kanjo writes UTF-8. Windows PowerShell 5.1 decodes the output of a program it captures or pipes with the console code page, so Polish letters in captured output come out wrong there unless that code page is UTF-8 (`chcp 65001`).
- Browsers installed as Snap or Flatpak packages may be unable to read the sign-in page in the data directory (or, for the demo, in the temporary directory). Then `kanjo open` and `kanjo up` print the address after 20 seconds. This has not been tested with a real sandboxed browser.
- In `kanjo demo`, the first-run guide still offers a Connect step, which the demo server refuses.
- There is no command to rotate the access token yet.
- The minisign key for releases has not been published, so the install scripts verify signatures only with cosign.

## Planned

From the specification, and not built yet:

- A team server in the LAN: the same binary in server mode, collecting aggregates from each machine and OpenTelemetry (OTLP) data from Claude Code, with SSO and roles, team budgets and chargeback per client. Prompt text would still never leave a developer's machine.
- A terminal UI for use over SSH.
- A Docker image for the team server.
- A desktop edition as a Tauri shell around the same server and web UI. [docs/desktop.md](desktop.md) describes the seams that already exist for it.
